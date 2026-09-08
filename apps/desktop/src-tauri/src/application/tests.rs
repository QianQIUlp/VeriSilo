use std::sync::TryLockError;
use std::{fs, path::PathBuf};

use chrono::Utc;
use uuid::Uuid;

use super::environments::{verified_current_wsl_artifact, LocalEnvironmentControl};
use super::identity::{managed_launcher_error, managed_vault_error};
use crate::domain::{
    BrowserDescriptor, BrowserKind, CreateSiloInput, NetworkProfile, Silo, SiloExecutionTarget,
    SCHEMA_VERSION,
};
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
            assert_eq!(diagnosis["runtimeState"], "idle");
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
fn provider_reservation_blocks_launch_update_archive_and_delete_until_completion() {
    let control = LocalEnvironmentControl::default();
    let provider = control.reserve().expect("provider reservation");

    for blocked_operation in ["launch", "update", "archive", "delete"] {
        assert!(
            matches!(
                control.reservation.try_lock(),
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
