use crate::engine::EngineError;
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
    process::{Command, Stdio},
};

struct VerificationDirectory(PathBuf);
impl Drop for VerificationDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn verify_detached_cms_sha256(
    payload: &[u8],
    signature: &[u8],
) -> Result<Vec<u8>, EngineError> {
    validate_cms_structure(signature)?;
    let directory = VerificationDirectory(
        std::env::temp_dir().join(format!("verisilo-cms-{}", uuid::Uuid::new_v4())),
    );
    fs::DirBuilder::new().mode(0o700).create(&directory.0)?;
    let signature_path = directory.0.join("signature.der");
    let content_path = directory.0.join("content.bin");
    let signer_path = directory.0.join("signer.pem");
    let verified_path = directory.0.join("verified.bin");
    for (path, bytes) in [(&signature_path, signature), (&content_path, payload)] {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
    }
    // Certificate-chain validation is intentionally replaced by the caller's
    // exact SHA-256 certificate pin; content and signature verification remain on.
    let status = openssl_command()?
        .args([
            "cms",
            "-verify",
            "-binary",
            "-inform",
            "DER",
            "-noverify",
            "-in",
        ])
        .arg(&signature_path)
        .arg("-content")
        .arg(&content_path)
        .arg("-signer")
        .arg(&signer_path)
        .arg("-out")
        .arg(&verified_path)
        .stdout(Stdio::null())
        .status()?;
    if !status.success() {
        return Err(rejected("OpenSSL rejected the detached CMS signature"));
    }
    if fs::read(&verified_path)? != payload {
        return Err(rejected(
            "CMS content differs from the exact manifest signing payload",
        ));
    }
    let signer_pem = fs::read_to_string(&signer_path)?;
    if signer_pem.matches("-----BEGIN CERTIFICATE-----").count() != 1 {
        return Err(rejected("CMS must contain exactly one signer"));
    }
    let certificate_info = openssl_command()?
        .args([
            "x509",
            "-noout",
            "-startdate",
            "-enddate",
            "-ext",
            "extendedKeyUsage",
            "-in",
        ])
        .arg(&signer_path)
        .output()?;
    if !certificate_info.status.success() {
        return Err(rejected("CMS signer certificate metadata is unavailable"));
    }
    let certificate_info = String::from_utf8(certificate_info.stdout)
        .map_err(|_| rejected("CMS signer metadata is not UTF-8"))?;
    let date = |prefix| -> Result<chrono::DateTime<chrono::Utc>, EngineError> {
        let value = certificate_info
            .lines()
            .find_map(|line| line.strip_prefix(prefix))
            .ok_or_else(|| rejected("CMS signer certificate validity is missing"))?;
        chrono::NaiveDateTime::parse_from_str(value, "%b %e %H:%M:%S %Y GMT")
            .map(|date| date.and_utc())
            .map_err(|_| rejected("CMS signer certificate validity is invalid"))
    };
    let now = chrono::Utc::now();
    if now < date("notBefore=")? || now > date("notAfter=")? {
        return Err(rejected("CMS signer certificate is not currently valid"));
    }
    if !certificate_info
        .lines()
        .skip_while(|line| !line.contains("Extended Key Usage:"))
        .skip(1)
        .flat_map(|line| line.split(','))
        .any(|usage| usage.trim() == "Code Signing")
    {
        return Err(rejected(
            "CMS signer certificate must explicitly declare the code-signing EKU",
        ));
    }
    let mut command = openssl_command()?;
    let output = command
        .args(["x509", "-outform", "DER", "-in"])
        .arg(&signer_path)
        .output()?;
    if !output.status.success() || output.stdout.is_empty() || output.stdout.len() > 48 * 1024 {
        return Err(rejected("CMS signer certificate conversion failed"));
    }
    Ok(output.stdout)
}

fn openssl_command() -> Result<Command, EngineError> {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("openssl")));
    let path = bundled
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("/usr/bin/openssl"));
    if !path.is_file() {
        return Err(rejected(
            "OpenSSL is required to verify the native Linux engine package",
        ));
    }
    let mut command = Command::new(path);
    command
        .env_remove("OPENSSL_CONF")
        .env_remove("OPENSSL_MODULES")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_LIBRARY_PATH")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    Ok(command)
}

fn rejected(message: &str) -> EngineError {
    EngineError::VerificationUnavailable(message.to_owned())
}

// Require detached SignedData, one SHA-256 digest and one SHA-256 signer before
// OpenSSL verifies the cryptography. This avoids accepting a different CMS
// algorithm under the manifest's cms-detached-sha256 declaration.
fn validate_cms_structure(signature: &[u8]) -> Result<(), EngineError> {
    const SIGNED_DATA: &[u8] = &[42, 134, 72, 134, 247, 13, 1, 7, 2];
    const DATA: &[u8] = &[42, 134, 72, 134, 247, 13, 1, 7, 1];
    const SHA256: &[u8] = &[96, 134, 72, 1, 101, 3, 4, 2, 1];
    let mut signature = signature;
    let mut outer = der(&mut signature, 0x30)?;
    if !signature.is_empty() || der(&mut outer, 6)? != SIGNED_DATA {
        return Err(rejected("CMS is not SignedData"));
    }
    let mut explicit = der(&mut outer, 0xa0)?;
    let mut signed = der(&mut explicit, 0x30)?;
    if !outer.is_empty() || !explicit.is_empty() {
        return Err(rejected("CMS has trailing data"));
    }
    der(&mut signed, 2)?;
    let mut digests = der(&mut signed, 0x31)?;
    let mut algorithm = der(&mut digests, 0x30)?;
    if der(&mut algorithm, 6)? != SHA256 || !digests.is_empty() {
        return Err(rejected("CMS requires exactly the SHA-256 digest"));
    }
    let mut content = der(&mut signed, 0x30)?;
    if der(&mut content, 6)? != DATA || !content.is_empty() {
        return Err(rejected("CMS content must be detached"));
    }
    for tag in [0xa0, 0xa1] {
        if signed.first() == Some(&tag) {
            der(&mut signed, tag)?;
        }
    }
    let mut signers = der(&mut signed, 0x31)?;
    let mut signer = der(&mut signers, 0x30)?;
    if !signed.is_empty() || !signers.is_empty() {
        return Err(rejected("CMS requires one signer"));
    }
    der(&mut signer, 2)?;
    let sid_tag = *signer
        .first()
        .ok_or_else(|| rejected("CMS signer identity is missing"))?;
    if !matches!(sid_tag, 0x30 | 0x80) {
        return Err(rejected("CMS signer identity is invalid"));
    }
    der(&mut signer, sid_tag)?;
    let mut signer_algorithm = der(&mut signer, 0x30)?;
    if der(&mut signer_algorithm, 6)? != SHA256 {
        return Err(rejected("CMS signer digest must be SHA-256"));
    }
    Ok(())
}

fn der<'a>(input: &mut &'a [u8], tag: u8) -> Result<&'a [u8], EngineError> {
    if input.len() < 2 || input[0] != tag {
        return Err(rejected("CMS DER structure is invalid"));
    }
    let first = input[1];
    let (length, header) = if first < 128 {
        (first as usize, 2)
    } else {
        let count = (first & 0x7f) as usize;
        if count == 0 || count > 4 || input.len() < 2 + count || input[2] == 0 {
            return Err(rejected("CMS DER length is invalid"));
        }
        let length = input[2..2 + count]
            .iter()
            .fold(0_usize, |length, byte| (length << 8) | *byte as usize);
        if length < 128 {
            return Err(rejected("CMS DER length is not canonical"));
        }
        (length, 2 + count)
    };
    let end = header
        .checked_add(length)
        .filter(|end| *end <= input.len())
        .ok_or_else(|| rejected("CMS DER is truncated"))?;
    let value = &input[header..end];
    *input = &input[end..];
    Ok(value)
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_openssl_accepts_code_signer_and_rejects_tampered_payload_and_sha1() {
        let directory = super::VerificationDirectory(
            std::env::temp_dir().join(format!("verisilo-cms-test-{}", uuid::Uuid::new_v4())),
        );
        std::fs::create_dir(&directory.0).unwrap();
        let key = directory.0.join("key.pem");
        let cert = directory.0.join("cert.pem");
        let content = directory.0.join("content");
        let signed = directory.0.join("signed.der");
        let certificate = super::openssl_command()
            .unwrap()
            .args([
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=VeriSilo Linux test",
                "-addext",
                "extendedKeyUsage=codeSigning",
                "-keyout",
            ])
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(certificate.success());
        let payload = b"exact native manifest signing payload";
        std::fs::write(&content, payload).unwrap();
        for algorithm in ["sha256", "sha1"] {
            let status = super::openssl_command()
                .unwrap()
                .args(["cms", "-sign", "-binary", "-md", algorithm, "-in"])
                .arg(&content)
                .arg("-signer")
                .arg(&cert)
                .arg("-inkey")
                .arg(&key)
                .args(["-outform", "DER", "-out"])
                .arg(&signed)
                .status()
                .unwrap();
            assert!(status.success());
            let signature = std::fs::read(&signed).unwrap();
            if algorithm == "sha256" {
                let expected = super::openssl_command()
                    .unwrap()
                    .args(["x509", "-outform", "DER", "-in"])
                    .arg(&cert)
                    .output()
                    .unwrap();
                assert!(expected.status.success());
                assert_eq!(
                    super::verify_detached_cms_sha256(payload, &signature).unwrap(),
                    expected.stdout
                );
                assert!(
                    super::verify_detached_cms_sha256(b"tampered manifest", &signature).is_err()
                );
            } else {
                assert!(super::verify_detached_cms_sha256(payload, &signature).is_err());
            }
        }
    }

    #[test]
    fn cms_parser_rejects_truncation_indefinite_lengths_and_wrong_tags() {
        for input in [&[][..], &[0x30, 0x80], &[0x30, 0x81, 0], &[0x31, 0]] {
            assert!(super::validate_cms_structure(input).is_err());
        }
    }
}
