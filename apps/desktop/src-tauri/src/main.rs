#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(not(debug_assertions), dev))]
compile_error!(
    "VeriSilo release builds must enable custom-protocol; use `tauri build` or `cargo build --release --features custom-protocol`."
);

fn main() {
    verisilo_desktop_lib::run();
}
