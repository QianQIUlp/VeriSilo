//! The two local Managed slots. Each slot keeps the existing single-session
//! launcher, Profile lease, relay and health lock together under one owner.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    domain::{
        NetworkProfile, ProxyScheme, RuntimeActivation, RuntimeSessionStatus, RuntimeState, Silo,
        SiloExecutionTarget, LOCAL_MANAGED_SESSION_LIMIT,
    },
    engine::EngineAdapterId,
    launcher::{RuntimeManager, LOCAL_RUNTIME_RECORD_PREFIX},
    runtime_watchdog::RuntimeWatchdog,
    website_identity::WebsiteIdentityObservation,
};

struct LocalRuntimeSlot {
    runtime: Arc<Mutex<RuntimeManager>>,
    snapshot: Mutex<RuntimeSessionStatus>,
    reserved: AtomicBool,
    pending_vault_lock: AtomicBool,
    revoker_running: AtomicBool,
    watchdog: Mutex<Option<RuntimeWatchdog>>,
}

pub(crate) fn supports_managed_concurrency(silo: &Silo) -> bool {
    matches!(silo.execution_target, SiloExecutionTarget::Local)
        && silo.adapter_id() == EngineAdapterId::Camoufox
        && silo.network_profile.external_mihomo_binding().is_none()
        && matches!(
            &silo.network_profile,
            NetworkProfile::Direct {
                proxy_required: false
            } | NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Http | ProxyScheme::Socks5,
                ..
            }
        )
}

impl LocalRuntimeSlot {
    fn new(silo_id: Uuid, runtime: RuntimeManager) -> Self {
        let snapshot = RuntimeSessionStatus {
            silo_id,
            activation: runtime.cached_activation(),
            website_identity: runtime.website_identity(),
        };
        let occupied = runtime.has_runtime_ownership();
        let runtime = Arc::new(Mutex::new(runtime));
        let watchdog = occupied.then(|| {
            RuntimeWatchdog::start(&runtime)
                .expect("VeriSilo needs a native watchdog for each active Managed Silo")
        });
        Self {
            runtime,
            snapshot: Mutex::new(snapshot),
            reserved: AtomicBool::new(false),
            pending_vault_lock: AtomicBool::new(false),
            revoker_running: AtomicBool::new(false),
            watchdog: Mutex::new(watchdog),
        }
    }

    fn snapshot(&self) -> RuntimeSessionStatus {
        if let Ok(runtime) = self.runtime.try_lock() {
            let activation = runtime.cached_activation();
            let previous = self.snapshot.lock().unwrap().clone();
            if self.reserved.load(Ordering::Acquire)
                && previous.activation.state == RuntimeState::Preflight
                && activation.active_silo_id != Some(previous.silo_id)
            {
                return previous;
            }
            let status = RuntimeSessionStatus {
                silo_id: previous.silo_id,
                website_identity: (activation.active_silo_id.is_some()
                    && activation.state == RuntimeState::Running)
                    .then(|| runtime.website_identity())
                    .flatten(),
                activation,
            };
            *self.snapshot.lock().unwrap() = status.clone();
            status
        } else {
            self.snapshot.lock().unwrap().clone()
        }
    }

    fn occupied(&self) -> bool {
        if self.reserved.load(Ordering::Acquire) {
            return true;
        }
        match self.runtime.try_lock() {
            Ok(runtime) => runtime.has_runtime_ownership(),
            Err(_) => true,
        }
    }

    fn start_watchdog(&self) -> Result<(), String> {
        let mut watchdog = self
            .watchdog
            .lock()
            .map_err(|_| "Managed runtime watchdog state is unavailable.".to_owned())?;
        if watchdog.is_none() {
            *watchdog =
                Some(RuntimeWatchdog::start(&self.runtime).map_err(|error| {
                    format!("Managed runtime watchdog could not start: {error}")
                })?);
        }
        Ok(())
    }

    fn stop_watchdog_if_quiescent(&self) {
        if self.occupied() {
            return;
        }
        if let Ok(mut watchdog) = self.watchdog.lock() {
            if let Some(mut watchdog) = watchdog.take() {
                watchdog.shutdown();
            }
        }
    }

    fn update(
        &self,
        activation: RuntimeActivation,
        website_identity: Option<WebsiteIdentityObservation>,
    ) {
        let silo_id = self.snapshot.lock().unwrap().silo_id;
        let website_identity = (activation.active_silo_id == Some(silo_id)
            && activation.state == RuntimeState::Running)
            .then_some(website_identity)
            .flatten();
        *self.snapshot.lock().unwrap() = RuntimeSessionStatus {
            silo_id,
            activation,
            website_identity,
        };
    }

    fn revoke_if_pending(&self) {
        if !self.pending_vault_lock.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut runtime) = self.runtime.try_lock() {
            if self.pending_vault_lock.swap(false, Ordering::AcqRel) {
                let activation = runtime.revoke_secrets_for_vault_lock();
                drop(runtime);
                self.update(activation, None);
            }
        }
    }

    fn queue_vault_lock(self: &Arc<Self>) {
        self.pending_vault_lock.store(true, Ordering::Release);
        self.revoke_if_pending();
        if !self.pending_vault_lock.load(Ordering::Acquire)
            || self.revoker_running.swap(true, Ordering::AcqRel)
        {
            return;
        }
        let slot = Arc::clone(self);
        if thread::Builder::new()
            .name("verisilo-managed-vault-revoker".to_owned())
            .spawn(move || loop {
                let Ok(mut runtime) = slot.runtime.lock() else {
                    slot.revoker_running.store(false, Ordering::Release);
                    break;
                };
                if slot.pending_vault_lock.swap(false, Ordering::AcqRel) {
                    let activation = runtime.revoke_secrets_for_vault_lock();
                    drop(runtime);
                    slot.update(activation, None);
                }
                slot.revoker_running.store(false, Ordering::Release);
                if !slot.pending_vault_lock.load(Ordering::Acquire)
                    || slot.revoker_running.swap(true, Ordering::AcqRel)
                {
                    break;
                }
            })
            .is_err()
        {
            self.revoker_running.store(false, Ordering::Release);
        }
    }
}

/// Registry locks only protect slot admission and Arc lookup. Host launch,
/// stop, re-observation and health I/O lock the one target RuntimeManager.
pub(crate) struct LocalRuntimeSet {
    root: PathBuf,
    legacy: Arc<Mutex<RuntimeManager>>,
    legacy_recorded_silo_id: Mutex<Option<Uuid>>,
    legacy_pending_vault_lock: Arc<AtomicBool>,
    legacy_revoker_running: Arc<AtomicBool>,
    slots: Mutex<BTreeMap<Uuid, Arc<LocalRuntimeSlot>>>,
    load_error: Option<String>,
}

pub(crate) struct LocalRuntimeStartGuard<'a> {
    set: &'a LocalRuntimeSet,
    silo_id: Uuid,
    slot: Arc<LocalRuntimeSlot>,
}

impl LocalRuntimeStartGuard<'_> {
    pub(crate) fn runtime(&self) -> Arc<Mutex<RuntimeManager>> {
        Arc::clone(&self.slot.runtime)
    }
}

impl Drop for LocalRuntimeStartGuard<'_> {
    fn drop(&mut self) {
        self.slot.revoke_if_pending();
        if let Ok(runtime) = self.slot.runtime.try_lock() {
            self.set.update(
                self.silo_id,
                runtime.cached_activation(),
                runtime.website_identity(),
            );
        }
        self.slot.reserved.store(false, Ordering::Release);
        self.slot.stop_watchdog_if_quiescent();
    }
}

impl LocalRuntimeSet {
    pub(crate) fn open(root: &Path, legacy: Arc<Mutex<RuntimeManager>>) -> Self {
        let legacy_recorded_silo_id = legacy
            .lock()
            .ok()
            .and_then(|runtime| runtime.recorded_silo_id());
        let mut slots = BTreeMap::new();
        let mut load_error = None;
        match fs::read_dir(root.join("runtime")) {
            Ok(files) => {
                for file in files {
                    let file = match file {
                        Ok(file) => file,
                        Err(error) => {
                            load_error = Some(format!(
                                "Could not enumerate Managed runtime records: {error}"
                            ));
                            break;
                        }
                    };
                    let name = file.file_name().to_string_lossy().into_owned();
                    let Some(id_text) = name
                        .strip_prefix(LOCAL_RUNTIME_RECORD_PREFIX)
                        .and_then(|name| name.strip_suffix(".json"))
                    else {
                        continue;
                    };
                    let id = match Uuid::parse_str(id_text) {
                        Ok(id) => id,
                        Err(_) => {
                            load_error = Some(format!(
                                "Managed runtime record has an invalid Silo ID: {name}"
                            ));
                            continue;
                        }
                    };
                    slots.insert(
                        id,
                        Arc::new(LocalRuntimeSlot::new(
                            id,
                            RuntimeManager::open_for_silo(root, id),
                        )),
                    );
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                load_error = Some(format!("Could not read Managed runtime records: {error}"))
            }
        }
        Self {
            root: root.to_path_buf(),
            legacy,
            legacy_recorded_silo_id: Mutex::new(legacy_recorded_silo_id),
            legacy_pending_vault_lock: Arc::new(AtomicBool::new(false)),
            legacy_revoker_running: Arc::new(AtomicBool::new(false)),
            slots: Mutex::new(slots),
            load_error,
        }
    }

    pub(crate) fn reserve_start(&self, silo: &Silo) -> Result<LocalRuntimeStartGuard<'_>, String> {
        if !supports_managed_concurrency(silo) {
            return Err("Local Managed concurrency supports Camoufox with Direct or a required fixed HTTP/SOCKS5 proxy.".to_owned());
        }
        if let Some(error) = self.load_error.as_ref() {
            return Err(format!("Unresolved Managed runtime record: {error}"));
        }
        let mut slots = self
            .slots
            .lock()
            .map_err(|_| "Managed runtime slot state is unavailable.".to_owned())?;
        let legacy = self.legacy.try_lock().map_err(|_| {
            "The existing local runtime is busy; retry after its operation finishes.".to_owned()
        })?;
        if legacy.has_runtime_ownership() {
            return Err("The existing local runtime or recovery record must be resolved before starting a Managed concurrent session.".to_owned());
        }
        drop(legacy);
        let occupied = slots.values().filter(|slot| slot.occupied()).count();
        if let Some(slot) = slots.get(&silo.id) {
            if slot.occupied() {
                return Err(
                    "This Managed Silo is already starting, running, or requires recovery."
                        .to_owned(),
                );
            }
            if occupied >= LOCAL_MANAGED_SESSION_LIMIT {
                return Err(format!(
                    "At most {LOCAL_MANAGED_SESSION_LIMIT} local Managed Silos can run at once."
                ));
            }
            slot.reserved.store(true, Ordering::Release);
            if let Err(error) = slot.start_watchdog() {
                slot.reserved.store(false, Ordering::Release);
                return Err(error);
            }
            let slot = Arc::clone(slot);
            slot.update(
                RuntimeActivation {
                    active_silo_id: Some(silo.id),
                    state: RuntimeState::Preflight,
                    updated_at: Utc::now(),
                    message: Some("正在准备此 Managed Silo 的独立运行会话。".to_owned()),
                    browser_verification: None,
                    engine_evidence: None,
                    network_evidence: None,
                    identity_evidence: None,
                },
                None,
            );
            return Ok(LocalRuntimeStartGuard {
                set: self,
                silo_id: silo.id,
                slot,
            });
        }
        if occupied >= LOCAL_MANAGED_SESSION_LIMIT {
            return Err(format!(
                "At most {LOCAL_MANAGED_SESSION_LIMIT} local Managed Silos can run at once."
            ));
        }
        let slot = Arc::new(LocalRuntimeSlot::new(
            silo.id,
            RuntimeManager::open_for_silo(&self.root, silo.id),
        ));
        if slot.occupied() {
            slots.insert(silo.id, slot);
            return Err("This Managed Silo has an unresolved runtime record.".to_owned());
        }
        slot.reserved.store(true, Ordering::Release);
        slot.start_watchdog()?;
        slot.update(
            RuntimeActivation {
                active_silo_id: Some(silo.id),
                state: RuntimeState::Preflight,
                updated_at: Utc::now(),
                message: Some("正在准备此 Managed Silo 的独立运行会话。".to_owned()),
                browser_verification: None,
                engine_evidence: None,
                network_evidence: None,
                identity_evidence: None,
            },
            None,
        );
        slots.insert(silo.id, Arc::clone(&slot));
        Ok(LocalRuntimeStartGuard {
            set: self,
            silo_id: silo.id,
            slot,
        })
    }

    pub(crate) fn get(&self, silo_id: Uuid) -> Option<Arc<Mutex<RuntimeManager>>> {
        if let Ok(legacy) = self.legacy.try_lock() {
            if legacy.has_runtime_ownership()
                && (legacy.recorded_silo_id() == Some(silo_id)
                    || legacy.cached_activation().active_silo_id == Some(silo_id))
            {
                return Some(Arc::clone(&self.legacy));
            }
        } else if self
            .legacy_recorded_silo_id
            .lock()
            .ok()
            .is_some_and(|id| *id == Some(silo_id))
        {
            return Some(Arc::clone(&self.legacy));
        }
        if let Ok(slots) = self.slots.lock() {
            if let Some(slot) = slots.get(&silo_id) {
                return Some(Arc::clone(&slot.runtime));
            }
        }
        if let Ok(legacy) = self.legacy.try_lock() {
            if legacy.recorded_silo_id() == Some(silo_id)
                || legacy.cached_activation().active_silo_id == Some(silo_id)
            {
                return Some(Arc::clone(&self.legacy));
            }
        } else if self
            .legacy_recorded_silo_id
            .lock()
            .ok()
            .is_some_and(|id| *id == Some(silo_id))
        {
            return Some(Arc::clone(&self.legacy));
        }
        None
    }

    pub(crate) fn note_legacy_owner(&self, silo_id: Uuid) {
        if let Ok(mut owner) = self.legacy_recorded_silo_id.lock() {
            *owner = Some(silo_id);
        }
    }

    pub(crate) fn entries(&self) -> Vec<(Uuid, Arc<Mutex<RuntimeManager>>)> {
        self.slots
            .lock()
            .map(|slots| {
                slots
                    .iter()
                    .map(|(id, slot)| (*id, Arc::clone(&slot.runtime)))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(crate) fn snapshots(&self) -> Vec<RuntimeSessionStatus> {
        let slots = self
            .slots
            .lock()
            .map(|slots| slots.values().cloned().collect::<Vec<_>>());
        slots
            .unwrap_or_default()
            .into_iter()
            .map(|slot| {
                let snapshot = slot.snapshot();
                slot.stop_watchdog_if_quiescent();
                snapshot
            })
            .collect()
    }

    pub(crate) fn snapshot_for(&self, silo_id: Uuid) -> Option<RuntimeSessionStatus> {
        self.slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(&silo_id).cloned())
            .map(|slot| slot.snapshot())
    }

    pub(crate) fn update(
        &self,
        silo_id: Uuid,
        activation: RuntimeActivation,
        website_identity: Option<WebsiteIdentityObservation>,
    ) {
        let slot = self
            .slots
            .lock()
            .ok()
            .and_then(|slots| slots.get(&silo_id).cloned());
        if let Some(slot) = slot {
            slot.update(activation, website_identity);
            slot.stop_watchdog_if_quiescent();
        }
    }

    pub(crate) fn is_in_use(&self, silo_id: Uuid) -> bool {
        if self.load_error.is_some() {
            return true;
        }
        let slot_occupied = match self.slots.lock() {
            Ok(slots) => slots.get(&silo_id).is_some_and(|slot| slot.occupied()),
            Err(_) => return true,
        };
        if slot_occupied {
            return true;
        }
        if let Ok(legacy) = self.legacy.try_lock() {
            if legacy.record_unreadable() {
                return true;
            }
            if legacy.recorded_silo_id() == Some(silo_id)
                || legacy.cached_activation().active_silo_id == Some(silo_id)
            {
                return legacy.has_runtime_ownership();
            }
        } else if self
            .legacy_recorded_silo_id
            .lock()
            .ok()
            .is_some_and(|id| *id == Some(silo_id))
        {
            return true;
        } else {
            // A busy legacy manager may have started a new Silo since open.
            // A target edit must wait rather than guess its ownership.
            return true;
        }
        false
    }

    pub(crate) fn slot_occupied(&self, silo_id: Uuid) -> bool {
        self.slots
            .lock()
            .map(|slots| slots.get(&silo_id).is_some_and(|slot| slot.occupied()))
            .unwrap_or(true)
    }

    pub(crate) fn has_occupied(&self) -> bool {
        self.load_error.is_some()
            || self
                .slots
                .lock()
                .map(|slots| slots.values().any(|slot| slot.occupied()))
                .unwrap_or(true)
    }

    pub(crate) fn all_quiescent(&self) -> bool {
        !self.has_occupied()
    }

    /// Auto-lock can happen while a second Host is launching. Flag every
    /// session first, then revoke each one as soon as its own lock is free.
    pub(crate) fn revoke_all_for_vault_lock(&self) {
        let slots = self
            .slots
            .lock()
            .map(|slots| slots.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for slot in slots {
            slot.queue_vault_lock();
        }
        self.legacy_pending_vault_lock
            .store(true, Ordering::Release);
        if let Ok(mut runtime) = self.legacy.try_lock() {
            if self.legacy_pending_vault_lock.swap(false, Ordering::AcqRel) {
                runtime.revoke_secrets_for_vault_lock();
            }
            return;
        }
        if self.legacy_revoker_running.swap(true, Ordering::AcqRel) {
            return;
        }
        let legacy = Arc::clone(&self.legacy);
        // The legacy path is exclusive. Its existing long operation can finish
        // before revocation without making either Managed slot wait for it.
        let pending = Arc::clone(&self.legacy_pending_vault_lock);
        let running = Arc::clone(&self.legacy_revoker_running);
        if thread::Builder::new()
            .name("verisilo-legacy-vault-revoker".to_owned())
            .spawn(move || loop {
                let Ok(mut runtime) = legacy.lock() else {
                    running.store(false, Ordering::Release);
                    break;
                };
                if pending.swap(false, Ordering::AcqRel) {
                    runtime.revoke_secrets_for_vault_lock();
                }
                running.store(false, Ordering::Release);
                if !pending.load(Ordering::Acquire) || running.swap(true, Ordering::AcqRel) {
                    break;
                }
            })
            .is_err()
        {
            self.legacy_revoker_running.store(false, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ExternalMihomoBinding, SCHEMA_VERSION};
    use crate::engine::SiloEngineConfig;

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "verisilo-local-runtime-set-test-{}",
                Uuid::new_v4()
            ));
            fs::create_dir_all(root.join("runtime")).expect("create isolated runtime root");
            Self(root)
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn silo(root: &Path, id: Uuid, network_profile: NetworkProfile) -> Silo {
        Silo {
            id,
            schema_version: SCHEMA_VERSION,
            name: id.to_string(),
            color: "#4f46e5".to_owned(),
            browser: None,
            execution_target: SiloExecutionTarget::Local,
            profile_directory: root
                .join("silos")
                .join(id.to_string())
                .join("profiles")
                .to_string_lossy()
                .into_owned(),
            network_profile,
            engine: SiloEngineConfig::Camoufox {
                identity_template: None,
                fallback_rules: Vec::new(),
                artifact_binding: None,
            },
            seed_reference: Uuid::new_v4(),
            created_at: Utc::now(),
            identity_locked_at: None,
            archived_at: None,
        }
    }

    fn direct(root: &Path) -> Silo {
        silo(
            root,
            Uuid::new_v4(),
            NetworkProfile::Direct {
                proxy_required: false,
            },
        )
    }

    fn set(root: &Path) -> LocalRuntimeSet {
        LocalRuntimeSet::open(root, Arc::new(Mutex::new(RuntimeManager::open(root))))
    }

    fn write_record(root: &Path, silo_id: Uuid, legacy: bool, state: &str) {
        let filename = if legacy {
            "browser-session.json".to_owned()
        } else {
            format!("{LOCAL_RUNTIME_RECORD_PREFIX}{silo_id}.json")
        };
        let now = Utc::now();
        let value = serde_json::json!({
            "siloId": silo_id,
            "pid": u32::MAX,
            "startedAt": now,
            "lastSeenAt": now,
            "state": state,
        });
        fs::write(root.join("runtime").join(filename), value.to_string())
            .expect("write minimal runtime record");
    }

    #[test]
    fn managed_admission_reserves_exactly_two_independent_silos() {
        let root = TestRoot::new();
        let set = set(&root.0);
        let a = direct(&root.0);
        let b = direct(&root.0);
        let c = direct(&root.0);
        let first = set.reserve_start(&a).expect("reserve A");
        assert_eq!(
            set.snapshot_for(a.id).unwrap().activation.state,
            RuntimeState::Preflight
        );
        assert!(
            set.reserve_start(&a).is_err(),
            "duplicate start must be rejected"
        );
        let second = set.reserve_start(&b).expect("reserve B while A starts");
        assert!(
            set.reserve_start(&c).is_err(),
            "third session must be rejected"
        );
        assert!(set.is_in_use(a.id));
        assert!(set.is_in_use(b.id));
        drop(first);
        let third = set.reserve_start(&c).expect("A released its reservation");
        drop(third);
        drop(second);
        assert!(set.all_quiescent());
    }

    #[test]
    fn fixed_proxy_is_eligible_but_mihomo_remains_exclusive() {
        let root = TestRoot::new();
        let mut fixed = silo(
            &root.0,
            Uuid::new_v4(),
            NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Socks5,
                host: "127.0.0.1".to_owned(),
                port: 1080,
                bypass_list: Vec::new(),
                credential_reference: None,
                external_mihomo: None,
            },
        );
        assert!(supports_managed_concurrency(&fixed));
        if let NetworkProfile::FixedProxy {
            external_mihomo, ..
        } = &mut fixed.network_profile
        {
            *external_mihomo = Some(ExternalMihomoBinding {
                controller_url: "http://127.0.0.1:9090".to_owned(),
                selector_group: "GLOBAL".to_owned(),
                node_name: "node-a".to_owned(),
                controller_secret_reference: None,
            });
        }
        assert!(!supports_managed_concurrency(&fixed));
        assert!(set(&root.0).reserve_start(&fixed).is_err());
    }

    #[test]
    fn per_silo_recovery_and_unreadable_records_block_admission() {
        let root = TestRoot::new();
        let a = direct(&root.0);
        let b = direct(&root.0);
        write_record(&root.0, a.id, false, "running");
        fs::write(
            root.0
                .join("runtime")
                .join(format!("{LOCAL_RUNTIME_RECORD_PREFIX}{}.json", b.id)),
            "{broken",
        )
        .expect("write unreadable recovery record");
        let set = set(&root.0);
        assert!(set.is_in_use(a.id));
        assert!(set.is_in_use(b.id));
        assert!(!set.all_quiescent());
        assert!(set.reserve_start(&a).is_err());
        assert!(set.reserve_start(&b).is_err());
    }

    #[test]
    fn live_legacy_owner_takes_precedence_over_old_stopped_slot() {
        let root = TestRoot::new();
        let a = direct(&root.0);
        write_record(&root.0, a.id, false, "stopped");
        write_record(&root.0, a.id, true, "running");
        let set = set(&root.0);
        let owner = set.get(a.id).expect("target runtime owner");
        assert!(Arc::ptr_eq(&owner, &set.legacy));
        assert!(set.is_in_use(a.id));
        assert!(set.reserve_start(&a).is_err());
        assert_eq!(
            set.snapshot_for(a.id).unwrap().activation.state,
            RuntimeState::Stopped
        );
    }

    #[test]
    fn busy_a_does_not_block_b_snapshot_or_ownership() {
        let root = TestRoot::new();
        let set = set(&root.0);
        let a = direct(&root.0);
        let b = direct(&root.0);
        let first = set.reserve_start(&a).expect("reserve A");
        let second = set.reserve_start(&b).expect("reserve B");
        let a_runtime = first.runtime();
        let _a_busy = a_runtime.lock().expect("hold only A runtime");
        assert_eq!(
            set.snapshot_for(b.id).unwrap().activation.state,
            RuntimeState::Preflight
        );
        assert!(set.is_in_use(b.id));
        assert_eq!(
            set.snapshot_for(a.id).unwrap().activation.state,
            RuntimeState::Preflight
        );
        drop(_a_busy);
        drop(first);
        drop(second);
    }
}
