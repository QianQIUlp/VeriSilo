use std::sync::TryLockError;
use std::{fs, path::PathBuf};

use chrono::Utc;
use uuid::Uuid;

use super::environments::{verified_current_wsl_artifact, LocalEnvironmentControl};
use super::identity::{
    managed_identity_generation_error, managed_launcher_error, managed_launcher_failure,
    managed_vault_error,
};
use crate::domain::{
    BrowserDescriptor, BrowserKind, CreateSiloInput, NetworkProfile, Silo, SiloExecutionTarget,
    SCHEMA_VERSION,
};
use crate::engine::EngineError;
use crate::launcher::LauncherError;
use crate::vault::{VaultError, VaultRuntime};

#[test]
fn independent_core_roots_keep_vault_sessions_and_reopen_separate() {
    use super::{
        desktop_status_with, initialize_vault_with, lock_vault_with, unlock_vault_with, DesktopCore,
    };
    use crate::domain::VaultLockState;

    let root = temporary_root("independent-cores");
    let first_root = root.join("first");
    let second_root = root.join("second");
    fs::create_dir_all(&first_root).unwrap();
    fs::create_dir_all(&second_root).unwrap();
    let resources = root.join("no-engine-resources");
    {
        let first = DesktopCore::open(first_root.clone(), resources.clone());
        let second = DesktopCore::open(second_root, resources.clone());
        initialize_vault_with(&first, "first test passphrase").unwrap();
        initialize_vault_with(&second, "second test passphrase").unwrap();
        lock_vault_with(&first).unwrap();
        assert!(matches!(
            desktop_status_with(&first).unwrap().vault.state,
            VaultLockState::Locked
        ));
        assert!(matches!(
            desktop_status_with(&second).unwrap().vault.state,
            VaultLockState::Unlocked
        ));
    }
    {
        let reopened = DesktopCore::open(first_root, resources);
        assert!(unlock_vault_with(&reopened, "second test passphrase").is_err());
        assert!(matches!(
            unlock_vault_with(&reopened, "first test passphrase")
                .unwrap()
                .state,
            VaultLockState::Unlocked
        ));
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn listing_engine_adapters_does_not_rediscover_stock_browsers() {
    let source = include_str!("engines.rs");
    let start = source
        .find("fn list_engine_adapters(")
        .expect("list_engine_adapters");
    let body = source
        .get(start..start.saturating_add(1600))
        .expect("list_engine_adapters body");
    assert!(
        !body.contains("discover_installed_browsers"),
        "adapter listing must not spawn Chrome/Edge probes"
    );
}

fn temporary_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("verisilo-lib-{label}-{}", Uuid::new_v4()))
}

#[test]
fn silo_diagnosis_scopes_provider_requests_and_evidence() {
    use super::{diagnose_silo_with, initialize_vault_with, DesktopCore};
    use crate::domain::{ExternalMihomoBinding, ProxyScheme};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // Fail before exercising the production path if local discovery is reintroduced:
    // this test must never probe a developer's real Clash installation.
    assert!(!include_str!("silos.rs").contains("diagnose_local_clash"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let root = temporary_root("diagnostic-scope");
    fs::create_dir_all(&root).unwrap();
    let browser = root.join("chrome.exe");
    fs::write(&browser, []).unwrap();
    fs::write(
        browser.with_extension("version-output"),
        "Google Chrome 126.0.6478.127\n",
    )
    .unwrap();
    {
        let core = DesktopCore::open(root.clone(), root.join("resources"));
        initialize_vault_with(&core, "synthetic diagnostic passphrase").unwrap();
        let fixed = |binding| NetworkProfile::FixedProxy {
            proxy_required: true,
            scheme: ProxyScheme::Socks5,
            host: "127.0.0.1".to_owned(),
            port: 7890,
            bypass_list: vec![],
            credential_reference: None,
            external_mihomo: binding,
        };
        let profiles = [
            NetworkProfile::Direct {
                proxy_required: false,
            },
            fixed(None),
            NetworkProfile::Pac {
                proxy_required: false,
                pac_url: "https://example.test/qa.pac".to_owned(),
            },
            fixed(Some(ExternalMihomoBinding {
                controller_url: format!("http://{address}/"),
                selector_group: "QA group".to_owned(),
                node_name: "QA node".to_owned(),
                controller_secret_reference: None,
            })),
        ];
        let mut silos = Vec::new();
        for (index, network_profile) in profiles.into_iter().enumerate() {
            let bound = network_profile.external_mihomo_binding().is_some();
            silos.push(
                core.vault
                    .lock()
                    .unwrap()
                    .create_silo(
                        &root,
                        CreateSiloInput {
                            name: format!("QA silo {index}"),
                            color: "#5b5ce2".to_owned(),
                            browser_kind: BrowserKind::Chrome,
                            executable_path: browser.to_string_lossy().into_owned(),
                            execution_target: SiloExecutionTarget::Local,
                            network_profile,
                            engine: Default::default(),
                            proxy_credentials: None,
                            mihomo_controller_secret: bound.then(|| {
                                crate::domain::MihomoControllerSecretInput {
                                    secret: "synthetic-controller-secret".to_owned(),
                                }
                            }),
                        },
                    )
                    .unwrap(),
            );
        }
        for silo in &silos[..3] {
            let diagnosis = diagnose_silo_with(&core, silo.id).unwrap();
            assert!(diagnosis.get("clash").is_none());
            assert_eq!(
                diagnosis["network"],
                serde_json::to_value(&silo.network_profile).unwrap()
            );
            assert!(diagnosis["runtimeState"].is_null());
            assert!(diagnosis["runtimeMessage"].is_null());
            assert_eq!(diagnosis["active"], false);
            assert_eq!(diagnosis["vault"], "unlocked");
        }
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
        let server = std::thread::spawn(move || {
            for (path, body) in [
                (
                    "/configs",
                    r#"{"mode":"rule","socks-port":7890,"mixed-port":0,"secret":"unrelated-account"}"#,
                ),
                (
                    "/proxies/QA%20group",
                    r#"{"type":"Selector","now":"QA node","all":["QA node","unrelated-account-node"]}"#,
                ),
                (
                    "/proxies/QA%20node",
                    r#"{"type":"Socks5","alive":true,"history":[{"delay":42}],"account":"unrelated-account"}"#,
                ),
            ] {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "missing synthetic request"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
                assert!(request.contains("Authorization: Bearer synthetic-controller-secret\r\n"));
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
            listener
        });
        let diagnosis = diagnose_silo_with(&core, silos[3].id).unwrap();
        let listener = server.join().unwrap();
        assert_eq!(diagnosis["clash"]["mode"], "rule");
        assert_eq!(diagnosis["clash"]["groups"][0]["nodes"][0]["delayMs"], 42);
        assert_eq!(diagnosis["clash"]["groups"].as_array().unwrap().len(), 1);
        assert_eq!(
            diagnosis["clash"]["groups"][0]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(!diagnosis.to_string().contains("unrelated-account"));
        assert!(!diagnosis
            .to_string()
            .contains("synthetic-controller-secret"));
        // A synthetic stale environment record for a different local Silo fails
        // reconciliation before any environment backend is contacted.
        {
            let mut environment = core.environment_runtime.lock().unwrap();
            environment.activation.active_silo_id = Some(silos[1].id);
            environment.activation.state = crate::domain::RuntimeState::RecoveryRequired;
            environment.activation.message = Some("QA other Silo provider error".to_owned());
            environment.wsl_distribution = Some("QA-stale-distribution".to_owned());
        }
        let direct = diagnose_silo_with(&core, silos[0].id).unwrap();
        assert!(direct["runtimeState"].is_null());
        assert!(direct["runtimeMessage"].is_null());
        assert_eq!(
            core.environment_runtime
                .lock()
                .unwrap()
                .activation
                .message
                .as_deref(),
            Some("QA other Silo provider error"),
            "another Silo must not be reconciled by diagnose"
        );
        let related = diagnose_silo_with(&core, silos[1].id).unwrap();
        assert_eq!(related["runtimeState"], "recovery_required");
        assert!(related["runtimeMessage"].as_str().is_some());
        let direct = diagnose_silo_with(&core, silos[0].id).unwrap();
        assert!(direct["runtimeState"].is_null());
        assert!(direct["runtimeMessage"].is_null());
        assert!(direct.get("clash").is_none());
        assert!(!direct.to_string().contains("QA node"));
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn diagnosis_attributes_global_stopped_evidence_only_to_the_silo_that_ran() {
    use super::{diagnose_silo_with, initialize_vault_with, unlock_vault_with, DesktopCore};

    let root = temporary_root("diagnose-stopped-attribution");
    fs::create_dir_all(&root).unwrap();
    let browser = root.join("chrome.exe");
    fs::write(&browser, []).unwrap();
    fs::write(
        browser.with_extension("version-output"),
        "Google Chrome 126.0.6478.127\n",
    )
    .unwrap();
    let passphrase = "diagnose stopped attribution passphrase";
    let (ran_id, fresh_id) = {
        let core = DesktopCore::open(root.clone(), root.join("resources"));
        initialize_vault_with(&core, passphrase).unwrap();
        let input = |name: &str| CreateSiloInput {
            name: name.to_owned(),
            color: "#5b5ce2".to_owned(),
            browser_kind: BrowserKind::Chrome,
            executable_path: browser.to_string_lossy().into_owned(),
            execution_target: SiloExecutionTarget::Local,
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
            engine: Default::default(),
            proxy_credentials: None,
            mihomo_controller_secret: None,
        };
        let mut vault = core.vault.lock().unwrap();
        let ran = vault.create_silo(&root, input("ran then stopped")).unwrap();
        let fresh = vault.create_silo(&root, input("never started")).unwrap();
        (ran.id, fresh.id)
    };
    // Persist the minimal record of the Silo that actually ran and then
    // stopped, exactly as the desktop runtime leaves it after a converged
    // session. `active_silo_id` is consequently None for both Silos.
    let record = format!(
        r#"{{"siloId":"{ran_id}","pid":424242,"startedAt":"2026-09-08T00:00:00Z","lastSeenAt":"2026-09-08T00:01:00Z","state":"stopped"}}"#
    );
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(root.join("runtime").join("browser-session.json"), record).unwrap();
    {
        let core = DesktopCore::open(root.clone(), root.join("resources"));
        unlock_vault_with(&core, passphrase).unwrap();
        // The never-started Silo has no attributable runtime evidence and must
        // not inherit the global stopped presentation (QA-R1-03).
        let fresh_diagnosis = diagnose_silo_with(&core, fresh_id).unwrap();
        assert!(
            fresh_diagnosis["runtimeState"].is_null(),
            "never-started Silo must not inherit the global stopped state: {fresh_diagnosis}"
        );
        assert!(fresh_diagnosis["runtimeMessage"].is_null());
        assert_eq!(fresh_diagnosis["active"], false);
        // The Silo that really ran keeps its attributable stopped evidence.
        let ran_diagnosis = diagnose_silo_with(&core, ran_id).unwrap();
        assert_eq!(ran_diagnosis["runtimeState"], "stopped");
        assert!(ran_diagnosis["runtimeMessage"].as_str().is_some());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn global_status_stops_presenting_a_historical_observation_without_an_active_silo() {
    use super::{desktop_status_with, initialize_vault_with, DesktopCore};

    let root = temporary_root("identity-historical-observation");
    fs::create_dir_all(&root).unwrap();
    let browser = root.join("chrome.exe");
    fs::write(&browser, []).unwrap();
    fs::write(
        browser.with_extension("version-output"),
        "Google Chrome 126.0.6478.127\n",
    )
    .unwrap();
    let core = DesktopCore::open(root.clone(), root.join("resources"));
    initialize_vault_with(&core, "historical identity passphrase").unwrap();
    let managed = core
        .vault
        .lock()
        .unwrap()
        .create_silo(
            &root,
            CreateSiloInput {
                name: "historical managed silo".to_owned(),
                color: "#5b5ce2".to_owned(),
                browser_kind: BrowserKind::Chrome,
                executable_path: browser.to_string_lossy().into_owned(),
                execution_target: SiloExecutionTarget::Local,
                network_profile: NetworkProfile::Direct {
                    proxy_required: false,
                },
                engine: Default::default(),
                proxy_credentials: None,
                mihomo_controller_secret: None,
            },
        )
        .unwrap();
    // Persist a page-script observation for that Silo, as a previous managed
    // session would have left it after the Silo stopped.
    let session_dir = root
        .join("silos")
        .join(managed.id.to_string())
        .join("engine-state")
        .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    fs::create_dir_all(&session_dir).unwrap();
    fs::write(
        session_dir.join("observed.json"),
        r#"{
  "generatedAtUtc": "2026-09-08T01:02:03Z",
  "observedFull": {
    "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:142.0) Gecko/20100101 Firefox/142.0",
    "language": "zh-CN",
    "languages": ["zh-CN", "zh"],
    "platform": "Win32",
    "screen": {"width": 1920, "height": 1080, "colorDepth": 24},
    "hardwareConcurrency": 8,
    "webdriver": false,
    "session": {"timezone": "Asia/Shanghai"}
  }
}"#,
    )
    .unwrap();
    {
        let mut runtime = core.runtime.lock().unwrap();
        // While that Silo is active its observation hydrates as the current
        // identity, attributed to this exact Silo.
        runtime.hydrate_website_identity(Some(managed.id));
        let observation = runtime.website_identity().expect("current observation");
        assert_eq!(observation.silo_id, managed.id);
    }
    // With no active Silo the global status must stop presenting the old
    // observation as the current identity (QA-R1-02).
    let status = desktop_status_with(&core).unwrap();
    assert!(
        status.website_identity.is_none(),
        "a historical observation must not be presented as the current global identity"
    );
    // The persisted evidence itself is not deleted by the fix.
    assert!(session_dir.join("observed.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_health_releases_vault_and_rechecks_expiry_before_returning() {
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::Duration;

    use super::{initialize_vault_with, list_silos_with, DesktopCore};
    use crate::domain::VaultLockState;

    for expire_during_health in [false, true] {
        let root = temporary_root("status-health-vault-contention");
        fs::create_dir_all(&root).unwrap();
        let core = Arc::new(DesktopCore::open(root.clone(), root.join("resources")));
        core.vault.lock().unwrap().set_test_now(Utc::now());
        let initial = initialize_vault_with(&core, "status health test passphrase").unwrap();
        let deadline = initial.auto_lock_at.unwrap();
        let observation_silo = Uuid::new_v4();
        let session_dir = root
            .join("silos")
            .join(observation_silo.to_string())
            .join("engine-state")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&session_dir).unwrap();
        fs::write(
            session_dir.join("observed.json"),
            r#"{"generatedAtUtc":"2026-09-08T01:02:03Z","observedFull":{"userAgent":"Mozilla/5.0 test","language":"zh-CN","languages":["zh-CN"],"platform":"Win32","screen":{"width":1920,"height":1080,"colorDepth":24},"hardwareConcurrency":8,"webdriver":false,"session":{"timezone":"Asia/Shanghai"}}}"#,
        )
        .unwrap();
        core.runtime
            .lock()
            .unwrap()
            .hydrate_website_identity(Some(observation_silo));

        let (health_started_tx, health_started) = mpsc::channel();
        let (release_health, resume_health) = mpsc::channel();
        let status_core = Arc::clone(&core);
        let status_worker = thread::spawn(move || {
            super::runtime::desktop_status_with_refresh(&status_core, |runtime| {
                assert!(runtime.website_identity().is_some());
                health_started_tx.send(()).unwrap();
                resume_health.recv().unwrap();
                runtime.activation()
            })
        });
        health_started.recv_timeout(Duration::from_secs(2)).unwrap();
        // Slow legacy health owns its Runtime only. It must not take the global
        // lifecycle barrier or block unrelated Vault reads.
        let lifecycle_reserved = matches!(
            core.local_control.reservation.try_write(),
            Err(TryLockError::WouldBlock)
        );
        let (listed_tx, listed) = mpsc::channel();
        let list_core = Arc::clone(&core);
        let list_worker = thread::spawn(move || {
            listed_tx.send(list_silos_with(&list_core)).unwrap();
        });
        let read_while_health_waits = listed.recv_timeout(Duration::from_secs(2));
        if read_while_health_waits.is_ok() && expire_during_health {
            core.vault.lock().unwrap().set_test_now(deadline);
        }
        // Always release the worker before asserting, including on failure.
        release_health.send(()).unwrap();
        let status = status_worker.join().unwrap().unwrap();
        list_worker.join().unwrap();
        assert!(!lifecycle_reserved, "status health must not reserve every Silo lifecycle");
        assert!(
            read_while_health_waits.unwrap().unwrap().is_empty(),
            "Vault reads must not wait for runtime health"
        );
        if expire_during_health {
            assert!(matches!(status.vault.state, VaultLockState::Locked));
            assert!(status.vault.auto_lock_at.is_none());
            assert!(core.runtime.lock().unwrap().website_identity().is_none());
            let snapshot: serde_json::Value = serde_json::from_slice(
                &fs::read(root.join(crate::native_host::RUNTIME_STATUS_SNAPSHOT_FILE)).unwrap(),
            )
            .unwrap();
            assert_eq!(snapshot["vault"]["state"], "locked");
            assert!(snapshot["vault"]["autoLockAt"].is_null());
        } else {
            assert!(matches!(status.vault.state, VaultLockState::Unlocked));
            assert_eq!(status.vault.auto_lock_at, Some(deadline));
        }
        assert!(status.website_identity.is_none());
        drop(core);
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn status_marks_identity_evidence_stale_when_its_silo_was_deleted() {
    use super::{desktop_status_with, initialize_vault_with, DesktopCore};
    use crate::domain::IdentityEvidenceState;
    use crate::launcher::RuntimeManager;

    let root = temporary_root("status-deleted-identity");
    fs::create_dir_all(&root).unwrap();
    let core = DesktopCore::open(root.clone(), root.join("resources"));
    initialize_vault_with(&core, "deleted identity test passphrase").unwrap();
    let silo_id = Uuid::new_v4();
    let record = serde_json::json!({
        "siloId": silo_id,
        "pid": 424242,
        "startedAt": "2026-09-08T00:00:00Z",
        "lastSeenAt": "2026-09-08T00:01:00Z",
        "state": "stopped",
        "identityEvidence": {
            "siloId": silo_id,
            "runtimeId": Uuid::new_v4(),
            "sessionId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "artifactId": "previous-artifact",
            "artifactFileSha256": "a".repeat(64),
            "engineAdapter": "camoufox",
            "observedAt": "2026-09-08T00:01:00Z",
            "state": "matched",
            "signals": []
        }
    });
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(
        root.join("runtime").join("browser-session.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    *core.runtime.lock().unwrap() = RuntimeManager::open(&root);

    let status = desktop_status_with(&core).unwrap();
    let evidence = status.activation.unwrap().identity_evidence.unwrap();
    assert_eq!(evidence.silo_id, silo_id);
    assert_eq!(evidence.state, IdentityEvidenceState::Stale);
    assert!(status.website_identity.is_none());
    drop(core);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn diagnostic_runtime_contention_does_not_hold_vault_or_probe_another_silo() {
    use std::sync::{mpsc, Arc};
    use std::thread;
    use std::time::Duration;

    use super::{initialize_vault_with, list_silos_with, DesktopCore};
    use crate::domain::RuntimeState;

    let root = temporary_root("diagnostic-runtime-vault-contention");
    fs::create_dir_all(&root).unwrap();
    let core = Arc::new(DesktopCore::open(root.clone(), root.join("resources")));
    initialize_vault_with(&core, "diagnostic contention passphrase").unwrap();
    let target_id = recent_run_test_silo(&core, "diagnostic target").id;
    let other_silo_id = Uuid::new_v4();
    {
        let mut environment = core.environment_runtime.lock().unwrap();
        environment.activation.active_silo_id = Some(other_silo_id);
        environment.activation.state = RuntimeState::RecoveryRequired;
        environment.activation.message = Some("retain unrelated provider evidence".to_owned());
    }
    let runtime_guard = core.runtime.lock().unwrap();
    let diagnostic_core = Arc::clone(&core);
    let (diagnosed_tx, diagnosed) = mpsc::channel();
    let diagnostic_worker = thread::spawn(move || {
        diagnosed_tx.send(super::runtime::diagnostic_status_for_silo(&diagnostic_core, target_id)).unwrap();
    });
    let target_while_other_runtime_busy = diagnosed.recv_timeout(Duration::from_secs(2));
    let (listed_tx, listed) = mpsc::channel();
    let list_core = Arc::clone(&core);
    let list_worker = thread::spawn(move || {
        listed_tx.send(list_silos_with(&list_core)).unwrap();
    });
    let read_while_runtime_waits = listed.recv_timeout(Duration::from_secs(2));
    // Release even on regression, so failed assertions cannot strand workers.
    drop(runtime_guard);
    diagnostic_worker.join().unwrap();
    let diagnostic = target_while_other_runtime_busy.expect("target diagnosis must not wait for another runtime").unwrap();
    list_worker.join().unwrap();

    assert_eq!(read_while_runtime_waits.unwrap().unwrap().len(), 1);
    assert_eq!(diagnostic.activation.as_ref().unwrap().active_silo_id, None);
    assert_eq!(
        core.environment_runtime
            .lock()
            .unwrap()
            .activation
            .message
            .as_deref(),
        Some("retain unrelated provider evidence"),
        "diagnosis must not reconcile another Silo's provider"
    );
    drop(core);
    fs::remove_dir_all(root).unwrap();
}

fn wsl_silo(id: Uuid, distribution: &str) -> Silo {
    Silo {
        id,
        schema_version: SCHEMA_VERSION,
        name: "WSL artifact test".to_owned(),
        color: "#5b5ce2".to_owned(),
        browser: Some(BrowserDescriptor {
            kind: BrowserKind::Chrome,
            executable_path: "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
                .to_owned(),
            version: Some("126.0.0.0".to_owned()),
        }),
        execution_target: SiloExecutionTarget::Wsl {
            distribution: distribution.to_owned(),
        },
        profile_directory: "C:\\Users\\Test\\AppData\\Local\\VeriSilo\\silo".to_owned(),
        network_profile: NetworkProfile::Direct {
            proxy_required: false,
        },
        engine: Default::default(),
        seed_reference: Uuid::new_v4(),
        created_at: Utc::now(),
        identity_locked_at: None,
        archived_at: None,
    }
}

fn wsl_artifact_directory(root: &std::path::Path, silo_id: Uuid) -> PathBuf {
    root.join("environments")
        .join("wsl")
        .join(silo_id.to_string())
}

fn write_wsl_binding(root: &std::path::Path, silo_id: Uuid, distribution: &str) -> PathBuf {
    let directory = wsl_artifact_directory(root, silo_id);
    fs::create_dir_all(&directory).expect("create WSL artifact directory");
    fs::write(
        directory.join("binding.json"),
        serde_json::to_vec(&serde_json::json!({
            "schemaVersion": crate::environment::backend::ENVIRONMENT_CONTRACT_VERSION,
            "environmentId": silo_id,
            "backend": "wsl-chromium",
            "providerKey": distribution,
        }))
        .expect("serialize WSL binding"),
    )
    .expect("write WSL binding");
    directory
}

#[test]
fn verified_current_wsl_artifact_accepts_only_one_matching_wsl_binding() {
    let root = temporary_root("verified-wsl-binding");
    let silo_id = Uuid::new_v4();
    let silo = wsl_silo(silo_id, "Ubuntu-24.04");
    write_wsl_binding(&root, silo_id, "Ubuntu-24.04");

    assert_eq!(
        verified_current_wsl_artifact(&root, &silo).expect("verify matching WSL artifact"),
        Some("Ubuntu-24.04".to_owned())
    );

    fs::remove_dir_all(root).expect("remove matching WSL fixture");
}

#[test]
fn verified_current_wsl_artifact_returns_none_without_an_artifact() {
    let root = temporary_root("verified-wsl-none");
    fs::create_dir_all(&root).expect("create empty WSL fixture");
    let silo = wsl_silo(Uuid::new_v4(), "Ubuntu-24.04");

    assert_eq!(
        verified_current_wsl_artifact(&root, &silo).expect("verify missing WSL artifact"),
        None
    );

    fs::remove_dir_all(root).expect("remove empty WSL fixture");
}

#[test]
fn verified_current_wsl_artifact_rejects_extra_backend_artifacts() {
    let root = temporary_root("verified-wsl-extra-backend");
    let silo_id = Uuid::new_v4();
    let silo = wsl_silo(silo_id, "Ubuntu-24.04");
    write_wsl_binding(&root, silo_id, "Ubuntu-24.04");
    fs::create_dir_all(
        root.join("environments")
            .join("sandbox")
            .join(silo_id.to_string()),
    )
    .expect("create extra backend artifact");

    assert!(verified_current_wsl_artifact(&root, &silo).is_err());

    fs::remove_dir_all(root).expect("remove extra backend fixture");
}

#[test]
fn verified_current_wsl_artifact_rejects_missing_or_incomplete_binding() {
    let missing_root = temporary_root("verified-wsl-missing-binding");
    let missing_id = Uuid::new_v4();
    let missing_silo = wsl_silo(missing_id, "Ubuntu-24.04");
    fs::create_dir_all(wsl_artifact_directory(&missing_root, missing_id))
        .expect("create missing binding artifact");
    assert!(verified_current_wsl_artifact(&missing_root, &missing_silo).is_err());
    fs::remove_dir_all(missing_root).expect("remove missing binding fixture");

    let incomplete_root = temporary_root("verified-wsl-incomplete-binding");
    let incomplete_id = Uuid::new_v4();
    let incomplete_silo = wsl_silo(incomplete_id, "Ubuntu-24.04");
    let directory = wsl_artifact_directory(&incomplete_root, incomplete_id);
    fs::create_dir_all(&directory).expect("create incomplete binding artifact");
    fs::write(directory.join("binding.json"), b"{}").expect("write incomplete binding");
    assert!(verified_current_wsl_artifact(&incomplete_root, &incomplete_silo).is_err());
    fs::remove_dir_all(incomplete_root).expect("remove incomplete binding fixture");
}

#[test]
fn verified_current_wsl_artifact_rejects_distribution_mismatch() {
    let root = temporary_root("verified-wsl-distribution-mismatch");
    let silo_id = Uuid::new_v4();
    let silo = wsl_silo(silo_id, "Ubuntu-24.04");
    write_wsl_binding(&root, silo_id, "Debian");

    assert!(verified_current_wsl_artifact(&root, &silo).is_err());

    fs::remove_dir_all(root).expect("remove distribution mismatch fixture");
}

#[test]
fn wsl_destroy_then_transient_vault_failure_can_be_retried_safely() {
    let root = temporary_root("wsl-delete-retry");
    fs::create_dir_all(&root).expect("create WSL delete fixture");
    let mut vault = VaultRuntime::default();
    vault
        .initialize(&root, "a WSL deletion retry passphrase")
        .expect("initialize retry Vault");
    let silo = vault
        .create_silo(
            &root,
            CreateSiloInput {
                name: "WSL delete retry".to_owned(),
                color: "#5b5ce2".to_owned(),
                browser_kind: BrowserKind::Chrome,
                executable_path: "/usr/bin/chromium".to_owned(),
                execution_target: SiloExecutionTarget::Wsl {
                    distribution: "Ubuntu-24.04".to_owned(),
                },
                network_profile: NetworkProfile::Direct {
                    proxy_required: false,
                },
                engine: Default::default(),
                proxy_credentials: None,
                mihomo_controller_secret: None,
            },
        )
        .expect("create WSL Silo");
    let artifact = write_wsl_binding(&root, silo.id, "Ubuntu-24.04");
    assert!(verified_current_wsl_artifact(&root, &silo)
        .expect("verify WSL artifact before destroy")
        .is_some());

    fs::remove_dir_all(artifact).expect("simulate successful WSL destroy");
    assert!(vault.delete_silo(&root, silo.id, true, true).is_err());
    assert_eq!(vault.list_silos().expect("list retained Silo").len(), 1);

    vault
        .delete_silo(&root, silo.id, false, true)
        .expect("retry Vault deletion after transient failure");
    assert!(vault.list_silos().expect("list deleted Silos").is_empty());

    fs::remove_dir_all(root).expect("remove WSL delete fixture");
}

#[test]
fn managed_failures_return_stable_user_codes_without_internal_details() {
    assert_eq!(
        managed_launcher_error(LauncherError::ProxyPreflight(
            "proxy.internal.example:1080".to_owned(),
        )),
        "managed_network_mismatch"
    );
    assert_eq!(
        managed_vault_error(VaultError::SiloProfileInUse),
        "managed_profile_in_use"
    );
    let mihomo = managed_launcher_error(LauncherError::Mihomo(
        "Clash 当前是直连模式，所选节点不会生效。请在 Clash 里改回规则或全局模式后再启动。"
            .to_owned(),
    ));
    assert!(mihomo.contains("直连模式"), "{mihomo}");
    assert!(!mihomo.contains("managed_network_mismatch"), "{mihomo}");
}

#[test]
fn provision_errors_use_host_codes_without_exposing_private_messages() {
    let network = managed_identity_generation_error(EngineError::HostProvisionRejected {
        code: "network_observation_failed".to_owned(),
        message: "C:\\Users\\private\\proxy".to_owned(),
    });
    assert!(network.contains("查询出口地区"));
    assert!(!network.contains("private"));

    let unknown = managed_identity_generation_error(EngineError::HostProvisionRejected {
        code: "provision_rejected".to_owned(),
        message: "C:\\Users\\private\\artifact".to_owned(),
    });
    assert_eq!(unknown, "managed_identity_generation_failed");
}

#[test]
fn managed_launch_failure_separates_safe_diagnostics_from_private_details() {
    let timeout = managed_launcher_failure(LauncherError::RuntimeReceipt(
        "Camoufox Host response timeout/EOF: timed out".to_owned(),
    ));
    let value = serde_json::to_value(timeout).unwrap();
    assert_eq!(value["code"], "managed_browser_open_failed");
    assert!(value["detail"].as_str().unwrap().contains("没有按时回应"));

    let private = managed_launcher_failure(LauncherError::ProxyPreflight(
        "proxy.internal.example:1080 password=private".to_owned(),
    ));
    let value = serde_json::to_value(private).unwrap();
    assert_eq!(value["code"], "managed_network_mismatch");
    assert!(value.get("detail").is_none());

    let rejected = managed_launcher_failure(LauncherError::RuntimeReceipt(
        "Camoufox Host rejected launch: private path C:\\Users\\hidden (integrity_rejected)"
            .to_owned(),
    ));
    let value = serde_json::to_value(rejected).unwrap();
    assert_eq!(
        value["detail"],
        "浏览器宿主拒绝本次操作（错误码 integrity_rejected）。"
    );
}

#[test]
fn provider_reservation_blocks_launch_update_archive_and_delete_until_completion() {
    let control = LocalEnvironmentControl::default();
    let provider = control.reserve().expect("provider reservation");

    for blocked_operation in ["launch", "update", "archive", "delete"] {
        assert!(
            matches!(
                control.reservation.try_write(),
                Err(TryLockError::WouldBlock)
            ),
            "{blocked_operation} must share the in-flight provider reservation"
        );
    }

    drop(provider);
    drop(
        control
            .reserve()
            .expect("lifecycle operation proceeds after provider completion"),
    );
}

fn recent_run_test_silo(core: &super::DesktopCore, name: &str) -> Silo {
    let browser = core.root.join("chrome.exe");
    fs::write(&browser, []).unwrap();
    fs::write(
        browser.with_extension("version-output"),
        "Google Chrome 126.0.6478.127\n",
    )
    .unwrap();
    core.vault
        .lock()
        .unwrap()
        .create_silo(
            &core.root,
            CreateSiloInput {
                name: name.to_owned(),
                color: "#5b5ce2".to_owned(),
                browser_kind: BrowserKind::Chrome,
                executable_path: browser.to_string_lossy().into_owned(),
                execution_target: SiloExecutionTarget::Local,
                network_profile: NetworkProfile::Direct {
                    proxy_required: false,
                },
                engine: Default::default(),
                proxy_credentials: None,
                mihomo_controller_secret: None,
            },
        )
        .unwrap()
}

#[test]
fn recent_run_failed_launches_stay_per_silo_after_desktop_reopen() {
    use super::{
        initialize_vault_with, launch_silo_with, list_recent_runs, unlock_vault_with, DesktopCore,
    };
    use crate::domain::RuntimeState;
    let root = temporary_root("recent-run-failed-launch");
    fs::create_dir_all(&root).unwrap();
    let passphrase = "recent run reopen passphrase";
    let records = {
        let core = DesktopCore::open(root.clone(), root.join("resources"));
        initialize_vault_with(&core, passphrase).unwrap();
        let a = recent_run_test_silo(&core, "A");
        let b = recent_run_test_silo(&core, "B");
        fs::remove_file(root.join("chrome.exe")).unwrap();
        for silo in [&a, &b] {
            assert!(launch_silo_with(&core, silo.id).is_err());
            let record = core
                .vault
                .lock()
                .unwrap()
                .get_recent_run(silo.id)
                .unwrap()
                .unwrap();
            assert!(matches!(
                record.state,
                RuntimeState::Failed | RuntimeState::VerificationFailed
            ));
            assert!(record.ended_at.is_some());
            assert!(record.identity_evidence.is_none());
            assert!(record.network_evidence.is_none());
        }
        let records = list_recent_runs(&core).unwrap();
        assert_eq!(records.len(), 2);
        assert_ne!(records[0].run_id, records[1].run_id);
        records
    };
    let reopened = DesktopCore::open(root.clone(), root.join("resources"));
    assert!(list_recent_runs(&reopened).is_err());
    unlock_vault_with(&reopened, passphrase).unwrap();
    let restored = list_recent_runs(&reopened).unwrap();
    assert_eq!(
        serde_json::to_value(restored).unwrap(),
        serde_json::to_value(records).unwrap()
    );
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn recent_run_status_reconciles_stop_and_reports_save_failure_without_hiding_runtime() {
    use super::{desktop_status_with, initialize_vault_with, unlock_vault_with, DesktopCore};
    use crate::domain::{RuntimeActivation, RuntimeNetworkEvidence, RuntimeState};
    let root = temporary_root("recent-run-status");
    fs::create_dir_all(&root).unwrap();
    let passphrase = "recent run status passphrase";
    let silo_id = {
        let core = DesktopCore::open(root.clone(), root.join("resources"));
        initialize_vault_with(&core, passphrase).unwrap();
        let silo = recent_run_test_silo(&core, "stopped Silo");
        let mut vault = core.vault.lock().unwrap();
        vault.begin_recent_run(&root, &silo).unwrap();
        vault
            .update_recent_run(
                &root,
                silo.id,
                &RuntimeActivation {
                    active_silo_id: Some(silo.id),
                    state: RuntimeState::Running,
                    network_evidence: Some(RuntimeNetworkEvidence::configured(
                        &silo.network_profile,
                        false,
                    )),
                    ..RuntimeActivation::idle()
                },
            )
            .unwrap();
        silo.id
    };
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(
        root.join("runtime/browser-session.json"),
        serde_json::to_vec(&serde_json::json!({
            "siloId": silo_id, "pid": 424242, "startedAt": Utc::now(),
            "lastSeenAt": Utc::now(), "state": "stopped"
        }))
        .unwrap(),
    )
    .unwrap();
    let core = DesktopCore::open(root.clone(), root.join("resources"));
    unlock_vault_with(&core, passphrase).unwrap();
    // Block only the snapshot's durable write in this disposable fixture.
    fs::rename(root.join("vault.json"), root.join("vault.saved")).unwrap();
    fs::create_dir(root.join("vault.json")).unwrap();
    let status = desktop_status_with(&core).unwrap();
    assert_eq!(status.activation.as_ref().unwrap().state, RuntimeState::Stopped);
    assert_eq!(status.recent_runs_warning, Some("save_failed"));
    assert_eq!(
        core.vault
            .lock()
            .unwrap()
            .get_recent_run(silo_id)
            .unwrap()
            .unwrap()
            .state,
        RuntimeState::Running
    );
    fs::remove_dir(root.join("vault.json")).unwrap();
    fs::rename(root.join("vault.saved"), root.join("vault.json")).unwrap();
    let status = desktop_status_with(&core).unwrap();
    assert!(status.recent_runs_warning.is_none());
    let record = core
        .vault
        .lock()
        .unwrap()
        .get_recent_run(silo_id)
        .unwrap()
        .unwrap();
    assert_eq!(record.state, RuntimeState::Stopped);
    assert!(record.ended_at.is_some());
    drop(core);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn silo_reservations_allow_independent_targets_and_exclude_global_replacement() {
    let control = LocalEnvironmentControl::default();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let first = control.reserve_silo(a).unwrap();
    let second = control.reserve_silo(b).unwrap();
    assert!(control.reserve_silo(a).is_err(), "duplicate target operation must fail immediately");
    assert!(matches!(control.reservation.try_write(), Err(TryLockError::WouldBlock)));
    drop(first);
    let restarted = control.reserve_silo(a).unwrap();
    drop(restarted);
    drop(second);
    assert!(control.reservation.try_write().is_ok());
}

#[test]
fn global_replacement_and_engine_maintenance_reject_per_silo_recovery() {
    let root = temporary_root("concurrency-global-recovery-guards");
    let id = Uuid::new_v4();
    fs::create_dir_all(root.join("runtime")).unwrap();
    fs::write(
        crate::launcher::local_runtime_record_path(&root, id),
        serde_json::to_vec(&serde_json::json!({
            "siloId": id, "runtimeId": Uuid::new_v4(), "pid": std::process::id(),
            "startedAt": Utc::now(), "lastSeenAt": Utc::now(), "state": "running"
        })).unwrap(),
    ).unwrap();
    let core = super::DesktopCore::open(root.clone(), root.join("resources"));
    super::initialize_vault_with(&core, "synthetic global boundary passphrase").unwrap();
    assert!(core.runtime.lock().unwrap().cached_activation().active_silo_id.is_none());
    let engine_error = super::rollback_engine_package(&core, crate::engine::EngineAdapterId::Camoufox)
        .unwrap_err();
    assert!(engine_error.contains("Stop all Silos"), "{engine_error}");
    let restore_error = super::restore_vault(&core, root.join("absent.backup").display().to_string(),
        "synthetic backup passphrase".to_owned(), true).unwrap_err();
    assert!(restore_error.contains("every local Silo"), "{restore_error}");
    assert!(crate::launcher::local_runtime_record_path(&root, id).is_file());
    drop(core);
    fs::remove_dir_all(root).unwrap();
}
