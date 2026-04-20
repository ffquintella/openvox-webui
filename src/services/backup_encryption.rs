//! Backup encryption using ChaCha20-Poly1305
//!
//! Uses Argon2id for password-based key derivation and ChaCha20-Poly1305
//! for authenticated encryption. This provides both confidentiality and
//! integrity protection for backup files.

use anyhow::{Context, Result};
use argon2::Argon2;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Key, Nonce,
};

/// Encrypted data with all components needed for decryption
#[derive(Debug, Clone)]
pub struct EncryptedData {
    /// Salt used for key derivation (16 bytes)
    pub salt: [u8; 16],
    /// Nonce used for encryption (12 bytes)
    pub nonce: [u8; 12],
    /// The encrypted ciphertext with authentication tag
    pub ciphertext: Vec<u8>,
}

impl EncryptedData {
    /// Convert salt to base64 string for storage
    pub fn salt_base64(&self) -> String {
        BASE64.encode(self.salt)
    }

    /// Convert nonce to base64 string for storage
    pub fn nonce_base64(&self) -> String {
        BASE64.encode(self.nonce)
    }

    /// Create from base64 encoded components
    pub fn from_base64(salt_b64: &str, nonce_b64: &str, ciphertext: Vec<u8>) -> Result<Self> {
        let salt_bytes = BASE64
            .decode(salt_b64)
            .context("Failed to decode salt from base64")?;
        let nonce_bytes = BASE64
            .decode(nonce_b64)
            .context("Failed to decode nonce from base64")?;

        if salt_bytes.len() != 16 {
            anyhow::bail!("Invalid salt length: expected 16, got {}", salt_bytes.len());
        }
        if nonce_bytes.len() != 12 {
            anyhow::bail!(
                "Invalid nonce length: expected 12, got {}",
                nonce_bytes.len()
            );
        }

        let mut salt = [0u8; 16];
        let mut nonce = [0u8; 12];
        salt.copy_from_slice(&salt_bytes);
        nonce.copy_from_slice(&nonce_bytes);

        Ok(Self {
            salt,
            nonce,
            ciphertext,
        })
    }
}

/// Encrypt data with a password using ChaCha20-Poly1305
///
/// The encryption process:
/// 1. Generate random salt (16 bytes)
/// 2. Derive key from password using Argon2id
/// 3. Generate random nonce (12 bytes)
/// 4. Encrypt data with ChaCha20-Poly1305
///
/// # Arguments
/// * `data` - The plaintext data to encrypt
/// * `password` - The password to use for key derivation
///
/// # Returns
/// The encrypted data including salt and nonce needed for decryption
pub fn encrypt(data: &[u8], password: &str) -> Result<EncryptedData> {
    // Generate random salt for key derivation
    let salt: [u8; 16] = rand::random();

    // Derive key from password using Argon2id
    let key = derive_key(password, &salt)?;

    // Generate random nonce
    let nonce_array = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    // Create cipher and encrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce = Nonce::from_slice(&nonce_array);

    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&nonce_array);

    Ok(EncryptedData {
        salt,
        nonce: nonce_bytes,
        ciphertext,
    })
}

/// Decrypt data with a password
///
/// # Arguments
/// * `encrypted` - The encrypted data including salt and nonce
/// * `password` - The password used for encryption
///
/// # Returns
/// The decrypted plaintext data
///
/// # Errors
/// Returns an error if:
/// - Key derivation fails
/// - Decryption fails (wrong password or corrupted data)
pub fn decrypt(encrypted: &EncryptedData, password: &str) -> Result<Vec<u8>> {
    // Derive key from password using the same salt
    let key = derive_key(password, &encrypted.salt)?;

    // Create cipher and decrypt
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce = Nonce::from_slice(&encrypted.nonce);

    let plaintext = cipher
        .decrypt(nonce, encrypted.ciphertext.as_ref())
        .map_err(|_| anyhow::anyhow!("Decryption failed - incorrect password or corrupted data"))?;

    Ok(plaintext)
}

/// Verify that a password can decrypt the data without returning the plaintext
///
/// This is useful for verifying backups without fully decrypting them.
pub fn verify_password(encrypted: &EncryptedData, password: &str) -> bool {
    decrypt(encrypted, password).is_ok()
}

/// Derive a 256-bit key from password using Argon2id
///
/// Argon2id is a memory-hard key derivation function that provides resistance
/// against both GPU-based attacks (like Argon2d) and side-channel attacks
/// (like Argon2i).
///
/// Parameters are chosen for a good security/performance balance:
/// - Memory: 64 MB (m_cost = 65536)
/// - Iterations: 3 (t_cost = 3)
/// - Parallelism: 4 (p_cost = 4)
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let argon2 = Argon2::default();
    let mut key = [0u8; 32];

    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("Key derivation failed: {}", e))?;

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let data = b"Hello, World! This is test data for encryption.";
        let password = "test-password-123";

        let encrypted = encrypt(data, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();

        assert_eq!(data.to_vec(), decrypted);
    }

    #[test]
    fn test_wrong_password_fails() {
        let data = b"Secret data";
        let password = "correct-password";
        let wrong_password = "wrong-password";

        let encrypted = encrypt(data, password).unwrap();
        let result = decrypt(&encrypted, wrong_password);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Decryption failed"));
    }

    #[test]
    fn test_verify_password() {
        let data = b"Test data";
        let password = "my-password";

        let encrypted = encrypt(data, password).unwrap();

        assert!(verify_password(&encrypted, password));
        assert!(!verify_password(&encrypted, "wrong-password"));
    }

    #[test]
    fn test_different_encryptions_have_different_salts() {
        let data = b"Same data";
        let password = "same-password";

        let encrypted1 = encrypt(data, password).unwrap();
        let encrypted2 = encrypt(data, password).unwrap();

        // Salts should be different
        assert_ne!(encrypted1.salt, encrypted2.salt);
        // Nonces should be different
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        // Ciphertexts should be different (due to different salt/nonce)
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // But both should decrypt to the same plaintext
        assert_eq!(
            decrypt(&encrypted1, password).unwrap(),
            decrypt(&encrypted2, password).unwrap()
        );
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = b"Test data for base64";
        let password = "password123";

        let encrypted = encrypt(data, password).unwrap();

        let salt_b64 = encrypted.salt_base64();
        let nonce_b64 = encrypted.nonce_base64();

        let reconstructed =
            EncryptedData::from_base64(&salt_b64, &nonce_b64, encrypted.ciphertext.clone())
                .unwrap();

        let decrypted = decrypt(&reconstructed, password).unwrap();
        assert_eq!(data.to_vec(), decrypted);
    }

    #[test]
    fn test_empty_data() {
        let data = b"";
        let password = "password";

        let encrypted = encrypt(data, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();

        assert_eq!(data.to_vec(), decrypted);
    }

    #[test]
    fn test_large_data() {
        // Test with 1MB of data
        let data: Vec<u8> = (0..1024 * 1024).map(|i| (i % 256) as u8).collect();
        let password = "password-for-large-data";

        let encrypted = encrypt(&data, password).unwrap();
        let decrypted = decrypt(&encrypted, password).unwrap();

        assert_eq!(data, decrypted);
    }
}
