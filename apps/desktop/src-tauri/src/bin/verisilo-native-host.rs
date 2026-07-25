fn main() {
    if let Err(error) = verisilo_desktop_lib::native_host::run_native_host() {
        eprintln!("VeriSilo Native Messaging Host: {error}");
        std::process::exit(1);
    }
}
