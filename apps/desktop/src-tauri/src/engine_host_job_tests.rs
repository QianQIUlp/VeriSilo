use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn camoufox_host_job_stops_a_pending_provisioner_when_ownership_ends() {
    // Wait on an open stdin pipe, as a Host waiting for its provision
    // request does. No installed browser, package or user Vault is used.
    let mut command = Command::new("cmd.exe");
    command.args(["/D", "/Q", "/C", "set /p VERISILO_JOB_TEST="]);
    command.stdin(Stdio::piped());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    crate::domain::hide_windows_console(&mut command);
    let (mut child, job) =
        super::CamoufoxHostJobGuard::spawn(command).expect("start owned Host stand-in");
    assert!(child.try_wait().unwrap().is_none());

    // A hard desktop exit closes this same non-inherited job handle.
    drop(job);
    let deadline = Instant::now() + Duration::from_secs(2);
    let exited = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    if exited.is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(exited.is_some(), "provisioner outlived its ownership guard");
}
