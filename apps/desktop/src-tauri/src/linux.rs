//! Linux x86-64 process and file ownership used by the desktop runtime.
#[cfg(not(target_arch = "x86_64"))]
compile_error!("The native Linux desktop currently supports x86-64 only.");
use std::{
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    os::unix::{fs::OpenOptionsExt, process::CommandExt},
    path::Path,
    process::{Child, Command},
};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
    fn getppid() -> i32;
    fn geteuid() -> u32;
    fn fcntl(fd: i32, command: i32, ...) -> i32;
    fn syscall(number: i64, ...) -> i64;
    fn poll(fds: *mut PollFd, count: u64, timeout_ms: i32) -> i32;
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

pub(crate) fn effective_uid() -> u32 {
    unsafe { geteuid() }
}

#[repr(C)]
struct PosixLock {
    lock_type: i16,
    whence: i16,
    start: i64,
    length: i64,
    pid: i32,
}

pub(crate) struct PosixProfileFileLease {
    _file: fs::File,
}

impl PosixProfileFileLease {
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        Self::acquire_inner(path, false)
    }

    pub(crate) fn acquire_for_maintenance(path: &Path) -> io::Result<Self> {
        Self::acquire_inner(path, true)
    }

    fn acquire_inner(path: &Path, create: bool) -> io::Result<Self> {
        // Maintenance also anchors first startup before Firefox creates its lock.
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .mode(0o600)
            .custom_flags(0x20000)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Firefox lease is not a regular file",
            ));
        }
        let mut lock = PosixLock {
            lock_type: 1,
            whence: 0,
            start: 0,
            length: 0,
            pid: 0,
        }; // F_WRLCK, SEEK_SET, whole file
           // OFD locks conflict with Firefox's POSIX lock, but remain held when a
           // backup reader opens and closes another descriptor to this same file.
        if unsafe { fcntl(file.as_raw_fd(), 37, &mut lock as *mut PosixLock) } != 0 {
            // F_OFD_SETLK
            return Err(io::Error::last_os_error());
        }
        Ok(Self { _file: file })
    }
}

pub(crate) struct FileLease {
    _file: fs::File,
}

impl FileLease {
    pub(crate) fn acquire(path: &Path) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(0x20000) // O_NOFOLLOW: never lock a redirected file.
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lease is not a regular file",
            ));
        }
        if unsafe { flock(file.as_raw_fd(), 2 | 4) } != 0 {
            // LOCK_EX | LOCK_NB
            return Err(io::Error::last_os_error());
        }
        Ok(Self { _file: file })
    }
}

fn die_with_parent(parent_pid: u32) -> io::Result<()> {
    // Close the spawn/desktop-exit race after binding the creator's death signal.
    if unsafe { prctl(1, 9, 0, 0, 0) } != 0 {
        // PR_SET_PDEATHSIG, SIGKILL
        return Err(io::Error::last_os_error());
    }
    if unsafe { getppid() } != parent_pid as i32 {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "desktop owner exited during spawn",
        ));
    }
    Ok(())
}

pub(crate) fn spawn_owned(mut command: Command) -> io::Result<Child> {
    let parent_pid = std::process::id();
    command.process_group(0);
    unsafe {
        command.pre_exec(move || die_with_parent(parent_pid));
    }
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    // PDEATHSIG follows the creator thread. HTTP request threads can end as soon
    // as launch succeeds, so keep a dedicated creator alive until this child exits.
    std::thread::Builder::new()
        .name("linux-child-owner".to_owned())
        .spawn(move || {
            let mut child = match command.spawn() {
                Ok(child) => child,
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            };
            drop(command);
            // pidfd_open on Linux x86-64. The descriptor identifies this child
            // even after wait/reaping; no polling loop or recycled PID is involved.
            let fd = unsafe { syscall(434, child.id() as i32, 0_u32) };
            if fd < 0 {
                let error = io::Error::last_os_error();
                let _ = child.kill();
                let _ = child.wait();
                let _ = sender.send(Err(error));
                return;
            }
            let pidfd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
            if let Err(error) = sender.send(Ok(child)) {
                if let Ok(mut child) = error.0 {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                return;
            }
            let mut descriptor = PollFd {
                fd: pidfd.as_raw_fd(),
                events: 1, // POLLIN: child exit; POLLHUP is also returned after reaping.
                revents: 0,
            };
            loop {
                if unsafe { poll(&mut descriptor, 1, -1) } >= 0 {
                    break;
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    // Ending the creator fails closed via the child's PDEATHSIG.
                    eprintln!("Linux child ownership wait failed: {error}");
                    break;
                }
            }
        })?;
    receiver.recv().map_err(|_| {
        io::Error::new(
            io::ErrorKind::BrokenPipe,
            "Linux child owner failed during spawn",
        )
    })?
}

pub(crate) fn process_start_time(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?
        .1
        .split_ascii_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

pub(crate) fn process_is_alive(pid: u32) -> bool {
    match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit_once(')')
            .and_then(|(_, fields)| fields.split_ascii_whitespace().next())
            .is_some_and(|state| !matches!(state, "Z" | "X")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(_) => Path::new("/proc").join(pid.to_string()).exists(),
    }
}

pub(crate) fn terminate_process_group(pid: u32, start_time: Option<u64>) {
    // Never signal a recycled PID/group. Once the Host has exited, its packaged
    // supervisor owns descendant teardown and reports the exact tree result.
    if pid > 1
        && pid <= i32::MAX as u32
        && start_time.is_some()
        && process_start_time(pid) == start_time
    {
        unsafe { kill(-(pid as i32), 9) }; // Only a group created for this Host.
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn firefox_posix_lease_blocks_another_process_where_flock_does_not() {
        let root =
            std::env::temp_dir().join(format!("verisilo-linux-posix-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join(".parentlock");
        std::fs::write(&path, []).unwrap();
        let probe = || {
            std::process::Command::new("/usr/bin/python3")
            .args(["-c", "import os,sys,fcntl; fd=os.open(sys.argv[1],os.O_RDWR); fcntl.lockf(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)"])
            .arg(&path).stderr(std::process::Stdio::null()).status().unwrap().success()
        };
        let host_lease = super::FileLease::acquire(&path).unwrap();
        assert!(probe(), "flock cannot prove Firefox's POSIX lease is free");
        drop(host_lease);
        let browser_lease = super::PosixProfileFileLease::acquire(&path).unwrap();
        drop(std::fs::File::open(&path).unwrap());
        assert!(
            !probe(),
            "another process acquired an actively held Firefox lock"
        );
        drop(browser_lease);
        assert!(probe());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn host_group_guard_stops_its_child_and_descendant() {
        let root =
            std::env::temp_dir().join(format!("verisilo-linux-group-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let pid_file = root.join("descendant-pid");
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "sh"])
            .arg(&pid_file);
        let (mut child, guard) = crate::engine::CamoufoxHostJobGuard::spawn(command).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let descendant = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .filter(|value| value.ends_with('\n'))
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|pid| *pid > 0)
            {
                break pid;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "shell did not publish a complete descendant PID"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(super::process_is_alive(descendant));
        drop(guard);
        child.wait().unwrap();
        while super::process_is_alive(descendant) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!super::process_is_alive(descendant));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn owned_child_survives_request_thread_and_dies_with_desktop_owner() {
        use std::io::{BufRead, Write};
        const PID_PATH: &str = "VERISILO_TEST_LINUX_CHILD_PID_PATH";
        if let Some(pid_path) = std::env::var_os(PID_PATH) {
            let (mut child, _guard) = std::thread::spawn(|| {
                let mut command = std::process::Command::new("/bin/sh");
                command
                    .args([
                        "-c",
                        "while IFS= read -r line; do printf '%s\\n' \"$line\"; done",
                    ])
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::piped());
                crate::engine::CamoufoxHostJobGuard::spawn(command).unwrap()
            })
            .join()
            .unwrap();
            // The launching request thread has ended. Require a real response
            // from the child before advertising readiness to the outer process.
            let mut stdin = child.stdin.take().unwrap();
            stdin.write_all(b"ready\n").unwrap();
            stdin.flush().unwrap();
            let mut stdout = std::io::BufReader::new(child.stdout.take().unwrap());
            let mut response = String::new();
            stdout.read_line(&mut response).unwrap();
            assert_eq!(response, "ready\n");
            std::fs::write(pid_path, format!("{}\n", child.id())).unwrap();
            loop {
                std::thread::park();
            }
        }

        struct Owner(std::process::Child);
        impl Drop for Owner {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let root =
            std::env::temp_dir().join(format!("verisilo-linux-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let pid_path = root.join("child-pid");
        let mut owner = Owner(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "linux::tests::owned_child_survives_request_thread_and_dies_with_desktop_owner",
                    "--nocapture",
                ])
                .env(PID_PATH, &pid_path)
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&pid_path)
                .ok()
                .filter(|value| value.ends_with('\n'))
                .and_then(|value| value.trim().parse::<u32>().ok())
                .filter(|pid| *pid > 1)
            {
                break pid;
            }
            assert!(
                owner.0.try_wait().unwrap().is_none(),
                "child died when the request thread ended"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "child did not acknowledge after the request thread ended"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert!(super::process_is_alive(pid));
        owner.0.kill().unwrap(); // SIGKILL: no Rust Drop or graceful cleanup runs.
        owner.0.wait().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while super::process_is_alive(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            !super::process_is_alive(pid),
            "child survived desktop SIGKILL"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_lease_is_exclusive_and_released_after_drop() {
        let root =
            std::env::temp_dir().join(format!("verisilo-linux-lease-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("lease");
        let lease = super::FileLease::acquire(&path).unwrap();
        assert!(super::FileLease::acquire(&path).is_err());
        drop(lease);
        drop(super::FileLease::acquire(&path).unwrap());
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(root.join("other"), &path).unwrap();
        assert!(super::FileLease::acquire(&path).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
