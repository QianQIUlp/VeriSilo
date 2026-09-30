//! Linux x86-64 process and file ownership used by the desktop runtime.
#[cfg(not(target_arch = "x86_64"))]
compile_error!("The native Linux desktop currently supports x86-64 only.");
use std::{fs, io, os::fd::AsRawFd, os::unix::fs::OpenOptionsExt, path::Path};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
    fn getppid() -> i32;
    fn geteuid() -> u32;
    fn fcntl(fd: i32, command: i32, ...) -> i32;
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

pub(crate) fn die_with_parent(parent_pid: u32) -> io::Result<()> {
    // The packaged supervisor owns descendants; this closes the spawn/owner-exit race.
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
        let (mut child, guard) = crate::engine::CamoufoxHostJobGuard::spawn(&mut command).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !pid_file.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let descendant: u32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
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
