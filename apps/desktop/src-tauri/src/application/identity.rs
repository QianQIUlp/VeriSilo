use super::environments::environment_runtime_is_active;
use super::DesktopCore;
use crate::domain::{
    CreateManagedSiloInput, ManagedIdentityIntent, ManagedIdentityPreset, ManagedIdentityPreview,
    NetworkProfile, ProxyCredentialsInput, Silo, UpdateManagedIdentityInput,
};
use crate::engine::{CamoufoxProvisionOptions, EngineAdapterId, ExternalPackageEngineAdapter};
use crate::launcher::LauncherError;
use crate::proxy_relay::ProxyRelay;
use crate::vault::{
    MihomoControllerAuthentication, ProxyAuthentication, StoredIdentityArtifact, VaultError,
};
use crate::{engine, mihomo};
use rand::rngs::OsRng;
use rand::RngCore;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub(crate) fn managed_browser_package_root(state: &DesktopCore) -> PathBuf {
    state
        .resource_root
        .join("managed-browser")
        .join("engine-package")
}

pub(crate) fn managed_vault_error(error: VaultError) -> String {
    match error {
        VaultError::SiloRunning => "managed_another_silo_running",
        VaultError::SiloProfileInUse => "managed_profile_in_use",
        VaultError::InvalidData | VaultError::SiloNotFound | VaultError::UnmanagedProfile => {
            "managed_artifact_unavailable"
        }
        VaultError::Filesystem(_) => "managed_runtime_recovery_required",
        _ => "managed_create_failed",
    }
    .to_owned()
}

pub(crate) fn managed_launcher_error(error: LauncherError) -> String {
    match &error {
        LauncherError::AnotherSiloRunning => "managed_another_silo_running".to_owned(),
        LauncherError::ProfileInUse => "managed_profile_in_use".to_owned(),
        LauncherError::ProxyPreflight(detail)
        | LauncherError::ProxyRelay(detail)
        | LauncherError::InvalidNetwork(detail)
        | LauncherError::Mihomo(detail) => {
            pass_user_detail(&error.to_string(), detail, "managed_network_mismatch")
        }
        LauncherError::RuntimeReceipt(detail) | LauncherError::Bootstrap(detail) => {
            pass_user_detail(&error.to_string(), detail, "managed_browser_open_failed")
        }
        LauncherError::BrowserVerification(detail)
        | LauncherError::BrowserStartup(detail)
        | LauncherError::Engine(detail) => {
            pass_user_detail(&error.to_string(), detail, "managed_engine_unavailable")
        }
        LauncherError::Spawn(_) => {
            let raw = error.to_string();
            if has_cjk(&raw) {
                raw
            } else {
                "managed_engine_unavailable".to_owned()
            }
        }
    }
}

pub(crate) fn has_cjk(value: &str) -> bool {
    value
        .chars()
        .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
}

pub(crate) fn pass_user_detail(full: &str, detail: &str, fallback: &str) -> String {
    if has_cjk(detail) {
        full.to_owned()
    } else {
        fallback.to_owned()
    }
}

pub(crate) fn managed_proxy_error(raw: String) -> String {
    if has_cjk(&raw) {
        raw
    } else {
        "managed_network_mismatch".to_owned()
    }
}

pub(crate) fn managed_provision_roots(
    root: &std::path::Path,
    provision_id: Uuid,
) -> engine::CamoufoxHostRoots {
    let managed_root = root.join("silos").join(provision_id.to_string());
    engine::CamoufoxHostRoots {
        artifact_root: managed_root.join("identity"),
        profile_root: managed_root.join("profiles"),
        state_root: managed_root.join("engine-state"),
    }
}

pub(crate) fn managed_identity_generation_error(error: engine::EngineError) -> String {
    let raw = error.to_string();
    let lower = raw.to_ascii_lowercase();
    if lower.contains("ipwho") {
        return "无法通过当前代理查询出口地区。请确认 Clash 节点能访问外网后再试。".to_owned();
    }
    if lower.contains("no supported locale") {
        return "当前代理出口地区没有匹配的语言包。请换一条线路，或关闭「时区语言跟随出口」。"
            .to_owned();
    }
    if lower.contains("treeintegrity") || lower.contains("extra files") {
        return "内置浏览器目录多出了运行文件，身份生成被拦住了。请重试；若仍失败，重启应用后再创建。"
            .to_owned();
    }
    if lower.contains("strict json") || lower.contains("length prefix") {
        return "身份组件返回了无法识别的结果。".to_owned();
    }
    if let Some(message) = raw.split_once(": ").map(|(_, rest)| rest.trim()) {
        if !message.is_empty()
            && message.chars().count() <= 180
            && !message.contains('\\')
            && !message.contains('\n')
        {
            return format!("身份配置生成失败：{message}");
        }
    }
    "managed_identity_generation_failed".to_owned()
}

pub(crate) fn provision_managed_artifact(
    root: &std::path::Path,
    package_root: &std::path::Path,
    relay_silo_id: Uuid,
    intent: &ManagedIdentityIntent,
    network_profile: &NetworkProfile,
    proxy_credentials: Option<&ProxyCredentialsInput>,
    mihomo_controller_secret: Option<&str>,
    seed: &[u8; 32],
) -> Result<engine::CamoufoxProvisionResult, String> {
    let provision_id = Uuid::new_v4();
    let managed_root = root.join("silos").join(provision_id.to_string());
    let roots = managed_provision_roots(root, provision_id);
    let has_proxy = matches!(network_profile, NetworkProfile::FixedProxy { .. });
    let mihomo_authentication = mihomo_controller_secret
        .map(|secret| MihomoControllerAuthentication::new(secret.to_owned()));
    let mut pinned_inbound = None;
    if let Some(binding) = network_profile.external_mihomo_binding() {
        let inbound =
            mihomo::pin_selected_inbound(binding, mihomo_authentication.as_ref(), relay_silo_id)
                .map_err(|error| error.to_string())?;
        pinned_inbound = Some(inbound);
    }
    let relay_profile = pinned_inbound.as_ref().map_or_else(
        || network_profile.clone(),
        |inbound| match network_profile {
            NetworkProfile::FixedProxy {
                proxy_required,
                bypass_list,
                credential_reference,
                external_mihomo,
                ..
            } => NetworkProfile::FixedProxy {
                proxy_required: *proxy_required,
                scheme: crate::domain::ProxyScheme::Socks5,
                host: "127.0.0.1".to_owned(),
                port: inbound.port,
                bypass_list: bypass_list.clone(),
                credential_reference: *credential_reference,
                external_mihomo: external_mihomo.clone(),
            },
            other => other.clone(),
        },
    );
    let relay = if has_proxy {
        let authentication = proxy_credentials.map(|credentials| {
            ProxyAuthentication::new(credentials.username.clone(), credentials.password.clone())
        });
        let relay = ProxyRelay::start(
            &relay_profile,
            relay_silo_id,
            Uuid::new_v4(),
            authentication,
        )
        .map_err(|error| managed_proxy_error(error.to_string()))?;
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        relay
            .verify_upstream_until(Instant::now() + Duration::from_secs(10), &cancelled)
            .map_err(|error| managed_proxy_error(error.to_string()))?;
        Some(relay)
    } else {
        None
    };
    let proxy_server = relay.as_ref().map(|relay| {
        format!(
            "socks5://{}:{}",
            relay.endpoint().host,
            relay.endpoint().port
        )
    });
    let follow_network = intent.follow_network_exit(has_proxy);
    let host_preset = if follow_network {
        ManagedIdentityPreset::MatchFixedProxy
    } else {
        intent.host_preset()
    };
    let result = (|| -> Result<engine::CamoufoxProvisionResult, String> {
        let mut adapter =
            ExternalPackageEngineAdapter::production_prototype(EngineAdapterId::Camoufox)
                .map_err(|_| "managed_engine_unavailable".to_owned())?;
        adapter
            .ensure_builtin_package(package_root)
            .map_err(|_| "managed_engine_unavailable".to_owned())?;
        adapter
            .provision_camoufox_artifact(
                &roots,
                CamoufoxProvisionOptions {
                    preset: host_preset.as_str(),
                    seed,
                    proxy_server: proxy_server.as_deref(),
                    window: (intent.screen_width, intent.screen_height),
                    hardware_concurrency: intent.hardware_concurrency,
                    follow_network,
                    gpu_preset: intent.gpu_preset.as_deref(),
                    timezone: intent.timezone.as_deref(),
                },
            )
            .map_err(managed_identity_generation_error)
    })();
    drop(relay);
    if let (Some(binding), Some(inbound)) = (
        network_profile.external_mihomo_binding(),
        pinned_inbound.as_ref(),
    ) {
        let _ = mihomo::unpin_inbound(binding, mihomo_authentication.as_ref(), inbound);
    }
    let cleanup = match fs::remove_dir_all(&managed_root) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("managed_runtime_recovery_required".to_owned()),
    };
    match result {
        Ok(result) => {
            cleanup?;
            Ok(result)
        }
        Err(error) => match cleanup {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(cleanup_error),
        },
    }
}

pub(crate) fn create_managed_silo(
    state: &DesktopCore,
    input: CreateManagedSiloInput,
) -> Result<Silo, String> {
    create_managed_silo_with(&state, input)
}

pub(crate) fn create_managed_silo_with(
    state: &DesktopCore,
    input: CreateManagedSiloInput,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    input.validate().map_err(|error| error.to_string())?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    let package_root = managed_browser_package_root(&state);
    let mut seed = [0_u8; 32];
    OsRng.fill_bytes(&mut seed);
    let result = provision_managed_artifact(
        &state.root,
        &package_root,
        Uuid::new_v4(),
        &input.identity_intent(),
        &input.network_profile,
        input.proxy_credentials.as_ref(),
        input
            .mihomo_controller_secret
            .as_ref()
            .map(|secret| secret.secret.as_str()),
        &seed,
    )?;
    let artifact = StoredIdentityArtifact {
        artifact_id: result.artifact_id,
        schema: result.schema,
        raw_json: result.raw_json,
        raw_sha256: result.artifact_file_sha256,
    };
    vault
        .create_managed_silo(&state.root, input, artifact, &seed)
        .map_err(managed_vault_error)
}

pub(crate) fn list_managed_identity_previews(
    state: &DesktopCore,
) -> Result<std::collections::HashMap<Uuid, ManagedIdentityPreview>, String> {
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    vault.record_activity().map_err(|error| error.to_string())?;
    vault
        .list_managed_identity_previews()
        .map_err(managed_vault_error)
}

pub(crate) fn update_managed_identity(
    state: &DesktopCore,
    silo_id: Uuid,
    input: UpdateManagedIdentityInput,
) -> Result<Silo, String> {
    let _local_reservation = state.local_control.reserve()?;
    let mut vault = state
        .vault
        .lock()
        .map_err(|_| "VeriSilo vault state is unavailable.".to_owned())?;
    let mut runtime = state
        .runtime
        .lock()
        .map_err(|_| "VeriSilo runtime state is unavailable.".to_owned())?;
    let is_active = runtime.is_active(silo_id) || environment_runtime_is_active(&state, silo_id)?;
    drop(runtime);
    if is_active {
        return Err("managed_silo_active".to_owned());
    }
    let current = vault.get_silo(silo_id).map_err(|error| error.to_string())?;
    if current.engine.camoufox_artifact_binding().is_none() {
        return Err("managed_artifact_unavailable".to_owned());
    }
    if current.identity_locked_at.is_some() {
        return Err("managed_identity_locked".to_owned());
    }
    let has_proxy = current.network_profile.requires_proxy();
    let intent = input.identity_intent();
    intent
        .validate(has_proxy)
        .map_err(|_| "managed_identity_preset_invalid".to_owned())?;
    let proxy_authentication = vault
        .proxy_authentication_for_silo(silo_id)
        .map_err(managed_vault_error)?;
    let proxy_credentials =
        proxy_authentication
            .as_ref()
            .map(|authentication| ProxyCredentialsInput {
                username: authentication.username().to_owned(),
                password: authentication.password().to_owned(),
            });
    let mihomo_authentication = vault
        .mihomo_controller_authentication_for_silo(silo_id)
        .map_err(managed_vault_error)?;
    let current_seed = vault
        .identity_seed_for_silo(silo_id)
        .map_err(managed_vault_error)?;
    let mut seed = *current_seed;
    if input.rotate_seed {
        OsRng.fill_bytes(&mut seed);
    }
    let result = provision_managed_artifact(
        &state.root,
        &managed_browser_package_root(&state),
        silo_id,
        &intent,
        &current.network_profile,
        proxy_credentials.as_ref(),
        mihomo_authentication
            .as_ref()
            .map(MihomoControllerAuthentication::secret),
        &seed,
    )?;
    let artifact = StoredIdentityArtifact {
        artifact_id: result.artifact_id,
        schema: result.schema,
        raw_json: result.raw_json,
        raw_sha256: result.artifact_file_sha256,
    };
    vault
        .replace_managed_identity(
            &state.root,
            silo_id,
            artifact,
            input.rotate_seed.then_some(&seed),
        )
        .map_err(managed_vault_error)
}
