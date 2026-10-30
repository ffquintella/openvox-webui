//! TOTP (RFC 6238, HMAC-SHA1, 6 digits, 30 s) with a ±1 step window.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

use super::crypto::ct_eq;

pub const STEP_SECS: u64 = 30;
pub const DIGITS: u32 = 6;

pub fn generate_secret() -> Vec<u8> {
    let bytes: [u8; 20] = rand::random();
    bytes.to_vec()
}

pub fn secret_to_base32(secret: &[u8]) -> String {
    BASE32_NOPAD.encode(secret)
}

pub fn secret_from_base32(s: &str) -> Option<Vec<u8>> {
    BASE32_NOPAD.decode(s.trim().to_uppercase().as_bytes()).ok()
}

/// HOTP value (RFC 4226) for `counter`.
pub fn hotp(secret: &[u8], counter: u64) -> String {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(secret).expect("hmac accepts any key");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin = ((digest[offset] as u32 & 0x7f) << 24)
        | ((digest[offset + 1] as u32) << 16)
        | ((digest[offset + 2] as u32) << 8)
        | (digest[offset + 3] as u32);
    format!(
        "{:0width$}",
        bin % 10u32.pow(DIGITS),
        width = DIGITS as usize
    )
}

pub fn step_at(unix_secs: u64) -> u64 {
    unix_secs / STEP_SECS
}

/// Returns the accepted step when `code` matches the current step or one step
/// either side and the step is newer than `last_step`, so a code never works
/// twice.
pub fn verify(secret: &[u8], code: &str, unix_secs: u64, last_step: Option<u64>) -> Option<u64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let now = step_at(unix_secs);
    let mut accepted = None;
    // Check every candidate so timing does not reveal which step matched.
    for step in [now.saturating_sub(1), now, now + 1] {
        let ok = ct_eq(hotp(secret, step).as_bytes(), code.as_bytes());
        if ok && last_step.is_none_or(|l| step > l) {
            accepted = Some(step);
        }
    }
    accepted
}

pub fn provisioning_uri(issuer: &str, account: &str, secret: &[u8]) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
        urlencoding::encode(issuer),
        urlencoding::encode(account),
        secret_to_base32(secret),
        urlencoding::encode(issuer),
        DIGITS,
        STEP_SECS
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4226 appendix D (secret "12345678901234567890").
    #[test]
    fn hotp_matches_rfc_4226_vectors() {
        let s = b"12345678901234567890";
        let expect = ["755224", "287082", "359152", "969429", "338314"];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(hotp(s, i as u64), *e);
        }
    }

    // RFC 6238 appendix B, SHA1, T = 59 s -> 94287082 (8 digits); last 6 = 287082.
    #[test]
    fn totp_matches_rfc_6238_at_59s() {
        let s = b"12345678901234567890";
        assert_eq!(verify(s, "287082", 59, None), Some(1));
    }

    #[test]
    fn window_is_one_step_and_steps_are_single_use() {
        let s = b"12345678901234567890";
        let code = hotp(s, 10);
        assert_eq!(verify(s, &code, 10 * 30 + 5, None), Some(10));
        assert_eq!(verify(s, &code, 9 * 30, None), Some(10), "one step early");
        assert_eq!(verify(s, &code, 11 * 30, None), Some(10), "one step late");
        assert_eq!(verify(s, &code, 12 * 30, None), None, "two steps late");
        assert_eq!(verify(s, &code, 10 * 30, Some(10)), None, "replay");
        assert_eq!(verify(s, "12345", 300, None), None);
        assert_eq!(verify(s, "abcdef", 300, None), None);
    }

    #[test]
    fn base32_roundtrip() {
        let s = generate_secret();
        assert_eq!(secret_from_base32(&secret_to_base32(&s)).unwrap(), s);
    }
}
