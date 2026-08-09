#![cfg_attr(not(windows), allow(dead_code))]

use std::ffi::{c_void, OsStr};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;

type Bool = i32;
type Dword = u32;
type Handle = *mut c_void;
type Long = i32;

const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
const CREATE_SUSPENDED: Dword = 0x00000004;
const CREATE_UNICODE_ENVIRONMENT: Dword = 0x00000400;
const EXTENDED_STARTUPINFO_PRESENT: Dword = 0x00080000;
const STARTF_USESTDHANDLES: Dword = 0x00000100;
const WAIT_OBJECT_0: Dword = 0;
const WAIT_TIMEOUT: Dword = 258;
const STD_INPUT_HANDLE: Dword = -10i32 as Dword;
const STD_OUTPUT_HANDLE: Dword = -11i32 as Dword;
const STD_ERROR_HANDLE: Dword = -12i32 as Dword;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: Dword = 0x2000;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: Dword = 9;
const FILE_ATTRIBUTE_REPARSE_POINT: Dword = 0x0400;
const FILE_ATTRIBUTE_NORMAL: Dword = 0x0080;
const FILE_FLAG_OPEN_REPARSE_POINT: Dword = 0x00200000;
const FILE_SHARE_READ: Dword = 0x00000001;
const FILE_SHARE_WRITE: Dword = 0x00000002;
const GENERIC_READ: Dword = 0x80000000;
const GENERIC_WRITE: Dword = 0x40000000;
const OPEN_ALWAYS: Dword = 4;
const LOCKFILE_EXCLUSIVE_LOCK: Dword = 0x00000002;
const LOCKFILE_FAIL_IMMEDIATELY: Dword = 0x00000001;
const SYNCHRONIZE: Dword = 0x00100000;
const HANDLE_FLAG_INHERIT: Dword = 0x00000001;

#[repr(C)]
struct FileTime {
    low: Dword,
    high: Dword,
}

#[repr(C)]
struct Overlapped {
    internal: *mut c_void,
    internal_high: *mut c_void,
    offset: Dword,
    offset_high: Dword,
    event: Handle,
}

#[repr(C)]
struct SecurityAttributes {
    length: Dword,
    descriptor: *mut c_void,
    inherit_handle: Bool,
}

#[repr(C)]
struct StartupInfo {
    cb: Dword,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: Dword,
    y: Dword,
    x_size: Dword,
    y_size: Dword,
    x_count_chars: Dword,
    y_count_chars: Dword,
    fill_attribute: Dword,
    flags: Dword,
    show_window: u16,
    reserved2: u16,
    reserved2_ptr: *mut u8,
    std_input: Handle,
    std_output: Handle,
    std_error: Handle,
}

#[repr(C)]
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    process_id: Dword,
    thread_id: Dword,
}

#[repr(C)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: Dword,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: Dword,
    affinity: usize,
    priority_class: Dword,
    scheduling_class: Dword,
}

#[repr(C)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
struct JobObjectExtendedLimitInformation {
    basic: JobObjectBasicLimitInformation,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[repr(C)]
struct ProcessBasicInformation {
    reserved1: *mut c_void,
    peb_base_address: *mut c_void,
    reserved2: [*mut c_void; 2],
    unique_process_id: *mut c_void,
    inherited_from_unique_process_id: *mut c_void,
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> Dword;
    fn CloseHandle(handle: Handle) -> Bool;
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentProcessId() -> Dword;
    fn GetStdHandle(which: Dword) -> Handle;
    fn GetStartupInfoW(startup: *mut StartupInfo);
    fn CreateJobObjectW(attributes: *mut SecurityAttributes, name: *const u16) -> Handle;
    fn SetInformationJobObject(
        job: Handle,
        info_class: Dword,
        info: *mut c_void,
        info_length: Dword,
    ) -> Bool;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> Bool;
    fn IsProcessInJob(process: Handle, job: Handle, result: *mut Bool) -> Bool;
    fn TerminateJobObject(job: Handle, exit_code: Dword) -> Bool;
    fn CreateProcessW(
        application_name: *const u16,
        command_line: *mut u16,
        process_attributes: *mut SecurityAttributes,
        thread_attributes: *mut SecurityAttributes,
        inherit_handles: Bool,
        creation_flags: Dword,
        environment: *mut c_void,
        current_directory: *const u16,
        startup_info: *mut StartupInfo,
        process_information: *mut ProcessInformation,
    ) -> Bool;
    fn ResumeThread(thread: Handle) -> Dword;
    fn WaitForSingleObject(handle: Handle, milliseconds: Dword) -> Dword;
    fn GetExitCodeProcess(process: Handle, code: *mut Dword) -> Bool;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> Bool;
    fn OpenProcess(access: Dword, inherit: Bool, process_id: Dword) -> Handle;
    fn CreateFileW(
        name: *const u16,
        access: Dword,
        share: Dword,
        security: *mut SecurityAttributes,
        disposition: Dword,
        flags: Dword,
        template: Handle,
    ) -> Handle;
    fn LockFileEx(
        file: Handle,
        flags: Dword,
        reserved: Dword,
        low: Dword,
        high: Dword,
        overlapped: *mut Overlapped,
    ) -> Bool;
    fn UnlockFileEx(
        file: Handle,
        reserved: Dword,
        low: Dword,
        high: Dword,
        overlapped: *mut Overlapped,
    ) -> Bool;
    fn GetFileAttributesW(name: *const u16) -> Dword;
    fn GetHandleInformation(handle: Handle, flags: *mut Dword) -> Bool;
    fn SetHandleInformation(handle: Handle, mask: Dword, flags: Dword) -> Bool;
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: Dword,
        flags: Dword,
        size: *mut usize,
    ) -> Bool;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: Dword,
        attribute: usize,
        value: *mut c_void,
        size: usize,
        previous: *mut c_void,
        returned: *mut usize,
    ) -> Bool;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process: Handle,
        class: Dword,
        info: *mut c_void,
        length: Dword,
        returned: *mut Dword,
    ) -> Long;
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn last_error(context: &str) -> String {
    format!("{context} (Win32 error {})", unsafe { GetLastError() })
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() => output.push_str(&format!("\\u{:04x}", c as u32)),
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

fn filetime_value(time: &FileTime) -> u64 {
    ((time.high as u64) << 32) | time.low as u64
}

fn process_creation_time(process: Handle) -> Result<u64, String> {
    let mut creation = unsafe { zeroed::<FileTime>() };
    let mut exit = unsafe { zeroed::<FileTime>() };
    let mut kernel = unsafe { zeroed::<FileTime>() };
    let mut user = unsafe { zeroed::<FileTime>() };
    if unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(last_error("GetProcessTimes failed"));
    }
    Ok(filetime_value(&creation))
}

fn parent_process_id() -> Result<Dword, String> {
    let mut info = unsafe { zeroed::<ProcessBasicInformation>() };
    let mut returned = 0;
    let status = unsafe {
        NtQueryInformationProcess(
            GetCurrentProcess(),
            0,
            &mut info as *mut _ as *mut c_void,
            size_of::<ProcessBasicInformation>() as Dword,
            &mut returned,
        )
    };
    if status != 0 {
        return Err(format!(
            "NtQueryInformationProcess failed (NTSTATUS {status})"
        ));
    }
    Ok(info.inherited_from_unique_process_id as usize as Dword)
}

fn quote_windows_arg(value: &str) -> String {
    if !value.is_empty() && !value.chars().any(|c| c.is_whitespace() || c == '"') {
        return value.to_owned();
    }
    let mut output = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
        } else if ch == '"' {
            output.push_str(&"\\".repeat(slashes * 2 + 1));
            output.push('"');
            slashes = 0;
        } else {
            output.push_str(&"\\".repeat(slashes));
            output.push(ch);
            slashes = 0;
        }
    }
    output.push_str(&"\\".repeat(slashes * 2));
    output.push('"');
    output
}

fn environment_block() -> Vec<u16> {
    let mut values: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "VERISILO_REAL_EXE"
                    | "VERISILO_EXIT_FILE"
                    | "VERISILO_SUPERVISOR_FILE"
                    | "VERISILO_PROFILE_LOCK_PATH"
                    | "VERISILO_JOB_NAME"
            )
        })
        .collect();
    values.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    let mut block = Vec::new();
    for (key, value) in values {
        block.extend(OsStr::new(&format!("{key}={value}")).encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

fn read_handle(buffer: &[u8], offset: usize) -> Handle {
    let mut bytes = [0u8; size_of::<Handle>()];
    bytes.copy_from_slice(&buffer[offset..offset + size_of::<Handle>()]);
    usize::from_ne_bytes(bytes) as Handle
}

fn parent_stdio() -> Result<(Vec<u8>, Vec<Handle>), String> {
    let mut parent_startup = unsafe { zeroed::<StartupInfo>() };
    unsafe { GetStartupInfoW(&mut parent_startup) };
    if !parent_startup.reserved2_ptr.is_null() && parent_startup.reserved2 != 0 {
        let length = parent_startup.reserved2 as usize;
        let buffer = unsafe { std::slice::from_raw_parts(parent_startup.reserved2_ptr, length) };
        if length < 4 {
            return Err("parent CRT stdio buffer is truncated".to_owned());
        }
        let count = u32::from_le_bytes(buffer[0..4].try_into().unwrap()) as usize;
        let required = 4usize
            .checked_add(count)
            .and_then(|value| value.checked_add(count * size_of::<Handle>()))
            .ok_or_else(|| "parent CRT stdio buffer size overflow".to_owned())?;
        if count == 0 || count > 255 || required > length {
            return Err(format!(
                "invalid parent CRT stdio buffer count={count} length={length}"
            ));
        }
        let handle_base = 4 + count;
        let handles = (0..count)
            .map(|index| read_handle(buffer, handle_base + index * size_of::<Handle>()))
            .collect();
        return Ok((buffer.to_vec(), handles));
    }

    let handles = vec![
        unsafe { GetStdHandle(STD_INPUT_HANDLE) },
        unsafe { GetStdHandle(STD_OUTPUT_HANDLE) },
        unsafe { GetStdHandle(STD_ERROR_HANDLE) },
    ];
    let count = handles.len();
    let mut buffer = vec![0u8; 4 + count + count * size_of::<Handle>()];
    buffer[0..4].copy_from_slice(&(count as u32).to_le_bytes());
    buffer[4..7].copy_from_slice(&[0x41, 0x09, 0x09]);
    for (index, handle) in handles.iter().enumerate() {
        let offset = 4 + count + index * size_of::<Handle>();
        buffer[offset..offset + size_of::<Handle>()]
            .copy_from_slice(&(*handle as usize).to_ne_bytes());
    }
    Ok((buffer, handles))
}

fn inheritable_handles(handles: &[Handle]) -> Vec<Handle> {
    let mut inherited = Vec::new();
    for handle in handles.iter().copied() {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE || inherited.contains(&handle) {
            continue;
        }
        let mut flags = 0;
        if unsafe { GetHandleInformation(handle, &mut flags) } != 0 {
            if flags & HANDLE_FLAG_INHERIT == 0 {
                unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) };
            }
            let mut updated = 0;
            if unsafe { GetHandleInformation(handle, &mut updated) } != 0
                && updated & HANDLE_FLAG_INHERIT != 0
            {
                inherited.push(handle);
            }
        }
    }
    inherited
}

struct FileLease {
    handle: Handle,
    overlapped: Overlapped,
}

impl FileLease {
    fn acquire(path: &str) -> Result<Self, String> {
        let attributes = unsafe { GetFileAttributesW(wide(path).as_ptr()) };
        if attributes != u32::MAX && attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(format!("profile lock path is a reparse point: {path}"));
        }
        let path_wide = wide(path);
        let handle = unsafe {
            CreateFileW(
                path_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null_mut(),
                OPEN_ALWAYS,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            )
        };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(last_error("CreateFileW profile lock failed"));
        }
        let mut overlapped = unsafe { zeroed::<Overlapped>() };
        overlapped.offset = 1;
        if unsafe {
            LockFileEx(
                handle,
                LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                0,
                1,
                0,
                &mut overlapped,
            )
        } == 0
        {
            unsafe { CloseHandle(handle) };
            return Err(last_error("LockFileEx supervisor byte failed"));
        }
        Ok(Self { handle, overlapped })
    }
}

impl Drop for FileLease {
    fn drop(&mut self) {
        unsafe {
            UnlockFileEx(self.handle, 0, 1, 0, &mut self.overlapped);
            CloseHandle(self.handle);
        }
    }
}

struct Job {
    handle: Handle,
    name: String,
}

impl Job {
    fn create(name: &str) -> Result<Self, String> {
        let name_wide = wide(name);
        let handle = unsafe { CreateJobObjectW(null_mut(), name_wide.as_ptr()) };
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(last_error("CreateJobObjectW failed"));
        }
        let mut limits = unsafe { zeroed::<JobObjectExtendedLimitInformation>() };
        limits.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                handle,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                &mut limits as *mut _ as *mut c_void,
                size_of::<JobObjectExtendedLimitInformation>() as Dword,
            )
        } == 0
        {
            unsafe { CloseHandle(handle) };
            return Err(last_error("SetInformationJobObject failed"));
        }
        if unsafe { AssignProcessToJobObject(handle, GetCurrentProcess()) } == 0 {
            unsafe { CloseHandle(handle) };
            return Err(last_error(
                "AssignProcessToJobObject supervisor failed; outer Job may be non-nestable",
            ));
        }
        Ok(Self {
            handle,
            name: name.to_owned(),
        })
    }

    fn assign_child(&self, process: Handle) -> Result<(), String> {
        if unsafe { AssignProcessToJobObject(self.handle, process) } != 0 {
            return Ok(());
        }
        let mut in_job = 0;
        if unsafe { IsProcessInJob(process, self.handle, &mut in_job) } != 0 && in_job != 0 {
            return Ok(());
        }
        Err(last_error("AssignProcessToJobObject child failed"))
    }

    fn terminate(&self) {
        unsafe {
            TerminateJobObject(self.handle, 1);
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn write_metadata(
    path: &str,
    job: &Job,
    child: Handle,
    child_pid: Dword,
    stdio_handles: &[Handle],
) -> Result<(), String> {
    let supervisor = unsafe { GetCurrentProcess() };
    let supervisor_time = process_creation_time(supervisor)?;
    let child_time = process_creation_time(child)?;
    let pipe_handles = (3..=4)
        .map(|fd| {
            let raw = stdio_handles
                .get(fd)
                .copied()
                .unwrap_or(INVALID_HANDLE_VALUE);
            let mut flags = 0;
            let valid = !raw.is_null()
                && raw != INVALID_HANDLE_VALUE
                && unsafe { GetHandleInformation(raw, &mut flags) } != 0;
            format!(
                "{{\"fd\":{fd},\"handle\":{:?},\"valid\":{valid},\"flags\":{flags}}}",
                raw as usize
            )
        })
        .collect::<Vec<_>>();
    let metadata = format!(
        "{{\"supervisorPid\":{},\"supervisorCreationTime100ns\":{},\"childPid\":{},\"childCreationTime100ns\":{},\"jobName\":{},\"jobKillOnClose\":true,\"jobAssignmentVerified\":true,\"processHandleEvidence\":true,\"profileLockOffset\":1,\"inheritedPipeHandles\":[{}]}}\n",
        unsafe { GetCurrentProcessId() },
        supervisor_time,
        child_pid,
        child_time,
        json_string(&job.name),
        pipe_handles.join(","),
    );
    std::fs::write(path, metadata).map_err(|error| format!("write supervisor metadata: {error}"))
}

fn wait_for_child_or_parent(child: Handle, parent: Handle, job: &Job) -> Result<Dword, String> {
    loop {
        let child_state = unsafe { WaitForSingleObject(child, 100) };
        if child_state == WAIT_OBJECT_0 {
            let mut code = 1;
            if unsafe { GetExitCodeProcess(child, &mut code) } == 0 {
                return Err(last_error("GetExitCodeProcess failed"));
            }
            return Ok(code);
        }
        if child_state != WAIT_TIMEOUT {
            return Err(format!("WaitForSingleObject child failed: {child_state}"));
        }
        if unsafe { WaitForSingleObject(parent, 0) } == WAIT_OBJECT_0 {
            job.terminate();
            unsafe { WaitForSingleObject(child, 5000) };
            return Ok(1);
        }
    }
}

fn run() -> Result<Dword, String> {
    let real_exe = std::env::var("VERISILO_REAL_EXE")
        .map_err(|_| "VERISILO_REAL_EXE is missing".to_owned())?;
    let exit_file = std::env::var("VERISILO_EXIT_FILE")
        .map_err(|_| "VERISILO_EXIT_FILE is missing".to_owned())?;
    let supervisor_file = std::env::var("VERISILO_SUPERVISOR_FILE")
        .map_err(|_| "VERISILO_SUPERVISOR_FILE is missing".to_owned())?;
    let profile_lock_path = std::env::var("VERISILO_PROFILE_LOCK_PATH")
        .map_err(|_| "VERISILO_PROFILE_LOCK_PATH is missing".to_owned())?;
    let job_name = std::env::var("VERISILO_JOB_NAME")
        .map_err(|_| "VERISILO_JOB_NAME is missing".to_owned())?;

    let _profile_lease = FileLease::acquire(&profile_lock_path)?;
    let job = Job::create(&job_name)?;
    let parent_pid = parent_process_id()?;
    let parent = unsafe { OpenProcess(SYNCHRONIZE, 0, parent_pid) };
    if parent.is_null() {
        return Err(last_error("OpenProcess parent failed"));
    }

    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut command_line = quote_windows_arg(&real_exe);
    for arg in &args {
        command_line.push(' ');
        command_line.push_str(&quote_windows_arg(arg));
    }
    let mut command_line_wide = wide(&command_line);
    let application_wide = wide(&real_exe);
    let current_directory = Path::new(&real_exe)
        .parent()
        .and_then(|value| value.to_str())
        .unwrap_or(".");
    let current_directory_wide = wide(current_directory);
    let mut environment = environment_block();
    let (mut child_stdio, stdio_handles) = parent_stdio()?;
    if stdio_handles.len() < 3 {
        return Err("parent CRT stdio buffer has fewer than three handles".to_owned());
    }
    let mut startup = unsafe { zeroed::<StartupInfo>() };
    startup.cb = size_of::<StartupInfo>() as Dword;
    startup.flags = STARTF_USESTDHANDLES;
    startup.std_input = stdio_handles[0];
    startup.std_output = stdio_handles[1];
    startup.std_error = stdio_handles[2];
    startup.reserved2 = child_stdio.len() as u16;
    startup.reserved2_ptr = child_stdio.as_mut_ptr();
    let handles = inheritable_handles(&stdio_handles);
    let mut attribute_size = 0usize;
    unsafe {
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut attribute_size);
    }
    if attribute_size == 0 {
        return Err(last_error(
            "InitializeProcThreadAttributeList sizing failed",
        ));
    }
    let word_size = size_of::<usize>();
    let attribute_words = (attribute_size + word_size - 1) / word_size;
    let mut attribute_storage = vec![0usize; attribute_words];
    let attribute_list = attribute_storage.as_mut_ptr() as *mut c_void;
    if unsafe { InitializeProcThreadAttributeList(attribute_list, 1, 0, &mut attribute_size) } == 0
    {
        return Err(last_error("InitializeProcThreadAttributeList failed"));
    }
    const PROC_THREAD_ATTRIBUTE_HANDLE_LIST: usize = 0x00020002;
    if unsafe {
        UpdateProcThreadAttribute(
            attribute_list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            handles.as_ptr() as *mut c_void,
            handles.len() * size_of::<Handle>(),
            null_mut(),
            null_mut(),
        )
    } == 0
    {
        unsafe { DeleteProcThreadAttributeList(attribute_list) };
        return Err(last_error("UpdateProcThreadAttribute handle list failed"));
    }
    #[repr(C)]
    struct StartupInfoEx {
        startup: StartupInfo,
        attribute_list: *mut c_void,
    }
    let mut startup_ex = StartupInfoEx {
        startup,
        attribute_list,
    };
    startup_ex.startup.cb = size_of::<StartupInfoEx>() as Dword;
    let mut information = unsafe { zeroed::<ProcessInformation>() };
    let created = unsafe {
        CreateProcessW(
            application_wide.as_ptr(),
            command_line_wide.as_mut_ptr(),
            null_mut(),
            null_mut(),
            1,
            CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_mut_ptr() as *mut c_void,
            current_directory_wide.as_ptr(),
            &mut startup_ex.startup,
            &mut information,
        )
    };
    unsafe { DeleteProcThreadAttributeList(attribute_list) };
    if created == 0 {
        unsafe { CloseHandle(parent) };
        return Err(last_error("CreateProcessW Camoufox failed"));
    }
    if let Err(error) = job.assign_child(information.process) {
        job.terminate();
        unsafe {
            CloseHandle(information.thread);
            CloseHandle(information.process);
            CloseHandle(parent);
        }
        return Err(error);
    }
    if unsafe { ResumeThread(information.thread) } == u32::MAX {
        job.terminate();
        unsafe {
            CloseHandle(information.thread);
            CloseHandle(information.process);
            CloseHandle(parent);
        }
        return Err(last_error("ResumeThread Camoufox failed"));
    }
    if let Err(error) = write_metadata(
        &supervisor_file,
        &job,
        information.process,
        information.process_id,
        &stdio_handles,
    ) {
        job.terminate();
        unsafe {
            CloseHandle(information.thread);
            CloseHandle(information.process);
            CloseHandle(parent);
        }
        return Err(error);
    }

    let exit_code = wait_for_child_or_parent(information.process, parent, &job)?;
    let _ = std::fs::write(
        &exit_file,
        format!(
            "{{\"exitCode\":{},\"jobName\":{}}}\n",
            exit_code,
            json_string(&job.name)
        ),
    );
    unsafe {
        CloseHandle(information.thread);
        CloseHandle(information.process);
        CloseHandle(parent);
    }
    Ok(exit_code)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code as i32),
        Err(error) => {
            eprintln!("verisilo-camoufox-supervisor: {error}");
            std::process::exit(2);
        }
    }
}
