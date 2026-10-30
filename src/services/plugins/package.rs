//! `.ovxp` package reading, integrity checking and ed25519 verification (T68, T69).
//!
//! Zip layout: `plugin.yaml`, `MANIFEST.sha256` (`<sha256>  <path>` per file), `plugin.sig`
//! and the payload. The signature covers a header plus the exact bytes of `MANIFEST.sha256`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

use super::manifest::{self, safe_package_path};
use crate::models::{PluginManifest, SignatureEnvelope, SignatureHeader};

pub const MANIFEST_FILE: &str = "plugin.yaml";
pub const HASHES_FILE: &str = "MANIFEST.sha256";
pub const SIGNATURE_FILE: &str = "plugin.sig";

const MAX_ENTRIES: usize = 4096;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const SIGNATURE_DOMAIN: &[u8] = b"openvox-plugin-signature-v1\0";

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("not a valid package: {0}")]
    Malformed(String),
    #[error("the package has no signature (plugin.sig); unsigned plugins are never accepted")]
    Unsigned,
    #[error("package integrity check failed: {0}")]
    Integrity(String),
    #[error("invalid manifest")]
    Manifest(Vec<String>),
}

/// A package whose files were read and whose hashes match `MANIFEST.sha256`.
/// The signature has NOT been checked yet; see [`verify_signature`].
pub struct PluginPackage {
    pub files: BTreeMap<String, Vec<u8>>,
    pub manifest: PluginManifest,
    pub envelope: SignatureEnvelope,
    pub package_sha256: String,
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Fingerprint (key id) of a raw ed25519 public key.
pub fn key_id_of(public_key: &[u8]) -> String {
    sha256_hex(public_key)
}

pub fn read_package(bytes: &[u8], max_bytes: u64) -> Result<PluginPackage, PackageError> {
    if bytes.len() as u64 > max_bytes {
        return Err(PackageError::Malformed("package is too large".into()));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| PackageError::Malformed(e.to_string()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(PackageError::Malformed("too many files".into()));
    }

    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let mut lowered = BTreeSet::new();
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let entry = zip
            .by_index(i)
            .map_err(|e| PackageError::Malformed(e.to_string()))?;
        if entry.is_dir() {
            continue;
        }
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(PackageError::Malformed(
                "symbolic links are not allowed".into(),
            ));
        }
        let name = entry.name().to_string();
        if !safe_package_path(&name) {
            return Err(PackageError::Malformed(format!("unsafe path '{}'", name)));
        }
        if !lowered.insert(name.to_ascii_lowercase()) {
            return Err(PackageError::Malformed(format!(
                "duplicate file '{}'",
                name
            )));
        }
        let mut buf = Vec::new();
        entry
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| PackageError::Malformed(e.to_string()))?;
        if buf.len() as u64 > MAX_FILE_BYTES {
            return Err(PackageError::Malformed(format!("'{}' is too large", name)));
        }
        total += buf.len() as u64;
        if total > MAX_TOTAL_BYTES {
            return Err(PackageError::Malformed("package expands too large".into()));
        }
        files.insert(name, buf);
    }

    let sig_bytes = files.get(SIGNATURE_FILE).ok_or(PackageError::Unsigned)?;
    let hashes = files
        .get(HASHES_FILE)
        .ok_or_else(|| PackageError::Integrity(format!("{} is missing", HASHES_FILE)))?;
    check_hashes(&files, hashes)?;

    let envelope: SignatureEnvelope = serde_json::from_slice(sig_bytes)
        .map_err(|e| PackageError::Malformed(format!("plugin.sig: {}", e)))?;

    let yaml = files
        .get(MANIFEST_FILE)
        .ok_or_else(|| PackageError::Malformed("plugin.yaml is missing".into()))?;
    let names: BTreeSet<String> = files.keys().cloned().collect();
    let manifest = manifest::parse_and_validate(yaml, &names).map_err(PackageError::Manifest)?;
    // The SRI a `module` contribution declares must match its entry file.
    let problems = super::modules::check_integrity(&manifest, &files);
    if !problems.is_empty() {
        return Err(PackageError::Manifest(problems));
    }

    Ok(PluginPackage {
        files,
        manifest,
        envelope,
        package_sha256: sha256_hex(bytes),
    })
}

/// Every payload file must be listed with the right hash, and nothing may be
/// listed that is absent from the archive.
fn check_hashes(files: &BTreeMap<String, Vec<u8>>, hashes: &[u8]) -> Result<(), PackageError> {
    let text = std::str::from_utf8(hashes)
        .map_err(|_| PackageError::Integrity("MANIFEST.sha256 is not UTF-8".into()))?;
    let mut listed: BTreeMap<&str, &str> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (digest, path) = line
            .split_once("  ")
            .ok_or_else(|| PackageError::Integrity(format!("bad line '{}'", line)))?;
        let valid_digest = digest.len() == 64
            && digest
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c));
        if !valid_digest || !safe_package_path(path) {
            return Err(PackageError::Integrity(format!("bad line '{}'", line)));
        }
        if listed.insert(path, digest).is_some() {
            return Err(PackageError::Integrity(format!(
                "'{}' is listed twice",
                path
            )));
        }
    }
    if !listed.contains_key(MANIFEST_FILE) {
        return Err(PackageError::Integrity("plugin.yaml is not covered".into()));
    }
    for (path, data) in files {
        if path == HASHES_FILE || path == SIGNATURE_FILE {
            continue;
        }
        match listed.remove(path.as_str()) {
            Some(d) if d == sha256_hex(data) => {}
            Some(_) => return Err(PackageError::Integrity(format!("'{}' was modified", path))),
            None => {
                return Err(PackageError::Integrity(format!(
                    "'{}' is not covered by {}",
                    path, HASHES_FILE
                )))
            }
        }
    }
    if let Some((path, _)) = listed.into_iter().next() {
        return Err(PackageError::Integrity(format!(
            "'{}' is listed but missing",
            path
        )));
    }
    Ok(())
}

fn signing_message(header: &SignatureHeader, hashes: &[u8]) -> Vec<u8> {
    let mut msg = SIGNATURE_DOMAIN.to_vec();
    msg.extend_from_slice(
        format!(
            "id={}\nversion={}\napi_version={}\nkey_id={}\nsigned_at={}\n",
            header.id, header.version, header.api_version, header.key_id, header.signed_at
        )
        .as_bytes(),
    );
    msg.push(0);
    msg.extend_from_slice(hashes);
    msg
}

/// Checks the signature mathematically and that it is bound to this plugin id,
/// version and API version. Returns the signer's raw public key. Whether the
/// signer is trusted is decided separately by the trust store.
pub fn verify_signature(pkg: &PluginPackage) -> Result<[u8; 32], String> {
    let h = &pkg.envelope.header;
    let m = &pkg.manifest;
    if h.id != m.id || h.version != m.version || h.api_version != m.api_version {
        return Err("the signature was made for a different plugin id or version".into());
    }
    let pk: [u8; 32] = B64
        .decode(&pkg.envelope.public_key)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or("the embedded public key is malformed")?;
    if key_id_of(&pk) != h.key_id {
        return Err("key_id does not match the embedded public key".into());
    }
    let sig_bytes = B64
        .decode(&pkg.envelope.signature)
        .map_err(|_| "the signature is not valid base64")?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| "the signature is malformed")?;
    let key = VerifyingKey::from_bytes(&pk).map_err(|_| "the public key is not valid")?;
    let hashes = pkg
        .files
        .get(HASHES_FILE)
        .ok_or("MANIFEST.sha256 is missing")?;
    key.verify_strict(&signing_message(h, hashes), &sig)
        .map_err(|_| "the signature does not match the package contents")?;
    Ok(pk)
}

/// Parses an administrator-supplied public key: PEM (SPKI), an OpenSSH
/// `ssh-ed25519` line or base64 of the raw 32 bytes.
pub fn parse_public_key(input: &str) -> Result<[u8; 32], String> {
    const SPKI_PREFIX: [u8; 12] = [
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let s = input.trim();
    let raw: Vec<u8> = if s.starts_with("ssh-ed25519") {
        let key = ssh_key::PublicKey::from_openssh(s).map_err(|e| format!("invalid key: {}", e))?;
        key.key_data()
            .ed25519()
            .ok_or("only ed25519 keys are accepted")?
            .0
            .to_vec()
    } else if s.starts_with("-----BEGIN") {
        let body: String = s.lines().filter(|l| !l.starts_with("-----")).collect();
        let der = B64.decode(body.trim()).map_err(|_| "invalid PEM body")?;
        if der.len() != 44 || der[..12] != SPKI_PREFIX {
            return Err("only ed25519 public keys are accepted".into());
        }
        der[12..].to_vec()
    } else {
        B64.decode(s)
            .map_err(|_| "unrecognised public key format")?
    };
    let pk: [u8; 32] = raw.try_into().map_err(|_| "an ed25519 key is 32 bytes")?;
    VerifyingKey::from_bytes(&pk).map_err(|_| "the public key is not valid")?;
    Ok(pk)
}

/// Builds and signs a package. Exists for the test suites; the shipped host
/// never signs anything (developers use their own tooling).
pub fn build_signed_package(
    files: &[(&str, &[u8])],
    id: &str,
    version: &str,
    api_version: u32,
    signing_seed: [u8; 32],
) -> Vec<u8> {
    use ed25519_dalek::Signer;
    use std::io::Write;
    let sk = SigningKey::from_bytes(&signing_seed);
    let pk = sk.verifying_key().to_bytes();
    let mut sorted: Vec<_> = files.to_vec();
    sorted.sort_by_key(|(p, _)| *p);
    let mut hashes = String::new();
    for (p, d) in &sorted {
        hashes.push_str(&format!("{}  {}\n", sha256_hex(d), p));
    }
    let header = SignatureHeader {
        id: id.into(),
        version: version.into(),
        api_version,
        key_id: key_id_of(&pk),
        signed_at: "2026-10-28T00:00:00Z".into(),
    };
    let sig = sk.sign(&signing_message(&header, hashes.as_bytes()));
    let env = SignatureEnvelope {
        header,
        public_key: B64.encode(pk),
        signature: B64.encode(sig.to_bytes()),
    };
    let mut out = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut out);
        let opts = zip::write::SimpleFileOptions::default();
        for (p, d) in &sorted {
            zw.start_file(*p, opts).unwrap();
            zw.write_all(d).unwrap();
        }
        zw.start_file(HASHES_FILE, opts).unwrap();
        zw.write_all(hashes.as_bytes()).unwrap();
        zw.start_file(SIGNATURE_FILE, opts).unwrap();
        zw.write_all(serde_json::to_string(&env).unwrap().as_bytes())
            .unwrap();
        zw.finish().unwrap();
    }
    out.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: P\nruntime: { kind: builtin, entry: demo }\n";
    const SEED: [u8; 32] = [7; 32];

    fn pkg(extra: &[(&str, &[u8])]) -> Vec<u8> {
        let mut f: Vec<(&str, &[u8])> = vec![(MANIFEST_FILE, YAML.as_bytes())];
        f.extend_from_slice(extra);
        build_signed_package(&f, "demo", "1.0.0", 2, SEED)
    }

    #[test]
    fn valid_package_verifies() {
        let p = read_package(&pkg(&[("README.md", b"hi")]), 1 << 20).unwrap();
        let pk = verify_signature(&p).unwrap();
        assert_eq!(key_id_of(&pk), p.envelope.header.key_id);
    }

    #[test]
    fn unsigned_package_is_refused() {
        use std::io::Write;
        let mut out = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default();
            zw.start_file(MANIFEST_FILE, o).unwrap();
            zw.write_all(YAML.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        assert!(matches!(
            read_package(&out.into_inner(), 1 << 20),
            Err(PackageError::Unsigned)
        ));
    }

    #[test]
    fn tampered_file_is_detected() {
        use std::io::Write;
        let orig = pkg(&[("README.md", b"hi")]);
        let mut z = zip::ZipArchive::new(Cursor::new(orig)).unwrap();
        let mut out = Cursor::new(Vec::new());
        {
            let mut zw = zip::ZipWriter::new(&mut out);
            let o = zip::write::SimpleFileOptions::default();
            for i in 0..z.len() {
                let mut e = z.by_index(i).unwrap();
                let mut b = Vec::new();
                e.read_to_end(&mut b).unwrap();
                if e.name() == "README.md" {
                    b = b"evil".to_vec();
                }
                zw.start_file(e.name(), o).unwrap();
                zw.write_all(&b).unwrap();
            }
            zw.finish().unwrap();
        }
        assert!(matches!(
            read_package(&out.into_inner(), 1 << 20),
            Err(PackageError::Integrity(_))
        ));
    }

    #[test]
    fn signature_is_bound_to_id_and_version() {
        let bytes = build_signed_package(
            &[(MANIFEST_FILE, YAML.as_bytes())],
            "demo",
            "9.9.9",
            2,
            SEED,
        );
        let p = read_package(&bytes, 1 << 20).unwrap();
        assert!(verify_signature(&p)
            .unwrap_err()
            .contains("different plugin"));
    }

    #[test]
    fn wrong_key_material_fails() {
        let mut p = read_package(&pkg(&[]), 1 << 20).unwrap();
        let other = SigningKey::from_bytes(&[9; 32]).verifying_key().to_bytes();
        p.envelope.public_key = B64.encode(other);
        assert!(verify_signature(&p).is_err());
        let mut p = read_package(&pkg(&[]), 1 << 20).unwrap();
        p.envelope.signature = B64.encode([0u8; 64]);
        assert!(verify_signature(&p).is_err());
    }

    #[test]
    fn oversized_and_garbage_input_is_refused() {
        assert!(read_package(&[0u8; 10], 1 << 20).is_err());
        assert!(read_package(&pkg(&[]), 10).is_err());
    }

    #[test]
    fn public_keys_parse_from_base64_and_pem() {
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        assert_eq!(parse_public_key(&B64.encode(pk)).unwrap(), pk);
        let mut der = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        der.extend_from_slice(&pk);
        let pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            B64.encode(der)
        );
        assert_eq!(parse_public_key(&pem).unwrap(), pk);
        assert!(parse_public_key("garbage").is_err());
    }
}
