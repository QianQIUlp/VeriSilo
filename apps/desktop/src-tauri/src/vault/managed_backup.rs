//! Same-Vault, same-machine, same-user cold backup of one Managed Silo.
use super::*;
use crate::engine::{
    Sha256State, CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256, CAMOUFOX_FORMAL_V3_ENGINE_REVISION,
};
use crate::managed_backup::{ManagedSiloBackupInspection, ManagedSiloBackupReceipt};
use aes_gcm::aead::Payload;
use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::Component;

const MAGIC: &[u8; 8] = b"VSMBK001";
const CHUNK: usize = 1024 * 1024;
const MAX_METADATA: usize = 8 * 1024 * 1024;
const MAX_FILES: u64 = 2_000_000;
const MAX_PROFILE_BYTES: u64 = 8 * 1024 * 1024 * 1024 * 1024;
const JOURNAL_PREFIX: &str = ".managed-restore-";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Snapshot {
    silo: Silo,
    backup_at: String,
    seed: String,
    artifact: StoredIdentityArtifact,
    proxy: Option<StoredProxyCredential>,
    mihomo: Option<StoredMihomoControllerSecret>,
    engine_version: String,
    engine_revision: String,
    browser_asset_sha256: String,
    profile_bytes: u64,
    file_count: u64,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

fn rejected(reason: &str) -> VaultError {
    VaultError::InvalidSilo(reason.to_owned())
}

fn managed_silo(data: &VaultData, id: Uuid) -> Result<&Silo, VaultError> {
    let silo = data
        .silos
        .iter()
        .find(|silo| silo.id == id)
        .ok_or(VaultError::SiloNotFound)?;
    if silo.adapter_id() != crate::engine::EngineAdapterId::Camoufox
        || silo.execution_target != SiloExecutionTarget::Local
    {
        return Err(rejected(
            "Only a local Managed Camoufox Silo can use this backup.",
        ));
    }
    Ok(silo)
}

fn snapshot(data: &VaultData, id: Uuid, engine_version: &str) -> Result<Snapshot, VaultError> {
    let silo = managed_silo(data, id)?.clone();
    let binding = silo
        .engine
        .camoufox_artifact_binding()
        .ok_or(VaultError::InvalidData)?;
    let artifact = data
        .identity_artifacts
        .get(&binding.artifact_id)
        .ok_or(VaultError::InvalidData)?
        .clone();
    validate_identity_artifact_record(&binding.artifact_id, &artifact)?;
    if binding.schema != artifact.schema || binding.artifact_file_sha256 != artifact.raw_sha256 {
        return Err(VaultError::InvalidData);
    }
    let seed = data
        .seed_material
        .get(&silo.seed_reference)
        .ok_or(VaultError::InvalidData)?
        .clone();
    let proxy = silo
        .network_profile
        .credential_reference()
        .map(|reference| {
            data.proxy_credentials
                .get(&reference)
                .cloned()
                .ok_or(VaultError::InvalidData)
        })
        .transpose()?;
    let mihomo = silo
        .network_profile
        .mihomo_controller_secret_reference()
        .map(|reference| {
            data.mihomo_controller_secrets
                .get(&reference)
                .cloned()
                .ok_or(VaultError::InvalidData)
        })
        .transpose()?;
    Ok(Snapshot {
        silo,
        backup_at: Utc::now().to_rfc3339(),
        seed,
        artifact,
        proxy,
        mihomo,
        engine_version: engine_version.to_owned(),
        engine_revision: CAMOUFOX_FORMAL_V3_ENGINE_REVISION.to_owned(),
        browser_asset_sha256: CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256.to_owned(),
        profile_bytes: 0,
        file_count: 0,
    })
}

fn entropy(root: &Path, id: Uuid, seed: &str) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    let canonical = fs::canonicalize(root)?;
    let mut bytes = Zeroizing::new(Vec::new());
    bytes.extend_from_slice(b"VeriSilo Managed Backup v1\0");
    bytes.extend_from_slice(id.as_bytes());
    bytes.extend_from_slice(seed.as_bytes());
    bytes.extend_from_slice(canonical.to_string_lossy().as_bytes());
    Ok(bytes)
}

fn backup_key(
    passphrase: &str,
    salt: &[u8; 16],
    secret: &[u8],
) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    validate_passphrase(passphrase)?;
    if secret.len() != 32 {
        return Err(VaultError::InvalidData);
    }
    let mut key = derive_key(passphrase, salt)?;
    for (byte, factor) in key.iter_mut().zip(secret) {
        *byte ^= factor;
    }
    Ok(key)
}

#[cfg(target_os = "windows")]
fn dpapi(
    input: &[u8],
    entropy: &[u8],
    protect: bool,
    machine: bool,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *const c_void,
            prompt: *const c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *const c_void,
            prompt: *const c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let in_blob = Blob {
        len: input
            .len()
            .try_into()
            .map_err(|_| VaultError::InvalidData)?,
        data: input.as_ptr() as *mut u8,
    };
    let entropy_blob = Blob {
        len: entropy
            .len()
            .try_into()
            .map_err(|_| VaultError::InvalidData)?,
        data: entropy.as_ptr() as *mut u8,
    };
    let mut out_blob = Blob {
        len: 0,
        data: std::ptr::null_mut(),
    };
    let result = unsafe {
        if protect {
            CryptProtectData(
                &in_blob,
                std::ptr::null(),
                &entropy_blob,
                std::ptr::null(),
                std::ptr::null(),
                1 | if machine { 4 } else { 0 },
                &mut out_blob,
            )
        } else {
            CryptUnprotectData(
                &in_blob,
                std::ptr::null_mut(),
                &entropy_blob,
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut out_blob,
            )
        }
    };
    if result == 0 {
        return Err(rejected(
            "This backup cannot be opened by this Windows user and Vault.",
        ));
    }
    let copied =
        unsafe { std::slice::from_raw_parts(out_blob.data, out_blob.len as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(out_blob.data, 0, out_blob.len as usize);
        LocalFree(out_blob.data.cast());
    }
    Ok(Zeroizing::new(copied))
}

#[cfg(target_os = "linux")]
fn dpapi(
    input: &[u8],
    entropy: &[u8],
    protect: bool,
    machine: bool,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    // The binding includes the secret seed from the unlocked encrypted Vault.
    // Linux has no DPAPI: authenticate that binding plus the machine and user
    // with AES-GCM. Neither the seed nor a wrapping key is stored in the archive.
    let machine_id = fs::read_to_string("/etc/machine-id")?;
    let machine_id = machine_id.trim();
    if machine_id.len() != 32 || !machine_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(rejected(
            "A valid Linux machine-id is required for cold backup.",
        ));
    }
    let status = fs::read_to_string("/proc/self/status")?;
    let uid = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|uids| uids.split_whitespace().nth(1))
        .ok_or(VaultError::InvalidData)?;
    let mut binding = Zeroizing::new(Vec::new());
    binding.extend_from_slice(b"VeriSilo Linux backup wrapping v1\0");
    binding.extend_from_slice(machine_id.as_bytes());
    binding.push(0);
    binding.extend_from_slice(uid.as_bytes());
    binding.push(u8::from(machine));
    binding.extend_from_slice(entropy);
    let mut digest = Sha256State::new();
    digest.update(&binding);
    let key = Zeroizing::new(digest.finalize());
    let cipher =
        Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| VaultError::CryptographicSetup)?;
    if protect {
        let nonce = random_bytes::<12>();
        let sealed = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: input,
                    aad: &binding,
                },
            )
            .map_err(|_| VaultError::InvalidData)?;
        let mut output = Zeroizing::new(nonce.to_vec());
        output.extend_from_slice(&sealed);
        Ok(output)
    } else {
        if input.len() < 28 {
            return Err(VaultError::InvalidData);
        }
        cipher
            .decrypt(
                Nonce::from_slice(&input[..12]),
                Payload {
                    msg: &input[12..],
                    aad: &binding,
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| {
                rejected("This backup cannot be opened by this Linux user, machine and Vault.")
            })
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn dpapi(
    _input: &[u8],
    _entropy: &[u8],
    _protect: bool,
    _machine: bool,
) -> Result<Zeroizing<Vec<u8>>, VaultError> {
    Err(rejected(
        "Managed Silo cold backup requires Windows or Linux.",
    ))
}

struct HashingReader<R> {
    inner: R,
    hash: Sha256State,
}
impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(out)?;
        self.hash.update(&out[..n]);
        Ok(n)
    }
}

fn read_header<R: Read>(
    input: &mut R,
    root: &Path,
    id: Uuid,
    seed: &str,
    passphrase: &str,
) -> Result<([u8; 8], Zeroizing<[u8; 32]>, Vec<u8>), VaultError> {
    let mut fixed = [0_u8; 36]; // magic, salt, nonce prefix, DPAPI blob length
    input
        .read_exact(&mut fixed)
        .map_err(|_| VaultError::InvalidData)?;
    if &fixed[..8] != MAGIC {
        return Err(VaultError::InvalidData);
    }
    let salt: [u8; 16] = fixed[8..24].try_into().unwrap();
    let nonce: [u8; 8] = fixed[24..32].try_into().unwrap();
    let blob_len = u32::from_le_bytes(fixed[32..36].try_into().unwrap()) as usize;
    if !(32..=16_384).contains(&blob_len) {
        return Err(VaultError::InvalidData);
    }
    let mut blob = vec![0; blob_len];
    input
        .read_exact(&mut blob)
        .map_err(|_| VaultError::InvalidData)?;
    let binding = entropy(root, id, seed)?;
    let user_blob = dpapi(&blob, &binding, false, true)?;
    let secret = dpapi(&user_blob, &binding, false, false)?;
    let key = backup_key(passphrase, &salt, &secret)?;
    let mut aad = fixed.to_vec();
    aad.extend_from_slice(&blob);
    Ok((nonce, key, aad))
}

struct EncryptedWriter<W> {
    out: W,
    cipher: Aes256Gcm,
    prefix: [u8; 8],
    aad: Vec<u8>,
    counter: u32,
    buffer: Vec<u8>,
}
impl<W: Write> EncryptedWriter<W> {
    fn new(out: W, key: &[u8; 32], prefix: [u8; 8], aad: Vec<u8>) -> Result<Self, VaultError> {
        Ok(Self {
            out,
            cipher: Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::CryptographicSetup)?,
            prefix,
            aad,
            counter: 0,
            buffer: Vec::with_capacity(CHUNK),
        })
    }
    fn chunk(&mut self, final_chunk: bool) -> Result<(), VaultError> {
        let mut nonce = [0_u8; 12];
        nonce[..8].copy_from_slice(&self.prefix);
        nonce[8..].copy_from_slice(&self.counter.to_le_bytes());
        let mut aad = self.aad.clone();
        aad.extend_from_slice(&self.counter.to_le_bytes());
        aad.push(u8::from(final_chunk));
        let sealed = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &self.buffer,
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::CryptographicSetup)?;
        self.out
            .write_all(&(self.buffer.len() as u32).to_le_bytes())?;
        self.out.write_all(&sealed)?;
        self.buffer.zeroize();
        self.buffer.clear();
        self.counter = self.counter.checked_add(1).ok_or(VaultError::InvalidData)?;
        Ok(())
    }
    fn finish(mut self) -> Result<W, VaultError> {
        if !self.buffer.is_empty() {
            self.chunk(false)?;
        }
        self.chunk(true)?;
        self.out.flush()?;
        Ok(self.out)
    }
}
impl<W: Write> Write for EncryptedWriter<W> {
    fn write(&mut self, mut input: &[u8]) -> std::io::Result<usize> {
        let len = input.len();
        while !input.is_empty() {
            let n = (CHUNK - self.buffer.len()).min(input.len());
            self.buffer.extend_from_slice(&input[..n]);
            input = &input[n..];
            if self.buffer.len() == CHUNK {
                self.chunk(false).map_err(std::io::Error::other)?;
            }
        }
        Ok(len)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct DecryptedReader<R> {
    input: R,
    cipher: Aes256Gcm,
    prefix: [u8; 8],
    aad: Vec<u8>,
    counter: u32,
    buffer: Zeroizing<Vec<u8>>,
    offset: usize,
    finished: bool,
}
impl<R: Read> DecryptedReader<R> {
    fn new(input: R, key: &[u8; 32], prefix: [u8; 8], aad: Vec<u8>) -> Result<Self, VaultError> {
        Ok(Self {
            input,
            cipher: Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::CryptographicSetup)?,
            prefix,
            aad,
            counter: 0,
            buffer: Zeroizing::new(Vec::new()),
            offset: 0,
            finished: false,
        })
    }
    fn next_chunk(&mut self) -> std::io::Result<()> {
        let mut length = [0_u8; 4];
        self.input.read_exact(&mut length)?;
        let len = u32::from_le_bytes(length) as usize;
        if len > CHUNK {
            return Err(std::io::Error::other("backup chunk exceeds limit"));
        }
        let mut sealed = Zeroizing::new(vec![0; len + 16]);
        self.input.read_exact(&mut sealed)?;
        let mut nonce = [0_u8; 12];
        nonce[..8].copy_from_slice(&self.prefix);
        nonce[8..].copy_from_slice(&self.counter.to_le_bytes());
        let mut aad = self.aad.clone();
        aad.extend_from_slice(&self.counter.to_le_bytes());
        aad.push(u8::from(len == 0));
        let opened = self
            .cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &sealed,
                    aad: &aad,
                },
            )
            .map_err(|_| std::io::Error::other("backup authentication failed"))?;
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| std::io::Error::other("backup counter overflow"))?;
        self.buffer = Zeroizing::new(opened);
        self.offset = 0;
        if len == 0 {
            let mut trailing = [0_u8; 1];
            if self.input.read(&mut trailing)? != 0 {
                return Err(std::io::Error::other("backup has trailing bytes"));
            }
            self.finished = true;
        }
        Ok(())
    }
}
impl<R: Read> Read for DecryptedReader<R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.offset == self.buffer.len() && !self.finished {
            self.next_chunk()?;
        }
        if self.finished {
            return Ok(0);
        }
        let n = out.len().min(self.buffer.len() - self.offset);
        out[..n].copy_from_slice(&self.buffer[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}

fn excluded(relative: &Path) -> bool {
    let parts = relative
        .iter()
        .map(|part| part.to_str().unwrap_or(""))
        .collect::<Vec<_>>();
    if matches!(parts.first().copied(), Some("identity" | "engine-state")) {
        return true;
    }
    if relative.parent() == Some(Path::new("profiles"))
        && relative
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("silo-")
                    && (name.ends_with(".lock") || name.ends_with(".lock.supervisor"))
            })
    {
        return true;
    }
    if parts.as_slice() == ["browser-data", ".verisilo-runtime.lock"]
        || parts.as_slice()
            == [
                "browser-data",
                "engines",
                "camoufox",
                ".verisilo-runtime.lock",
            ]
    {
        return true;
    }
    parts.len() == 3
        && parts[0] == "profiles"
        && [
            "parent.lock",
            ".parentlock",
            "lockfile",
            "SingletonLock",
            "SingletonCookie",
            "SingletonSocket",
            ".verisilo-runtime.lock",
        ]
        .contains(&parts[2])
}

fn profile_entries(
    root: &Path,
    mut visit: impl FnMut(&Path, &fs::Metadata) -> Result<(), VaultError>,
) -> Result<(), VaultError> {
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0_u64;
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path)?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(VaultError::UnmanagedProfile);
        }
        if path != root {
            let relative = path
                .strip_prefix(root)
                .map_err(|_| VaultError::UnmanagedProfile)?;
            if excluded(relative) {
                continue;
            }
            count += 1;
            if count > MAX_FILES * 2 {
                return Err(rejected("The Managed Profile has too many entries."));
            }
            visit(relative, &metadata)?;
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(&path)? {
                pending.push(entry?.path());
            }
        } else if !metadata.is_file() {
            return Err(VaultError::UnmanagedProfile);
        }
    }
    Ok(())
}

fn checked_relative(path: &Path) -> Result<String, VaultError> {
    let mut names = Vec::new();
    for part in path.components() {
        let Component::Normal(name) = part else {
            return Err(VaultError::InvalidData);
        };
        let name = name.to_str().ok_or(VaultError::InvalidData)?;
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', ':']) {
            return Err(VaultError::InvalidData);
        }
        names.push(name);
    }
    let joined = names.join("/");
    if joined.is_empty() || joined.len() > 32_768 {
        return Err(VaultError::InvalidData);
    }
    Ok(joined)
}

fn parsed_relative(raw: &str) -> Result<PathBuf, VaultError> {
    if raw.len() > 32_768 || raw.is_empty() {
        return Err(VaultError::InvalidData);
    }
    let mut path = PathBuf::new();
    for part in raw.split('/') {
        if part.is_empty() || part == "." || part == ".." || part.contains(['\\', ':']) {
            return Err(VaultError::InvalidData);
        }
        path.push(part);
    }
    if checked_relative(&path)? != raw {
        return Err(VaultError::InvalidData);
    }
    Ok(path)
}

fn open_regular(path: &Path) -> Result<fs::File, VaultError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
        return Err(VaultError::UnmanagedProfile);
    }
    #[cfg(target_os = "windows")]
    let file = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x0020_0000)
            .open(path)?
    };
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .custom_flags(0x20000)
            .open(path)?
    };
    #[cfg(not(any(target_os = "windows", unix)))]
    let file = fs::File::open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || metadata_is_link_or_reparse(&opened) {
        return Err(VaultError::UnmanagedProfile);
    }
    Ok(file)
}

fn write_profile<W: Write>(out: &mut W, root: &Path) -> Result<(u64, u64), VaultError> {
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    profile_entries(root, |relative, metadata| {
        let raw = checked_relative(relative)?;
        if metadata.is_dir() {
            out.write_all(&[1])?;
        } else {
            out.write_all(&[2])?;
            files += 1;
            bytes = bytes
                .checked_add(metadata.len())
                .ok_or(VaultError::InvalidData)?;
            if files > MAX_FILES || bytes > MAX_PROFILE_BYTES {
                return Err(rejected("Managed Profile exceeds the v1 backup limit."));
            }
        }
        out.write_all(&(raw.len() as u32).to_le_bytes())?;
        out.write_all(raw.as_bytes())?;
        if metadata.is_file() {
            out.write_all(&metadata.len().to_le_bytes())?;
            let path = root.join(relative);
            let file = open_regular(&path)?;
            if file.metadata()?.len() != metadata.len() {
                return Err(rejected(
                    "Managed Profile changed while backup was running.",
                ));
            }
            let copied = std::io::copy(&mut file.take(metadata.len()), out)?;
            if copied != metadata.len() {
                return Err(rejected(
                    "Managed Profile changed while backup was running.",
                ));
            }
            let current = fs::symlink_metadata(&path)?;
            if current.len() != metadata.len()
                || current.modified().ok() != metadata.modified().ok()
            {
                return Err(rejected(
                    "Managed Profile changed while backup was running.",
                ));
            }
        }
        Ok(())
    })?;
    out.write_all(&[0])?;
    Ok((bytes, files))
}

fn read_plain<R: Read>(input: &mut R, len: usize) -> Result<Vec<u8>, VaultError> {
    let mut bytes = vec![0_u8; len];
    input
        .read_exact(&mut bytes)
        .map_err(|_| VaultError::InvalidData)?;
    Ok(bytes)
}

fn read_profile<R: Read>(input: &mut R, staged: Option<&Path>) -> Result<(u64, u64), VaultError> {
    let mut seen = HashSet::new();
    let mut files = 0_u64;
    let mut bytes = 0_u64;
    loop {
        let kind = read_plain(input, 1)?[0];
        if kind == 0 {
            break;
        }
        if kind != 1 && kind != 2 {
            return Err(VaultError::InvalidData);
        }
        let raw_len = u32::from_le_bytes(read_plain(input, 4)?.try_into().unwrap()) as usize;
        if raw_len == 0 || raw_len > 32_768 {
            return Err(VaultError::InvalidData);
        }
        let raw =
            String::from_utf8(read_plain(input, raw_len)?).map_err(|_| VaultError::InvalidData)?;
        let relative = parsed_relative(&raw)?;
        if excluded(&relative) || !seen.insert(raw) || seen.len() as u64 > MAX_FILES * 2 {
            return Err(VaultError::InvalidData);
        }
        if let Some(stage) = staged {
            let destination = stage.join(&relative);
            let parent = destination.parent().ok_or(VaultError::InvalidData)?;
            fs::create_dir_all(parent)?;
            #[cfg(target_os = "windows")]
            ensure_path_ancestors_have_no_links_or_reparse_points(parent)?;
            if kind == 1 {
                fs::create_dir(&destination)?;
            }
        }
        if kind == 2 {
            let len = u64::from_le_bytes(read_plain(input, 8)?.try_into().unwrap());
            files += 1;
            bytes = bytes.checked_add(len).ok_or(VaultError::InvalidData)?;
            if files > MAX_FILES || bytes > MAX_PROFILE_BYTES {
                return Err(VaultError::InvalidData);
            }
            let mut destination = if let Some(stage) = staged {
                Some(
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(stage.join(&relative))?,
                )
            } else {
                None
            };
            let mut remaining = len;
            let mut buffer = vec![0_u8; CHUNK];
            while remaining > 0 {
                let n = (remaining as usize).min(CHUNK);
                input
                    .read_exact(&mut buffer[..n])
                    .map_err(|_| VaultError::InvalidData)?;
                if let Some(file) = destination.as_mut() {
                    file.write_all(&buffer[..n])?;
                }
                remaining -= n as u64;
            }
            if let Some(file) = destination {
                file.sync_all()?;
            }
        }
    }
    let mut trailing = [0_u8; 1];
    if input
        .read(&mut trailing)
        .map_err(|_| VaultError::InvalidData)?
        != 0
    {
        return Err(VaultError::InvalidData);
    }
    Ok((bytes, files))
}

fn digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(target_os = "windows")]
fn publish_new(source: &Path, destination: &Path) -> Result<(), VaultError> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }
    let from = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let to = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0x8) } == 0 {
        if fs::symlink_metadata(destination).is_ok() {
            return Err(VaultError::BackupDestinationExists);
        }
        return Err(VaultError::Filesystem(std::io::Error::last_os_error()));
    }
    Ok(())
}
#[cfg(unix)]
fn publish_new(source: &Path, destination: &Path) -> Result<(), VaultError> {
    // Both paths have the same parent. Linking publishes the complete file
    // atomically and refuses to replace a destination created concurrently.
    fs::hard_link(source, destination).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            VaultError::BackupDestinationExists
        } else {
            VaultError::Filesystem(error)
        }
    })?;
    fs::remove_file(source)?;
    fs::File::open(destination.parent().ok_or(VaultError::InvalidData)?)?.sync_all()?;
    Ok(())
}

fn read_archive(
    file: &mut fs::File,
    root: &Path,
    id: Uuid,
    seed: &str,
    passphrase: &str,
    staged: Option<&Path>,
) -> Result<(Snapshot, String), VaultError> {
    file.seek(SeekFrom::Start(0))?;
    let mut hashed = HashingReader {
        inner: file,
        hash: Sha256State::new(),
    };
    let (prefix, key, aad) = read_header(&mut hashed, root, id, seed, passphrase)?;
    let mut clear = DecryptedReader::new(&mut hashed, &key, prefix, aad)?;
    let metadata_len = u32::from_le_bytes(read_plain(&mut clear, 4)?.try_into().unwrap()) as usize;
    if metadata_len == 0 || metadata_len > MAX_METADATA {
        return Err(VaultError::InvalidData);
    }
    let mut metadata = Zeroizing::new(read_plain(&mut clear, metadata_len)?);
    let snapshot: Snapshot =
        serde_json::from_slice(&metadata).map_err(|_| VaultError::InvalidData)?;
    metadata.zeroize();
    if snapshot.silo.id != id || snapshot.seed != seed {
        return Err(rejected("Backup belongs to another Silo or Vault."));
    }
    let (bytes, files) = read_profile(&mut clear, staged)?;
    if snapshot.profile_bytes != bytes || snapshot.file_count != files {
        return Err(VaultError::InvalidData);
    }
    drop(clear);
    Ok((snapshot, digest_hex(hashed.hash.finalize())))
}

fn host_profile_root(managed: &Path, id: Uuid) -> PathBuf {
    managed
        .join("profiles")
        .join(format!("silo-{}", id.simple()))
}

struct ManagedLease {
    _browser: BrowserProfileLease,
    #[cfg(target_os = "windows")]
    _native: Vec<WindowsProfileFileLock>,
}

fn profile_lease(
    vault: &mut VaultRuntime,
    managed: &Path,
    id: Uuid,
) -> Result<ManagedLease, VaultError> {
    let mut directories = vault
        .silo_by_id(id)?
        .all_engine_profile_directories()
        .to_vec();
    directories.push(host_profile_root(managed, id));
    let browser = BrowserProfileLease::acquire(&directories)?;
    #[cfg(target_os = "windows")]
    {
        let mut native = Vec::new();
        let host_parent = managed.join("profiles");
        if host_parent.is_dir() {
            native.push(
                WindowsProfileFileLock::acquire(
                    &host_parent.join(format!("silo-{}.lock", id.simple())),
                )
                .map_err(|_| VaultError::SiloProfileInUse)?,
            );
        }
        for directory in &directories {
            for name in ["parent.lock", ".parentlock"] {
                let path = directory.join(name);
                match fs::symlink_metadata(&path) {
                    Ok(metadata)
                        if metadata.is_file() && !metadata_is_link_or_reparse(&metadata) =>
                    {
                        native.push(
                            WindowsProfileFileLock::acquire(&path)
                                .map_err(|_| VaultError::SiloProfileInUse)?,
                        );
                    }
                    Ok(_) => return Err(VaultError::UnmanagedProfile),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(ManagedLease {
            _browser: browser,
            _native: native,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(ManagedLease { _browser: browser })
    }
}

fn archive_path(
    root: &Path,
    id: Uuid,
    selected: &Path,
    output: bool,
) -> Result<PathBuf, VaultError> {
    let parent = selected.parent().ok_or(VaultError::InvalidData)?;
    let canonical_parent = fs::canonicalize(parent)?;
    #[cfg(target_os = "windows")]
    ensure_path_ancestors_have_no_links_or_reparse_points(parent)?;
    let silos = fs::canonicalize(root.join("silos"))?;
    if canonical_parent.starts_with(&silos) {
        return Err(rejected("Keep the backup outside Managed Silo storage."));
    }
    let name = selected.file_name().ok_or(VaultError::InvalidData)?;
    let path = canonical_parent.join(name);
    if output {
        if fs::symlink_metadata(&path).is_ok() {
            return Err(VaultError::BackupDestinationExists);
        }
    } else {
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_file() || metadata_is_link_or_reparse(&metadata) {
            return Err(VaultError::InvalidData);
        }
    }
    let _ = id;
    Ok(path)
}

fn check_snapshot(
    root: &Path,
    id: Uuid,
    current: &Silo,
    seed: &str,
    snap: &Snapshot,
    current_engine_version: &str,
) -> Result<(), VaultError> {
    if snap.silo.id != id
        || snap.silo.seed_reference != current.seed_reference
        || snap.seed != seed
        || snap.engine_version != current_engine_version
        || snap.engine_version == "not-installed"
        || snap.engine_revision != CAMOUFOX_FORMAL_V3_ENGINE_REVISION
        || snap.browser_asset_sha256 != CAMOUFOX_FORMAL_V3_BROWSER_ASSET_SHA256
        || snap.silo.adapter_id() != crate::engine::EngineAdapterId::Camoufox
        || snap.silo.execution_target != SiloExecutionTarget::Local
        || snap.silo.profile_directory != current.profile_directory
    {
        return Err(rejected(
            "Backup is not compatible with this Silo, Vault, or active engine version.",
        ));
    }
    let _managed =
        verified_managed_silo_directory(root, id, Path::new(&current.profile_directory))?;
    let binding = snap
        .silo
        .engine
        .camoufox_artifact_binding()
        .ok_or(VaultError::InvalidData)?;
    validate_identity_artifact_record(&binding.artifact_id, &snap.artifact)?;
    if binding.schema != snap.artifact.schema
        || binding.artifact_file_sha256 != snap.artifact.raw_sha256
        || snap.silo.network_profile.credential_reference().is_some() != snap.proxy.is_some()
        || snap
            .silo
            .network_profile
            .mihomo_controller_secret_reference()
            .is_some()
            != snap.mihomo.is_some()
    {
        return Err(VaultError::InvalidData);
    }
    if STANDARD_NO_PAD
        .decode(snap.seed.as_bytes())
        .ok()
        .is_none_or(|raw| raw.len() != 32)
    {
        return Err(VaultError::InvalidData);
    }
    Ok(())
}

fn network_summary(profile: &NetworkProfile) -> String {
    match profile {
        NetworkProfile::Direct { .. } => "Direct".to_owned(),
        NetworkProfile::FixedProxy {
            scheme, host, port, ..
        } => format!("{scheme:?} {host}:{port}"),
        NetworkProfile::Pac { .. } => "PAC".to_owned(),
    }
}

impl VaultRuntime {
    pub fn backup_managed_silo(
        &mut self,
        root: &Path,
        id: Uuid,
        selected: &Path,
        passphrase: &str,
        engine_version: &str,
    ) -> Result<ManagedSiloBackupReceipt, VaultError> {
        validate_passphrase(passphrase)?;
        self.record_activity()?;
        let data = &self.unlocked_without_activity()?.data;
        let mut snap = snapshot(data, id, engine_version)?;
        if engine_version == "not-installed" {
            return Err(rejected("A compatible Camoufox engine must be installed."));
        }
        let managed =
            verified_managed_silo_directory(root, id, Path::new(&snap.silo.profile_directory))?;
        let host_profile = host_profile_root(&managed, id);
        if !host_profile.is_dir() {
            return Err(rejected(
                "The Managed Profile is missing and cannot be backed up.",
            ));
        }
        let destination = archive_path(root, id, selected, true)?;
        let lease = profile_lease(self, &managed, id)?;
        // Count first so the authenticated metadata binds the exact stream length.
        profile_entries(&managed, |_, metadata| {
            if metadata.is_file() {
                snap.file_count += 1;
                snap.profile_bytes = snap
                    .profile_bytes
                    .checked_add(metadata.len())
                    .ok_or(VaultError::InvalidData)?;
                if snap.file_count > MAX_FILES || snap.profile_bytes > MAX_PROFILE_BYTES {
                    return Err(rejected("Managed Profile exceeds the v1 backup limit."));
                }
            }
            Ok(())
        })?;
        let metadata = Zeroizing::new(serde_json::to_vec(&snap)?);
        if metadata.len() > MAX_METADATA {
            return Err(VaultError::InvalidData);
        }
        let secret = Zeroizing::new(random_bytes::<32>());
        let salt = random_bytes::<16>();
        let binding = entropy(root, id, &snap.seed)?;
        let key = backup_key(passphrase, &salt, secret.as_ref())?;
        let user_blob = dpapi(secret.as_ref(), &binding, true, false)?;
        let machine_blob = dpapi(&user_blob, &binding, true, true)?;
        let prefix = random_bytes::<8>();
        let mut header = Vec::new();
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&salt);
        header.extend_from_slice(&prefix);
        header.extend_from_slice(&(machine_blob.len() as u32).to_le_bytes());
        header.extend_from_slice(&machine_blob);
        let temporary = destination.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut options = fs::OpenOptions::new();
            options.write(true).read(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temporary)?;
            file.write_all(&header)?;
            let mut writer = EncryptedWriter::new(file, &key, prefix, header)?;
            writer.write_all(&(metadata.len() as u32).to_le_bytes())?;
            writer.write_all(&metadata)?;
            let actual = write_profile(&mut writer, &managed)?;
            if actual != (snap.profile_bytes, snap.file_count) {
                return Err(rejected(
                    "Managed Profile changed while backup was running.",
                ));
            }
            let file = writer.finish()?;
            file.sync_all()?;
            let bytes = file.metadata()?.len();
            publish_new(&temporary, &destination)?;
            Ok(ManagedSiloBackupReceipt {
                silo_id: id,
                destination_path: destination.to_string_lossy().into_owned(),
                bytes,
                profile_bytes: snap.profile_bytes,
                file_count: snap.file_count,
            })
        })();
        drop(lease);
        let _ = fs::remove_file(temporary);
        result
    }

    pub fn inspect_managed_silo_backup(
        &mut self,
        root: &Path,
        id: Uuid,
        selected: &Path,
        passphrase: &str,
        engine_version: &str,
    ) -> Result<ManagedSiloBackupInspection, VaultError> {
        self.record_activity()?;
        let current = managed_silo(&self.unlocked_without_activity()?.data, id)?.clone();
        let seed = Zeroizing::new(
            self.unlocked_without_activity()?
                .data
                .seed_material
                .get(&current.seed_reference)
                .ok_or(VaultError::InvalidData)?
                .clone(),
        );
        let source = archive_path(root, id, selected, false)?;
        let mut file = open_regular(&source)?;
        let (snap, digest) = read_archive(&mut file, root, id, &seed, passphrase, None)?;
        check_snapshot(root, id, &current, &seed, &snap, engine_version)?;
        Ok(ManagedSiloBackupInspection {
            silo_id: id,
            silo_name: snap.silo.name.clone(),
            created_at: snap.backup_at.clone(),
            artifact_id: snap.artifact.artifact_id.clone(),
            artifact_sha256: snap.artifact.raw_sha256.clone(),
            engine_version: snap.engine_version.clone(),
            profile_bytes: snap.profile_bytes,
            file_count: snap.file_count,
            archive_sha256: digest,
            network_summary: network_summary(&snap.silo.network_profile),
        })
    }

    pub fn restore_managed_silo_backup(
        &mut self,
        root: &Path,
        id: Uuid,
        selected: &Path,
        passphrase: &str,
        expected_digest: &str,
        confirm_overwrite: bool,
        engine_version: &str,
    ) -> Result<Silo, VaultError> {
        if !confirm_overwrite {
            return Err(VaultError::RestoreOverwriteNotConfirmed);
        }
        if expected_digest.len() != 64
            || !expected_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err(VaultError::InvalidData);
        }
        self.record_activity()?;
        recover_interrupted_managed_restore(root)?;
        let current = managed_silo(&self.unlocked_without_activity()?.data, id)?.clone();
        let seed = Zeroizing::new(
            self.unlocked_without_activity()?
                .data
                .seed_material
                .get(&current.seed_reference)
                .ok_or(VaultError::InvalidData)?
                .clone(),
        );
        let managed =
            verified_managed_silo_directory(root, id, Path::new(&current.profile_directory))?;
        let old_exists = match fs::symlink_metadata(&managed) {
            Ok(metadata) if metadata.is_dir() && !metadata_is_link_or_reparse(&metadata) => true,
            Ok(_) => return Err(VaultError::UnmanagedProfile),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        let source = archive_path(root, id, selected, false)?;
        let mut file = open_regular(&source)?;
        let journal = journal_path(root, id);
        fs::create_dir(&journal)?;
        let staged = journal.join("staged-managed");
        let prepared = (|| {
            fs::create_dir(&staged)?;
            let (mut snap, digest) =
                read_archive(&mut file, root, id, &seed, passphrase, Some(&staged))?;
            if digest != expected_digest {
                return Err(rejected(
                    "Backup changed since inspection; inspect it again.",
                ));
            }
            check_snapshot(root, id, &current, &seed, &snap, engine_version)?;
            let staged_host = host_profile_root(&staged, id);
            if !staged_host.is_dir() {
                return Err(VaultError::InvalidData);
            }
            let identity = staged.join("identity");
            fs::create_dir(&identity)?;
            let artifact_path = identity.join(format!("{}.json", snap.artifact.artifact_id));
            ensure_exact_materialized_file(&artifact_path, snap.artifact.raw_json.as_bytes())?;
            let sidecar = format!(
                "{}  {}.json\n",
                snap.artifact.raw_sha256, snap.artifact.artifact_id
            );
            ensure_exact_materialized_file(
                &identity.join(format!("{}.json.sha256", snap.artifact.artifact_id)),
                sidecar.as_bytes(),
            )?;
            let mut data = self.unlocked_without_activity()?.data.clone();
            let index = data
                .silos
                .iter()
                .position(|silo| silo.id == id)
                .ok_or(VaultError::SiloNotFound)?;
            let old_silo = data.silos[index].clone();
            // The UUID, creation identity and lock history remain monotonic.
            snap.silo.profile_directory = current.profile_directory.clone();
            snap.silo.identity_locked_at =
                current.identity_locked_at.or(snap.silo.identity_locked_at);
            snap.silo.archived_at = current.archived_at;
            if let Some(secret) = snap.proxy.take() {
                let reference = Uuid::new_v4();
                snap.silo
                    .network_profile
                    .set_credential_reference(reference)
                    .map_err(|_| VaultError::InvalidData)?;
                data.proxy_credentials.insert(reference, secret);
            }
            if let Some(secret) = snap.mihomo.take() {
                let reference = Uuid::new_v4();
                snap.silo
                    .network_profile
                    .set_mihomo_controller_secret_reference(reference)
                    .map_err(|_| VaultError::InvalidData)?;
                data.mihomo_controller_secrets.insert(reference, secret);
            }
            let artifact_id = snap.artifact.artifact_id.clone();
            if data.silos.iter().any(|silo| {
                silo.id != id
                    && silo
                        .engine
                        .camoufox_artifact_binding()
                        .is_some_and(|binding| binding.artifact_id == artifact_id)
            }) && data
                .identity_artifacts
                .get(&artifact_id)
                .is_some_and(|stored| stored != &snap.artifact)
            {
                return Err(VaultError::RestoreIdentityConflict);
            }
            data.identity_artifacts
                .insert(artifact_id, snap.artifact.clone());
            data.silos[index] = snap.silo.clone();
            remove_unreferenced_secrets(
                &mut data,
                old_silo.network_profile.credential_reference(),
                old_silo
                    .network_profile
                    .mihomo_controller_secret_reference(),
            );
            remove_unreferenced_identity_artifact(
                &mut data,
                old_silo
                    .engine
                    .camoufox_artifact_binding()
                    .map(|binding| binding.artifact_id.as_str()),
            );
            data.recent_runs.remove(&id);
            data.network_evidence.retain(|entry| entry.silo_id != id);
            validate_vault_data(&data)?;
            let lease = profile_lease(self, &managed, id)?;
            ensure_tree_has_no_links_or_reparse_points(&staged)?;
            // The old tree may contain a transient Camoufox package junction.
            if old_exists {
                remove_transient_camoufox_cache_links(
                    &managed.join("engine-state").join("camoufox-cache"),
                )?;
                ensure_tree_has_no_links_or_reparse_points(&managed)?;
            }
            drop(lease);
            // The rollback envelope must represent the validated unlocked
            // state, even if the on-disk file was changed independently.
            self.persist(root)?;
            let old_vault = journal.join("old-vault.json");
            let mut vault_source = open_regular(&vault_path(root))?;
            let mut vault_copy = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&old_vault)?;
            std::io::copy(&mut vault_source, &mut vault_copy)?;
            vault_copy.sync_all()?;
            if !old_exists {
                write_marker(&journal.join("old-missing"))?;
            }
            write_marker(&journal.join("prepared"))?;
            Ok((data, snap.silo.clone()))
        })();
        let (data, restored) = match prepared {
            Ok(value) => value,
            Err(error) => {
                let _ = recover_one(root, id);
                return Err(error);
            }
        };
        let transition = (|| {
            if old_exists {
                fs::rename(&managed, journal.join("old-managed"))?;
            }
            fs::rename(&staged, &managed)?;
            self.persist_data(root, &data)?;
            write_marker(&journal.join("committed"))?;
            Ok(())
        })();
        if let Err(error) = transition {
            if let Err(recovery_error) = recover_one(root, id) {
                self.lock();
                return Err(recovery_error);
            }
            return Err(error);
        }
        let deadline = self.auto_lock_time();
        let unlocked = self
            .unlocked
            .as_mut()
            .expect("committed restore retains the unlocked Vault");
        unlocked.data = data;
        unlocked.auto_lock_at = deadline;
        let _ = cleanup_committed(&journal);
        Ok(restored)
    }
}

fn journal_path(root: &Path, id: Uuid) -> PathBuf {
    root.join("silos").join(format!("{JOURNAL_PREFIX}{id}"))
}

fn write_marker(path: &Path) -> Result<(), VaultError> {
    let mut marker = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    marker.write_all(b"v1")?;
    marker.sync_all()?;
    Ok(())
}

fn remove_safe_tree(path: &Path) -> Result<(), VaultError> {
    if fs::symlink_metadata(path).is_err() {
        return Ok(());
    }
    remove_transient_camoufox_cache_links(&path.join("engine-state").join("camoufox-cache"))?;
    ensure_tree_has_no_links_or_reparse_points(path)?;
    fs::remove_dir_all(path)?;
    Ok(())
}

fn cleanup_committed(journal: &Path) -> Result<(), VaultError> {
    remove_safe_tree(&journal.join("old-managed"))?;
    remove_safe_tree(&journal.join("discarded-managed"))?;
    remove_safe_tree(&journal.join("staged-managed"))?;
    // Keep the commit marker until rollback inputs are gone. A crash during
    // cleanup must never reinterpret a committed Profile as uncommitted.
    if journal.join("prepared").exists() {
        fs::remove_file(journal.join("prepared"))?;
    }
    fs::remove_file(journal.join("committed"))?;
    fs::remove_dir_all(journal)?;
    Ok(())
}

fn recover_one(root: &Path, id: Uuid) -> Result<(), VaultError> {
    let journal = journal_path(root, id);
    let metadata = match fs::symlink_metadata(&journal) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || metadata_is_link_or_reparse(&metadata) {
        return Err(VaultError::UnmanagedProfile);
    }
    if journal.join("committed").exists() {
        let _ = cleanup_committed(&journal);
        return Ok(());
    }
    if !journal.join("prepared").exists() {
        return remove_safe_tree(&journal);
    }
    let managed = root.join("silos").join(id.to_string());
    let old = journal.join("old-managed");
    let discarded = journal.join("discarded-managed");
    let staged_still_present = journal.join("staged-managed").exists();
    // The destination may have been created by another process before our
    // stage rename. Never classify that path as ours and delete it.
    if staged_still_present
        && managed.exists()
        && (old.exists() || journal.join("old-missing").exists())
    {
        return Err(rejected("Managed Profile changed during restore; recovery requires resolving the conflicting directory."));
    }
    if old.exists() {
        if managed.exists() {
            fs::rename(&managed, &discarded)?;
        }
        if let Err(error) = fs::rename(&old, &managed) {
            if discarded.exists() {
                let _ = fs::rename(&discarded, &managed);
            }
            return Err(error.into());
        }
    } else if journal.join("old-missing").exists() {
        remove_safe_tree(&managed)?;
    } else if !managed.exists() {
        return Err(VaultError::InvalidData);
    }
    let old_vault = journal.join("old-vault.json");
    let mut source = open_regular(&old_vault)?;
    let temporary = journal.join("vault-rollback.tmp");
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    std::io::copy(&mut source, &mut target)?;
    target.sync_all()?;
    drop(target);
    replace_file(&temporary, &vault_path(root))?;
    remove_safe_tree(&discarded)?;
    fs::remove_file(journal.join("prepared"))?;
    remove_safe_tree(&journal)
}

pub(super) fn recover_interrupted_managed_restore(root: &Path) -> Result<(), VaultError> {
    let silos = root.join("silos");
    let entries = match fs::read_dir(&silos) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(raw) = name.strip_prefix(JOURNAL_PREFIX) {
            let id = Uuid::parse_str(raw).map_err(|_| VaultError::InvalidData)?;
            if journal_path(root, id) != entry.path() {
                return Err(VaultError::InvalidData);
            }
            recover_one(root, id)?;
        }
    }
    Ok(())
}

#[cfg(all(test, any(target_os = "windows", target_os = "linux")))]
mod tests {
    use super::*;
    use crate::domain::CreateManagedSiloInput;

    const PASSPHRASE: &str = "managed backup password for tests";
    const VAULT_PASSWORD: &str = "managed vault password for tests";
    const VERSION: &str = "123.4.5";

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_wrapping_authenticates_the_vault_binding_and_scope() {
        let input = [7; 32];
        let wrapped = dpapi(&input, b"secret-vault-binding", true, false).unwrap();
        assert_eq!(
            dpapi(&wrapped, b"secret-vault-binding", false, false)
                .unwrap()
                .as_slice(),
            input
        );
        assert!(dpapi(&wrapped, b"different-vault-binding", false, false).is_err());
        assert!(dpapi(&wrapped, b"secret-vault-binding", false, true).is_err());
        let mut corrupt = wrapped.to_vec();
        *corrupt.last_mut().unwrap() ^= 1;
        assert!(dpapi(&corrupt, b"secret-vault-binding", false, false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn publication_never_overwrites_an_existing_destination() {
        let root = root();
        let source = root.join("temporary");
        let destination = root.join("backup");
        fs::write(&source, b"complete").unwrap();
        fs::write(&destination, b"existing").unwrap();
        assert!(matches!(
            publish_new(&source, &destination),
            Err(VaultError::BackupDestinationExists)
        ));
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
        fs::remove_file(&destination).unwrap();
        publish_new(&source, &destination).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"complete");
        assert!(!source.exists());
        fs::remove_dir_all(root).unwrap();
    }

    fn root() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("verisilo-managed-backup-test-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    fn create_managed(vault: &mut VaultRuntime, root: &Path, name: &str) -> Silo {
        let artifact_id = format!("identity-{}", Uuid::new_v4().simple());
        let schema = CAMOUFOX_ARTIFACT_SCHEMA_V5.to_owned();
        let raw_json = format!(r#"{{"artifactId":"{artifact_id}","schema":"{schema}"}}"#);
        let artifact = StoredIdentityArtifact {
            artifact_id,
            schema,
            raw_sha256: sha256_hex_bytes(raw_json.as_bytes()),
            raw_json,
        };
        let input = CreateManagedSiloInput {
            name: name.to_owned(),
            color: "#4f46e5".to_owned(),
            identity_preset: ManagedIdentityPreset::BalancedEnUs,
            follow_network_exit: false,
            screen_width: 1280,
            screen_height: 800,
            hardware_concurrency: None,
            gpu_preset: None,
            timezone: None,
            network_profile: NetworkProfile::Direct {
                proxy_required: false,
            },
            proxy_credentials: None,
            mihomo_controller_secret: None,
        };
        let silo = vault
            .create_managed_silo(root, input, artifact, &[7; 32])
            .unwrap();
        let managed = root.join("silos").join(silo.id.to_string());
        let profile = host_profile_root(&managed, silo.id);
        fs::create_dir_all(profile.join("storage/default/example.com")).unwrap();
        fs::write(
            profile.join("storage/default/example.com/cookies.sqlite"),
            b"session-cookie",
        )
        .unwrap();
        silo
    }

    fn create_proxy_managed(
        vault: &mut VaultRuntime,
        root: &Path,
        name: &str,
        password: &str,
    ) -> Silo {
        let artifact_id = format!("identity-{}", Uuid::new_v4().simple());
        let schema = CAMOUFOX_ARTIFACT_SCHEMA_V6.to_owned();
        let raw_json = format!(r#"{{"artifactId":"{artifact_id}","schema":"{schema}"}}"#);
        let artifact = StoredIdentityArtifact {
            artifact_id,
            schema,
            raw_sha256: sha256_hex_bytes(raw_json.as_bytes()),
            raw_json,
        };
        let input = CreateManagedSiloInput {
            name: name.to_owned(),
            color: "#4f46e5".to_owned(),
            identity_preset: ManagedIdentityPreset::BalancedEnUs,
            follow_network_exit: true,
            screen_width: 1280,
            screen_height: 800,
            hardware_concurrency: None,
            gpu_preset: None,
            timezone: None,
            network_profile: NetworkProfile::FixedProxy {
                proxy_required: true,
                scheme: ProxyScheme::Socks5,
                host: "proxy.example.test".to_owned(),
                port: 1080,
                bypass_list: Vec::new(),
                credential_reference: None,
                external_mihomo: None,
            },
            proxy_credentials: Some(crate::domain::ProxyCredentialsInput {
                username: name.to_owned(),
                password: password.to_owned(),
            }),
            mihomo_controller_secret: None,
        };
        let silo = vault
            .create_managed_silo(root, input, artifact, &[7; 32])
            .unwrap();
        let managed = root.join("silos").join(silo.id.to_string());
        fs::create_dir_all(host_profile_root(&managed, silo.id)).unwrap();
        silo
    }

    #[test]
    fn round_trip_restores_host_profile_without_touching_other_silo() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let a = create_managed(&mut vault, &root, "A");
        let b = create_managed(&mut vault, &root, "B");
        let a_profile = host_profile_root(&root.join("silos").join(a.id.to_string()), a.id);
        let b_profile = host_profile_root(&root.join("silos").join(b.id.to_string()), b.id);
        let a_cookie = a_profile.join("storage/default/example.com/cookies.sqlite");
        let b_cookie = b_profile.join("storage/default/example.com/cookies.sqlite");
        let archive = root.join("backup.vsm");
        let receipt = vault
            .backup_managed_silo(&root, a.id, &archive, PASSPHRASE, VERSION)
            .unwrap();
        assert!(receipt.profile_bytes >= b"session-cookie".len() as u64);
        assert!(vault
            .inspect_managed_silo_backup(
                &root,
                a.id,
                &archive,
                "wrong password long enough",
                VERSION
            )
            .is_err());
        assert!(vault
            .inspect_managed_silo_backup(&root, b.id, &archive, PASSPHRASE, VERSION)
            .is_err());
        assert!(vault
            .inspect_managed_silo_backup(&root, a.id, &archive, PASSPHRASE, "other-engine")
            .is_err());
        let mut incompatible =
            snapshot(&vault.unlocked.as_ref().unwrap().data, a.id, VERSION).unwrap();
        incompatible.browser_asset_sha256 = "0".repeat(64);
        assert!(
            check_snapshot(&root, a.id, &a, &incompatible.seed, &incompatible, VERSION).is_err()
        );
        let inspection = vault
            .inspect_managed_silo_backup(&root, a.id, &archive, PASSPHRASE, VERSION)
            .unwrap();
        assert_eq!(inspection.silo_id, a.id);
        assert_eq!(inspection.profile_bytes, receipt.profile_bytes);
        let raw = fs::read(&archive).unwrap();
        assert_eq!(
            inspection.archive_sha256,
            crate::engine::sha256_hex_bytes(&raw)
        );
        fs::write(root.join("truncated.vsm"), &raw[..raw.len() - 1]).unwrap();
        assert!(vault
            .inspect_managed_silo_backup(
                &root,
                a.id,
                &root.join("truncated.vsm"),
                PASSPHRASE,
                VERSION
            )
            .is_err());
        let mut corrupted = raw;
        let last = corrupted.len() - 1;
        corrupted[last] ^= 1;
        fs::write(root.join("corrupt.vsm"), corrupted).unwrap();
        assert!(vault
            .inspect_managed_silo_backup(
                &root,
                a.id,
                &root.join("corrupt.vsm"),
                PASSPHRASE,
                VERSION
            )
            .is_err());
        fs::write(&a_cookie, b"changed-cookie").unwrap();
        let b_before = fs::read(&b_cookie).unwrap();
        assert!(vault
            .restore_managed_silo_backup(
                &root,
                a.id,
                &archive,
                PASSPHRASE,
                &inspection.archive_sha256,
                false,
                VERSION
            )
            .is_err());
        assert!(vault
            .restore_managed_silo_backup(
                &root,
                a.id,
                &archive,
                PASSPHRASE,
                &"0".repeat(64),
                true,
                VERSION
            )
            .is_err());
        assert_eq!(fs::read(&a_cookie).unwrap(), b"changed-cookie");
        vault
            .change_passphrase(&root, VAULT_PASSWORD, "rotated vault password for tests")
            .unwrap();
        vault.lock();
        vault
            .unlock(&root, "rotated vault password for tests")
            .unwrap();
        vault.archive_silo(&root, a.id, false).unwrap();
        let restored = vault
            .restore_managed_silo_backup(
                &root,
                a.id,
                &archive,
                PASSPHRASE,
                &inspection.archive_sha256,
                true,
                VERSION,
            )
            .unwrap();
        assert_eq!(restored.id, a.id);
        assert!(restored.archived_at.is_some());
        assert_eq!(fs::read(&a_cookie).unwrap(), b"session-cookie");
        assert_eq!(fs::read(&b_cookie).unwrap(), b_before);
        assert_eq!(vault.list_silos().unwrap().len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_managed_directory_can_be_restored_from_current_vault_metadata() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let silo = create_managed(&mut vault, &root, "A");
        let managed = root.join("silos").join(silo.id.to_string());
        let archive = root.join("backup.vsm");
        vault
            .backup_managed_silo(&root, silo.id, &archive, PASSPHRASE, VERSION)
            .unwrap();
        let inspected = vault
            .inspect_managed_silo_backup(&root, silo.id, &archive, PASSPHRASE, VERSION)
            .unwrap();
        fs::remove_dir_all(&managed).unwrap();
        vault
            .restore_managed_silo_backup(
                &root,
                silo.id,
                &archive,
                PASSPHRASE,
                &inspected.archive_sha256,
                true,
                VERSION,
            )
            .unwrap();
        assert_eq!(
            fs::read(
                host_profile_root(&managed, silo.id)
                    .join("storage/default/example.com/cookies.sqlite")
            )
            .unwrap(),
            b"session-cookie"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn required_proxy_secret_restores_without_changing_shared_secret_owner() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let a = create_proxy_managed(&mut vault, &root, "A", "alpha-secret");
        let b = create_proxy_managed(&mut vault, &root, "B", "bravo-secret");
        let archive = root.join("proxy.vsm");
        vault
            .backup_managed_silo(&root, a.id, &archive, PASSPHRASE, VERSION)
            .unwrap();
        let inspection = vault
            .inspect_managed_silo_backup(&root, a.id, &archive, PASSPHRASE, VERSION)
            .unwrap();
        let mut data = vault.unlocked.as_ref().unwrap().data.clone();
        let a_ref = data
            .silos
            .iter()
            .find(|silo| silo.id == a.id)
            .unwrap()
            .network_profile
            .credential_reference()
            .unwrap();
        let b_ref = data
            .silos
            .iter()
            .find(|silo| silo.id == b.id)
            .unwrap()
            .network_profile
            .credential_reference()
            .unwrap();
        data.silos
            .iter_mut()
            .find(|silo| silo.id == b.id)
            .unwrap()
            .network_profile
            .set_credential_reference(a_ref)
            .unwrap();
        data.proxy_credentials.remove(&b_ref);
        let new_ref = Uuid::new_v4();
        data.silos
            .iter_mut()
            .find(|silo| silo.id == a.id)
            .unwrap()
            .network_profile
            .set_credential_reference(new_ref)
            .unwrap();
        data.proxy_credentials.insert(
            new_ref,
            StoredProxyCredential {
                username: "A".to_owned(),
                password: "changed-secret".to_owned(),
            },
        );
        validate_vault_data(&data).unwrap();
        vault.persist_data(&root, &data).unwrap();
        vault.unlocked.as_mut().unwrap().data = data;
        let mut missing_secret =
            snapshot(&vault.unlocked.as_ref().unwrap().data, a.id, VERSION).unwrap();
        missing_secret.proxy = None;
        assert!(check_snapshot(
            &root,
            a.id,
            &a,
            &missing_secret.seed,
            &missing_secret,
            VERSION
        )
        .is_err());
        assert_eq!(
            vault
                .proxy_authentication_for_silo(a.id)
                .unwrap()
                .unwrap()
                .password(),
            "changed-secret"
        );
        assert_eq!(
            vault
                .proxy_authentication_for_silo(b.id)
                .unwrap()
                .unwrap()
                .password(),
            "alpha-secret"
        );
        vault
            .restore_managed_silo_backup(
                &root,
                a.id,
                &archive,
                PASSPHRASE,
                &inspection.archive_sha256,
                true,
                VERSION,
            )
            .unwrap();
        assert_eq!(
            vault
                .proxy_authentication_for_silo(a.id)
                .unwrap()
                .unwrap()
                .password(),
            "alpha-secret"
        );
        assert_eq!(
            vault
                .proxy_authentication_for_silo(b.id)
                .unwrap()
                .unwrap()
                .password(),
            "alpha-secret"
        );
        assert_eq!(
            vault
                .list_silos()
                .unwrap()
                .into_iter()
                .find(|silo| silo.id == b.id)
                .unwrap()
                .network_profile
                .credential_reference(),
            Some(a_ref)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stale_host_lock_is_allowed_but_held_lock_is_rejected() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let silo = create_managed(&mut vault, &root, "A");
        let lock_path = root
            .join("silos")
            .join(silo.id.to_string())
            .join("profiles")
            .join(format!("silo-{}.lock", silo.id.simple()));
        #[cfg(target_os = "windows")]
        let held = WindowsProfileFileLock::acquire(&lock_path).unwrap();
        #[cfg(target_os = "linux")]
        let held = crate::linux::FileLease::acquire(&lock_path).unwrap();
        assert!(matches!(
            vault.backup_managed_silo(
                &root,
                silo.id,
                &root.join("blocked.vsm"),
                PASSPHRASE,
                VERSION
            ),
            Err(VaultError::SiloProfileInUse)
        ));
        drop(held);
        vault
            .backup_managed_silo(
                &root,
                silo.id,
                &root.join("allowed.vsm"),
                PASSPHRASE,
                VERSION,
            )
            .unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unlock_rolls_back_prepared_restore_after_interruption() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let silo = create_managed(&mut vault, &root, "A");
        let managed = root.join("silos").join(silo.id.to_string());
        let journal = journal_path(&root, silo.id);
        fs::create_dir(&journal).unwrap();
        fs::copy(vault_path(&root), journal.join("old-vault.json")).unwrap();
        write_marker(&journal.join("prepared")).unwrap();
        fs::rename(&managed, journal.join("old-managed")).unwrap();
        fs::create_dir(&managed).unwrap();
        fs::write(managed.join("new-data"), b"new").unwrap();
        fs::write(vault_path(&root), b"interrupted new vault").unwrap();
        vault.lock();
        vault.unlock(&root, VAULT_PASSWORD).unwrap();
        assert!(!managed.join("new-data").exists());
        assert!(host_profile_root(&managed, silo.id)
            .join("storage/default/example.com/cookies.sqlite")
            .exists());
        assert!(!journal.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completed_rollback_cleanup_does_not_require_deleted_old_vault() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let silo = create_managed(&mut vault, &root, "A");
        let journal = journal_path(&root, silo.id);
        fs::create_dir(&journal).unwrap();
        fs::write(journal.join("leftover"), b"old transaction").unwrap();
        vault.lock();
        vault.unlock(&root, VAULT_PASSWORD).unwrap();
        assert!(!journal.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_never_deletes_a_competing_profile_created_before_stage_swap() {
        let root = root();
        let mut vault = VaultRuntime::default();
        vault.initialize(&root, VAULT_PASSWORD).unwrap();
        let silo = create_managed(&mut vault, &root, "A");
        let managed = root.join("silos").join(silo.id.to_string());
        fs::remove_dir_all(&managed).unwrap();
        let journal = journal_path(&root, silo.id);
        fs::create_dir(&journal).unwrap();
        fs::copy(vault_path(&root), journal.join("old-vault.json")).unwrap();
        fs::create_dir(journal.join("staged-managed")).unwrap();
        write_marker(&journal.join("old-missing")).unwrap();
        write_marker(&journal.join("prepared")).unwrap();
        fs::create_dir(&managed).unwrap();
        fs::write(managed.join("other-process"), b"keep").unwrap();
        assert!(recover_one(&root, silo.id).is_err());
        assert_eq!(fs::read(managed.join("other-process")).unwrap(), b"keep");
        assert!(journal.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
