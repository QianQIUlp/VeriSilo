fn main() {
    // These IDs are compiled into the Native Messaging Host. Release builds
    // without valid IDs remain safe (the Host authorizes no production
    // extension), and changing either value always invalidates Cargo's cache.
    println!("cargo:rerun-if-env-changed=VERISILO_CHROME_EXTENSION_ID");
    println!("cargo:rerun-if-env-changed=VERISILO_EDGE_EXTENSION_ID");
    println!("cargo:rerun-if-env-changed=VERISILO_HYPERV_IMAGE_FILE");
    println!("cargo:rerun-if-env-changed=VERISILO_HYPERV_IMAGE_SHA256");
    println!("cargo:rerun-if-env-changed=VERISILO_AUTHENTICODE_SIGNER_SHA256");
    println!("cargo:rerun-if-env-changed=VERISILO_ENGINE_SIGNER_SHA256");
    println!("cargo:rerun-if-env-changed=VERISILO_CAMOUFOX_LINUX_ASSET_LOCK");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        if let Some(path) = std::env::var_os("VERISILO_CAMOUFOX_LINUX_ASSET_LOCK") {
            let path = std::path::PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            let bytes = std::fs::read(&path).expect("read native Linux asset lock");
            let lock: serde_json::Value =
                serde_json::from_slice(&bytes).expect("parse native Linux asset lock");
            assert_eq!(
                lock["platform"].as_str(),
                Some("linux-x86_64"),
                "native Linux asset lock platform"
            );
            assert_eq!(
                lock["executableRelativePath"].as_str(),
                Some("camoufox-bin"),
                "native Linux executable"
            );
            for (field, variable) in [
                ("sha256", "VERISILO_CAMOUFOX_LINUX_ASSET_SHA256"),
                (
                    "browserExecutableSha256",
                    "VERISILO_CAMOUFOX_LINUX_EXECUTABLE_SHA256",
                ),
            ] {
                let sha = lock[field].as_str().expect("native Linux asset SHA");
                assert!(
                    sha.len() == 64
                        && sha
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                    "native Linux SHA must be lowercase hex"
                );
                println!("cargo:rustc-env={variable}={sha}");
            }
            let size = lock["sizeBytes"]
                .as_u64()
                .filter(|size| *size > 0)
                .expect("native Linux asset size");
            println!("cargo:rustc-env=VERISILO_CAMOUFOX_LINUX_ASSET_SIZE_BYTES={size}");
        }
    }
    tauri_build::build()
}
