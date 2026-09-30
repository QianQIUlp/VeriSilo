//! Linux process and file ownership used by the desktop runtime.
use std::{fs, io, os::fd::AsRawFd, os::unix::fs::OpenOptionsExt, path::Path};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: usize, arg3: usize, arg4: usize, arg5: usize) -> i32;
    fn getppid() -> i32;
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
