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
    tauri_build::build()
}
