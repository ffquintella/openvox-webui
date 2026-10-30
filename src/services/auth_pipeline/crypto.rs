//! Sealing of factor data and one-way hashes of client bindings.

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use sha2::{Digest, Sha256};

fn derive(key: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"openvox-webui/auth-pipeline/v1");
    h.update(key.as_bytes());
    h.finalize().into()
}

/// Seals `plaintext`: `base64(nonce || ciphertext)`.
pub fn seal(key: &str, plaintext: &[u8]) -> Result<String> {
    let nonce_bytes: [u8; 12] = rand::random();
    let cipher = ChaCha20Poly1305::new(&Key::from(derive(key)));
    let ct = cipher
        .encrypt(&Nonce::from(nonce_bytes), plaintext)
        .map_err(|_| anyhow!("sealing failed"))?;
    let mut out = nonce_bytes.to_vec();
    out.extend(ct);
    Ok(B64.encode(out))
}

/// Opens a value produced by [`seal`].
pub fn open(key: &str, sealed: &str) -> Result<Vec<u8>> {
    let raw = B64
        .decode(sealed)
        .map_err(|_| anyhow!("invalid sealed data"))?;
    if raw.len() < 13 {
        return Err(anyhow!("invalid sealed data"));
    }
    let (n, ct) = raw.split_at(12);
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(n);
    ChaCha20Poly1305::new(&Key::from(derive(key)))
        .decrypt(&Nonce::from(nonce), ct)
        .map_err(|_| anyhow!("opening failed"))
}

pub fn seal_json<T: serde::Serialize>(key: &str, value: &T) -> Result<String> {
    seal(key, &serde_json::to_vec(value)?)
}

pub fn open_json<T: serde::de::DeserializeOwned>(key: &str, sealed: &str) -> Result<T> {
    Ok(serde_json::from_slice(&open(key, sealed)?)?)
}

/// Stable hash used to bind challenges and flows to a client or browser.
pub fn bind_hash(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_be_bytes());
        h.update(p.as_bytes());
    }
    hex::encode(h.finalize())
}

/// Constant-time equality for equal-length secrets.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_roundtrips_and_rejects_wrong_key() {
        let s = seal("k1", b"seed").unwrap();
        assert_eq!(open("k1", &s).unwrap(), b"seed");
        assert!(open("k2", &s).is_err());
        assert_ne!(s, seal("k1", b"seed").unwrap(), "nonce is random");
    }

    #[test]
    fn bind_hash_is_unambiguous() {
        assert_ne!(bind_hash(&["ab", "c"]), bind_hash(&["a", "bc"]));
    }
}
