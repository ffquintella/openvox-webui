//! Test-only stand-in for the backend of the mfa-fido2 plugin, never shipped.
//! It runs in the host WASM runtime and implements the `factor` handlers
//! (PLUGIN_PLATFORM.md 17.15, 17.19), the `route` handlers (17.16) behind the
//! declarative panels (17.18) and the `auth.factor.removed` event (H-10).
//! WebAuthn checks and the ES256 signature verification are real; the limits
//! of the stand-in are listed in docs/architecture/e2e-mfa-fido2-transcript.md.

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CRED_KIND: &str = "mfa-fido2/webauthn-credential";
const REG_KIND: &str = "mfa-fido2/registration-state";
const AUTH_KIND: &str = "mfa-fido2/authentication-state";
const QUALIFIED_ID: &str = "mfa-fido2:security-key";
const ES256: i64 = -7;
const MAX_RESPONSE: usize = 128 * 1024;
const MAX_CLIENT_DATA: usize = 8 * 1024;
const MAX_ATTESTATION: usize = 64 * 1024;
const MAX_AUTH_DATA: usize = 4 * 1024;

const UP: u8 = 0x01;
const UV: u8 = 0x04;
const BE: u8 = 0x08;
const BS: u8 = 0x10;
const AT: u8 = 0x40;
const ED: u8 = 0x80;

// ---- ABI -------------------------------------------------------------------

#[link(wasm_import_module = "openvox")]
extern "C" {
    fn host_call(op_ptr: i32, op_len: i32, req_ptr: i32, req_len: i32) -> i64;
}

/// The host writes requests and host answers into memory taken from here.
/// Every call runs in a fresh instance, so nothing is ever freed.
#[no_mangle]
pub extern "C" fn alloc(len: i32) -> i32 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1) as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as usize as i32
}

fn pack(bytes: Vec<u8>) -> i64 {
    let len = bytes.len() as u32 as i64;
    let ptr = bytes.as_ptr() as usize as u32 as i64;
    std::mem::forget(bytes);
    (ptr << 32) | len
}

#[no_mangle]
pub extern "C" fn handle(ptr: i32, len: i32) -> i64 {
    // SAFETY: the host wrote `len` bytes at `ptr`, a block from `alloc`.
    let req = unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) };
    let out = match serde_json::from_slice::<Value>(req) {
        Ok(r) => json!({ "ok": dispatch(&r) }),
        Err(_) => json!({ "error": "the request is not JSON" }),
    };
    pack(serde_json::to_vec(&out).unwrap_or_else(|_| b"{\"error\":\"encode\"}".to_vec()))
}

/// One host operation; `Err` carries the host error code.
fn host(op: &str, req: Value) -> Result<Value, String> {
    let body = serde_json::to_vec(&req).map_err(|_| "encode".to_string())?;
    // SAFETY: both buffers live until the call returns.
    let packed = unsafe {
        host_call(
            op.as_ptr() as usize as i32,
            op.len() as i32,
            body.as_ptr() as usize as i32,
            body.len() as i32,
        )
    };
    let (p, l) = (((packed as u64) >> 32) as u32, packed as u32);
    // SAFETY: the host wrote `l` bytes at `p`, a block from `alloc`.
    let bytes = unsafe { std::slice::from_raw_parts(p as usize as *const u8, l as usize) };
    let mut env: Value = serde_json::from_slice(bytes).map_err(|_| "host_answer".to_string())?;
    if let Some(ok) = env.get_mut("ok") {
        return Ok(ok.take());
    }
    Err(env["error"]["code"]
        .as_str()
        .unwrap_or("host_error")
        .to_string())
}

fn dispatch(r: &Value) -> Value {
    let input = &r["input"];
    match (
        r["kind"].as_str().unwrap_or(""),
        r["handler"].as_str().unwrap_or(""),
    ) {
        ("factor", h) => {
            let res = match h {
                "begin_enroll" => begin_enroll(input),
                "complete_enroll" => complete_enroll(input),
                "begin_challenge" => begin_challenge(input),
                "verify" => verify(input),
                _ => Err(Fail::internal("unknown_handler")),
            };
            res.unwrap_or_else(|f| json!({ "error": { "kind": f.kind, "code": f.code } }))
        }
        ("route", _) => route(input),
        ("event", _) => on_event(input),
        // Lifecycle hooks and the purge job: nothing to do in the stand-in.
        _ => json!({}),
    }
}

// ---- errors ------------------------------------------------------------------

/// A factor error of the contract: `{"error": {"kind", "code"}}`.
struct Fail {
    kind: &'static str,
    code: &'static str,
}

impl Fail {
    fn invalid(code: &'static str) -> Self {
        Fail {
            kind: "invalid",
            code,
        }
    }
    fn unavailable(code: &'static str) -> Self {
        Fail {
            kind: "unavailable",
            code,
        }
    }
    fn internal(code: &'static str) -> Self {
        Fail {
            kind: "internal",
            code,
        }
    }
}

type R<T> = Result<T, Fail>;

// ---- small helpers -----------------------------------------------------------------

fn sha256(parts: &[&[u8]]) -> Vec<u8> {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().to_vec()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn b64d(v: &Value) -> Option<Vec<u8>> {
    B64.decode(v.as_str()?).ok()
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

fn credential_ref(credential_id: &[u8]) -> String {
    B64.encode(&sha256(&[b"mfa-fido2/credential-ref:", credential_id])[..16])
}

fn random32() -> R<Vec<u8>> {
    let v = host("crypto.random", json!({ "len": 32 }))
        .map_err(|_| Fail::unavailable("random_unavailable"))?;
    match b64d(&v["bytes"]) {
        Some(b) if b.len() == 32 => Ok(b),
        _ => Err(Fail::unavailable("random_unavailable")),
    }
}

fn audit(action: &str, details: Value) {
    let _ = host(
        "audit.write",
        json!({ "action": action, "details": details }),
    );
}

fn publish(event: &str, payload: Value) {
    let _ = host(
        "events.publish",
        json!({ "type": event, "payload": payload }),
    );
}

fn db_query(sql: &str, params: Value) -> R<Vec<Value>> {
    let v = host("db.query", json!({ "sql": sql, "params": params }))
        .map_err(|_| Fail::unavailable("storage_unavailable"))?;
    Ok(v["rows"].as_array().cloned().unwrap_or_default())
}

fn db_exec(sql: &str, params: Value) -> R<u64> {
    let v = host("db.exec", json!({ "sql": sql, "params": params }))
        .map_err(|_| Fail::unavailable("storage_unavailable"))?;
    Ok(v["rows_affected"].as_u64().unwrap_or(0))
}

/// Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, UTC).
fn rfc3339(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

// ---- configuration ---------------------------------------------------------------

struct Config {
    rp_id: String,
    rp_name: String,
    origins: Vec<String>,
    user_verification: String,
    attachment: String,
    allow_backup_eligible: bool,
    timeout_ms: u64,
    max_credentials: usize,
    recommended_min: usize,
    clone_block: bool,
}

fn origin_belongs(origin: &str, rp_id: &str) -> bool {
    let host = if let Some(h) = origin.strip_prefix("https://") {
        h
    } else if let Some(h) = origin.strip_prefix("http://") {
        if rp_id != "localhost" {
            return false;
        }
        h
    } else {
        return false;
    };
    let host = host.split(':').next().unwrap_or("");
    !origin.ends_with('/') && (host == rp_id || host.ends_with(&format!(".{}", rp_id)))
}

/// Reads the plugin settings. Only rule CV-1 (every origin belongs to
/// `rp.id`) is checked; an invalid configuration fails closed.
fn config() -> R<Config> {
    let v = host("config.get", json!({})).map_err(|_| Fail::unavailable("config_invalid"))?;
    let v = &v["values"];
    let rp = &v["rp"];
    let rp_id = s(rp, "id").to_string();
    let valid_id = !rp_id.is_empty()
        && rp_id.len() <= 253
        && rp_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.');
    let origins: Vec<String> = rp["origins"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|o| o.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if !valid_id
        || origins.is_empty()
        || origins.len() > 10
        || !origins.iter().all(|o| origin_belongs(o, &rp_id))
    {
        audit("config.invalid", json!({ "codes": ["CV-1"] }));
        return Err(Fail::unavailable("config_invalid"));
    }
    let a = &v["authenticators"];
    Ok(Config {
        rp_name: rp["name"].as_str().unwrap_or("OpenVox WebUI").to_string(),
        rp_id,
        origins,
        user_verification: a["user_verification"]
            .as_str()
            .unwrap_or("discouraged")
            .to_string(),
        attachment: a["attachment"]
            .as_str()
            .unwrap_or("cross-platform")
            .to_string(),
        allow_backup_eligible: a["allow_backup_eligible"].as_bool().unwrap_or(false),
        timeout_ms: a["timeout_ms"]
            .as_u64()
            .unwrap_or(120_000)
            .clamp(30_000, 240_000),
        max_credentials: a["max_credentials_per_user"].as_u64().unwrap_or(10) as usize,
        recommended_min: a["recommended_min_credentials"].as_u64().unwrap_or(2) as usize,
        clone_block: v["clone_detection"].as_str() != Some("flag"),
    })
}

// ---- CBOR (definite lengths only, depth <= 16, no duplicate map keys) -------

#[derive(Clone, PartialEq)]
enum Cbor {
    Uint(u64),
    Neg(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Cbor>),
    Map(Vec<(Cbor, Cbor)>),
    Bool(bool),
    Null,
}

impl Cbor {
    fn int(&self) -> Option<i64> {
        match self {
            Cbor::Uint(n) => i64::try_from(*n).ok(),
            Cbor::Neg(n) => Some(*n),
            _ => None,
        }
    }
    fn get(&self, key: &Cbor) -> Option<&Cbor> {
        match self {
            Cbor::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    fn at(&self, key: i64) -> Option<&Cbor> {
        let k = if key >= 0 {
            Cbor::Uint(key as u64)
        } else {
            Cbor::Neg(key)
        };
        self.get(&k)
    }
    fn field(&self, key: &str) -> Option<&Cbor> {
        self.get(&Cbor::Text(key.to_string()))
    }
    fn bytes(&self) -> Option<&[u8]> {
        match self {
            Cbor::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn new(b: &'a [u8]) -> Self {
        Reader { b, i: 0 }
    }
    fn take(&mut self, n: u64) -> Option<&'a [u8]> {
        let n = usize::try_from(n).ok()?;
        let end = self.i.checked_add(n)?;
        let out = self.b.get(self.i..end)?;
        self.i = end;
        Some(out)
    }
    fn arg(&mut self, info: u8) -> Option<u64> {
        let n = match info {
            0..=23 => return Some(info as u64),
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            // Indefinite lengths and reserved values are refused.
            _ => return None,
        };
        Some(
            self.take(n)?
                .iter()
                .fold(0u64, |acc, x| (acc << 8) | *x as u64),
        )
    }
    fn item(&mut self, depth: u8) -> Option<Cbor> {
        if depth > 16 {
            return None;
        }
        let ib = *self.b.get(self.i)?;
        self.i += 1;
        let (major, info) = (ib >> 5, ib & 0x1f);
        Some(match major {
            0 => Cbor::Uint(self.arg(info)?),
            1 => Cbor::Neg(-1 - i64::try_from(self.arg(info)?).ok()?),
            2 => {
                let n = self.arg(info)?;
                Cbor::Bytes(self.take(n)?.to_vec())
            }
            3 => {
                let n = self.arg(info)?;
                Cbor::Text(String::from_utf8(self.take(n)?.to_vec()).ok()?)
            }
            4 => {
                let n = self.arg(info)?;
                if n > 1024 {
                    return None;
                }
                let mut v = Vec::new();
                for _ in 0..n {
                    v.push(self.item(depth + 1)?);
                }
                Cbor::Array(v)
            }
            5 => {
                let n = self.arg(info)?;
                if n > 256 {
                    return None;
                }
                let mut m: Vec<(Cbor, Cbor)> = Vec::new();
                for _ in 0..n {
                    let k = self.item(depth + 1)?;
                    if m.iter().any(|(x, _)| *x == k) {
                        return None;
                    }
                    let v = self.item(depth + 1)?;
                    m.push((k, v));
                }
                Cbor::Map(m)
            }
            7 => match info {
                20 => Cbor::Bool(false),
                21 => Cbor::Bool(true),
                22 => Cbor::Null,
                _ => return None,
            },
            // Tags (major type 6) are not used by WebAuthn.
            _ => return None,
        })
    }
}

/// One CBOR item that spans the whole buffer.
fn cbor_exact(b: &[u8]) -> Option<Cbor> {
    let mut r = Reader::new(b);
    let v = r.item(0)?;
    (r.i == b.len()).then_some(v)
}

// ---- WebAuthn parsing --------------------------------------------------------------

struct Attested {
    aaguid: [u8; 16],
    credential_id: Vec<u8>,
    cose: Cbor,
    cose_bytes: Vec<u8>,
}

struct AuthData {
    rp_id_hash: Vec<u8>,
    flags: u8,
    sign_count: u32,
    attested: Option<Attested>,
}

fn parse_auth_data(b: &[u8]) -> Result<AuthData, &'static str> {
    if b.len() < 37 {
        return Err("response_malformed");
    }
    let flags = b[32];
    let sign_count = u32::from_be_bytes([b[33], b[34], b[35], b[36]]);
    let mut r = Reader::new(&b[37..]);
    let attested = if flags & AT != 0 {
        let aaguid: [u8; 16] = r
            .take(16)
            .and_then(|a| a.try_into().ok())
            .ok_or("response_malformed")?;
        let len = r
            .take(2)
            .map(|x| u16::from_be_bytes([x[0], x[1]]))
            .ok_or("response_malformed")?;
        if !(16..=1023).contains(&len) {
            return Err("response_malformed");
        }
        let credential_id = r.take(len as u64).ok_or("response_malformed")?.to_vec();
        let start = r.i;
        let cose = r.item(0).ok_or("response_malformed")?;
        let cose_bytes = b[37 + start..37 + r.i].to_vec();
        if cose_bytes.len() > 2048 {
            return Err("response_malformed");
        }
        Some(Attested {
            aaguid,
            credential_id,
            cose,
            cose_bytes,
        })
    } else {
        None
    };
    if flags & ED != 0 && !matches!(r.item(0), Some(Cbor::Map(_))) {
        return Err("response_malformed");
    }
    if r.i != b.len() - 37 {
        return Err("response_malformed");
    }
    Ok(AuthData {
        rp_id_hash: b[..32].to_vec(),
        flags,
        sign_count,
        attested,
    })
}

/// An ES256 COSE key (kty EC2, alg -7, crv P-256).
fn cose_es256(key: &Cbor) -> Result<VerifyingKey, &'static str> {
    if key.at(3).and_then(Cbor::int) != Some(ES256) {
        return Err("algorithm_not_allowed");
    }
    if key.at(1).and_then(Cbor::int) != Some(2) || key.at(-1).and_then(Cbor::int) != Some(1) {
        return Err("public_key_invalid");
    }
    let (Some(x), Some(y)) = (
        key.at(-2).and_then(Cbor::bytes),
        key.at(-3).and_then(Cbor::bytes),
    ) else {
        return Err("public_key_invalid");
    };
    if x.len() != 32 || y.len() != 32 {
        return Err("public_key_invalid");
    }
    let mut sec1 = vec![4u8];
    sec1.extend_from_slice(x);
    sec1.extend_from_slice(y);
    VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| "public_key_invalid")
}

/// ECDSA P-256 with SHA-256 over `msg`; the signature is DER (high or low S).
fn verify_es256(key: &VerifyingKey, msg: &[u8], der: &[u8]) -> bool {
    match Signature::from_der(der) {
        Ok(sig) => {
            let sig = sig.normalize_s().unwrap_or(sig);
            key.verify(msg, &sig).is_ok()
        }
        Err(_) => false,
    }
}

/// SPEC 9.4 step 3 and 9.6 step 5, without the ledger.
fn check_client_data(
    cdj: &[u8],
    ty: &str,
    challenge_sha256: &str,
    cfg: &Config,
    now: u64,
    expires_at: u64,
) -> Result<(), &'static str> {
    if cdj.len() > MAX_CLIENT_DATA {
        return Err("response_too_large");
    }
    let v: Value = serde_json::from_slice(cdj).map_err(|_| "response_malformed")?;
    if s(&v, "type") != ty {
        return Err("client_data_type_mismatch");
    }
    let challenge = b64d(&v["challenge"]).ok_or("challenge_mismatch")?;
    if !ct_eq(
        hex(&sha256(&[&challenge])).as_bytes(),
        challenge_sha256.as_bytes(),
    ) {
        return Err("challenge_mismatch");
    }
    if now > expires_at {
        return Err("ceremony_expired");
    }
    if !cfg.origins.iter().any(|o| o == s(&v, "origin")) {
        return Err("origin_not_allowed");
    }
    if v["crossOrigin"] == json!(true) || !v["topOrigin"].is_null() {
        return Err("cross_origin_not_allowed");
    }
    if s(&v["tokenBinding"], "status") == "present" {
        return Err("response_malformed");
    }
    Ok(())
}

/// Single use of a challenge (SPEC 9.2): false when it was used before.
fn ledger(challenge_sha256: &str, kind: &str, user_id: &str, now: u64, exp: u64) -> R<bool> {
    let n = db_exec(
        "INSERT OR IGNORE INTO used_challenges \
         (challenge_sha256, kind, user_id, used_at, expires_at) VALUES (?, ?, ?, ?, ?)",
        json!([challenge_sha256, kind, user_id, now, exp]),
    )?;
    Ok(n == 1)
}

fn aaguid_string(a: &[u8; 16]) -> String {
    let h = hex(a);
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

fn nickname(v: &Value) -> Option<String> {
    let n = v.as_str()?.trim();
    let count = n.chars().count();
    ((1..=64).contains(&count) && !n.chars().any(char::is_control)).then(|| n.to_string())
}

// ---- factor handlers -------------------------------------------------------------

/// SPEC section 6: records this plugin did not write are ignored.
fn own_records(list: &Value) -> Vec<&Value> {
    list.as_array()
        .map(|a| {
            a.iter()
                .filter(|r| s(&r["data"], "kind") == CRED_KIND)
                .collect()
        })
        .unwrap_or_default()
}

fn descriptors(records: &[&Value]) -> Vec<Value> {
    records
        .iter()
        .map(|r| {
            json!({ "type": "public-key", "id": r["data"]["credential_id"],
                    "transports": r["data"]["transports"] })
        })
        .collect()
}

fn begin_enroll(input: &Value) -> R<Value> {
    let cfg = config()?;
    let now = input["now_unix"].as_u64().unwrap_or(0);
    let existing = own_records(&input["existing"]);
    if existing.len() >= cfg.max_credentials {
        return Err(Fail::invalid("credential_limit_reached"));
    }
    let challenge = random32()?;
    let user_handle = match existing.first().map(|r| s(&r["data"], "user_handle")) {
        Some(h) if !h.is_empty() => h.to_string(),
        _ => B64.encode(random32()?),
    };
    let mut selection = json!({
        "residentKey": "discouraged",
        "requireResidentKey": false,
        "userVerification": cfg.user_verification,
    });
    if cfg.attachment == "cross-platform" {
        selection["authenticatorAttachment"] = json!("cross-platform");
    }
    let username = s(input, "username");
    let client = json!({ "publicKey": {
        "rp": { "id": cfg.rp_id, "name": cfg.rp_name },
        "user": { "id": user_handle, "name": username, "displayName": username },
        "challenge": B64.encode(&challenge),
        "pubKeyCredParams": [{ "type": "public-key", "alg": ES256 }],
        "timeout": cfg.timeout_ms,
        "excludeCredentials": descriptors(&existing),
        "authenticatorSelection": selection,
        "attestation": "none",
        "extensions": { "credProps": true },
    }});
    let pending = json!({
        "v": 1,
        "kind": REG_KIND,
        "user_id": s(input, "user_id"),
        "user_handle": user_handle,
        "challenge_sha256": hex(&sha256(&[&challenge])),
        "rp_id": cfg.rp_id,
        "user_verification": cfg.user_verification,
        "issued_at": now,
        "expires_at": now + cfg.timeout_ms / 1000 + 30,
    });
    Ok(json!({ "client": client, "mode": { "pending": pending } }))
}

fn complete_enroll(input: &Value) -> R<Value> {
    let user_id = s(input, "user_id").to_string();
    let r = enroll_checks(input, &user_id);
    if let Err(f) = &r {
        if f.kind == "invalid" {
            let details = json!({ "user_id": user_id, "reason": f.code });
            audit("credential.registration_rejected", details.clone());
            publish("mfa-fido2.credential.registration_rejected", details);
        }
    }
    r
}

/// The attestation statement: `(type, trust)` of SPEC 10.2.
fn attestation(
    fmt: &str,
    stmt: &Cbor,
    auth_data: &[u8],
    cdj: &[u8],
    key: &VerifyingKey,
) -> R<(&'static str, &'static str)> {
    match fmt {
        "none" => match stmt {
            Cbor::Map(m) if m.is_empty() => Ok(("none", "none")),
            _ => Err(Fail::invalid("attestation_invalid")),
        },
        "packed" => {
            if stmt.field("x5c").is_some() {
                // Certificate chains are out of scope for the stand-in.
                return Err(Fail::invalid("attestation_unsupported"));
            }
            let sig = stmt
                .field("sig")
                .and_then(Cbor::bytes)
                .ok_or(Fail::invalid("attestation_invalid"))?;
            let msg = [auth_data, &sha256(&[cdj])].concat();
            if stmt.field("alg").and_then(Cbor::int) != Some(ES256) || !verify_es256(key, &msg, sig)
            {
                return Err(Fail::invalid("attestation_invalid"));
            }
            Ok(("self", "self"))
        }
        // SPEC 7.2: other formats are not evaluated under policy optional.
        _ => Ok(("none", "none")),
    }
}

fn enroll_checks(input: &Value, user_id: &str) -> R<Value> {
    let cfg = config()?;
    let now = input["now_unix"].as_u64().unwrap_or(0);
    let pending_id = s(&input["pending"], "id").to_string();
    let st = &input["pending"]["data"];
    if st["v"] != json!(1) || s(st, "kind") != REG_KIND || s(st, "user_id") != user_id {
        return Err(Fail::invalid("ceremony_invalid"));
    }
    let resp = &input["response"];
    if serde_json::to_vec(resp)
        .map(|b| b.len())
        .unwrap_or(usize::MAX)
        > MAX_RESPONSE
    {
        return Err(Fail::invalid("response_too_large"));
    }
    let malformed = || Fail::invalid("response_malformed");
    let cred = &resp["credential"];
    let raw_id = b64d(&cred["rawId"]).ok_or_else(malformed)?;
    if s(cred, "type") != "public-key" || s(cred, "id") != s(cred, "rawId") {
        return Err(malformed());
    }
    let cdj = b64d(&cred["response"]["clientDataJSON"]).ok_or_else(malformed)?;
    let ao = b64d(&cred["response"]["attestationObject"]).ok_or_else(malformed)?;
    if ao.len() > MAX_ATTESTATION {
        return Err(Fail::invalid("response_too_large"));
    }
    let challenge = s(st, "challenge_sha256");
    let expires_at = st["expires_at"].as_u64().unwrap_or(0);
    check_client_data(&cdj, "webauthn.create", challenge, &cfg, now, expires_at)
        .map_err(Fail::invalid)?;
    if !ledger(challenge, "create", user_id, now, expires_at)? {
        return Err(Fail::invalid("ceremony_replayed"));
    }
    let att = cbor_exact(&ao).ok_or_else(malformed)?;
    let fmt = match att.field("fmt") {
        Some(Cbor::Text(t)) => t.clone(),
        _ => return Err(malformed()),
    };
    let stmt = att.field("attStmt").cloned().ok_or_else(malformed)?;
    let auth_data = att
        .field("authData")
        .and_then(Cbor::bytes)
        .ok_or_else(malformed)?
        .to_vec();
    let ad = parse_auth_data(&auth_data).map_err(Fail::invalid)?;
    if !ct_eq(&ad.rp_id_hash, &sha256(&[cfg.rp_id.as_bytes()])) {
        return Err(Fail::invalid("rp_id_hash_mismatch"));
    }
    if ad.flags & UP == 0 {
        return Err(Fail::invalid("user_presence_missing"));
    }
    if cfg.user_verification == "required" && ad.flags & UV == 0 {
        return Err(Fail::invalid("user_verification_missing"));
    }
    if ad.flags & BS != 0 && ad.flags & BE == 0 {
        return Err(Fail::invalid("backup_flags_invalid"));
    }
    if ad.flags & BE != 0 && !cfg.allow_backup_eligible {
        return Err(Fail::invalid("backup_eligible_not_allowed"));
    }
    let attested = ad.attested.as_ref().ok_or_else(malformed)?;
    if attested.credential_id != raw_id {
        return Err(malformed());
    }
    let key = cose_es256(&attested.cose).map_err(Fail::invalid)?;
    let attachment = cred["authenticatorAttachment"].as_str();
    if attachment == Some("platform") && cfg.attachment == "cross-platform" {
        return Err(Fail::invalid("platform_authenticator_not_allowed"));
    }
    let (att_type, trust) = attestation(&fmt, &stmt, &auth_data, &cdj, &key)?;
    let cred_id = B64.encode(&raw_id);
    let cref = credential_ref(&raw_id);
    let taken = db_query(
        "SELECT user_id FROM credentials_index WHERE credential_id = ?",
        json!([cred_id]),
    )?;
    if let Some(row) = taken.first() {
        if s(row, "user_id") == user_id {
            return Err(Fail::invalid("credential_already_registered"));
        }
        audit(
            "credential.registration_rejected",
            json!({ "user_id": user_id, "reason": "credential_id_collision" }),
        );
        return Err(Fail::invalid("registration_rejected"));
    }
    let active = db_query(
        "SELECT COUNT(*) AS n FROM credentials_index WHERE user_id = ? AND status != 'removed'",
        json!([user_id]),
    )?;
    if active.first().and_then(|r| r["n"].as_u64()).unwrap_or(0) as usize >= cfg.max_credentials {
        return Err(Fail::invalid("credential_limit_reached"));
    }
    let known = ["usb", "nfc", "ble", "internal", "hybrid", "smart-card"];
    let transports: Vec<String> = cred["response"]["transports"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|t| known.contains(t))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let label = nickname(&resp["label"]).unwrap_or_else(|| "Security key".to_string());
    let aaguid = aaguid_string(&attested.aaguid);
    let (be, bs, uv) = (ad.flags & BE != 0, ad.flags & BS != 0, ad.flags & UV != 0);
    let record = json!({
        "v": 1,
        "kind": CRED_KIND,
        "credential_id": cred_id,
        "public_key_cose": B64.encode(&attested.cose_bytes),
        "alg": ES256,
        "sign_count": ad.sign_count,
        "rp_id": cfg.rp_id,
        "user_handle": s(st, "user_handle"),
        "aaguid": aaguid,
        "transports": transports,
        "attachment": attachment,
        "discoverable": cred["clientExtensionResults"]["credProps"]["rk"],
        "uv_initialized": uv,
        "backup_eligible": be,
        "backup_state": bs,
        "attestation": { "format": fmt, "type": att_type, "trust": trust },
        "nickname": label,
        "created_at": now,
        "last_used_at": null,
    });
    db_exec(
        "INSERT INTO credentials_index (credential_ref, credential_id, user_id, username, \
         host_record_id, rp_id, aaguid, alg, attestation_format, attestation_trust, \
         transports, backup_eligible, backup_state, sign_count_seen, nickname, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        json!([
            cref,
            cred_id,
            user_id,
            s(input, "username"),
            pending_id,
            cfg.rp_id,
            aaguid,
            ES256,
            fmt,
            trust,
            serde_json::to_string(&transports).unwrap_or_default(),
            be,
            bs,
            ad.sign_count,
            label,
            now
        ]),
    )?;
    audit(
        "credential.registered",
        json!({ "user_id": user_id, "credential_ref": cref, "aaguid": aaguid, "format": fmt,
                "trust": trust, "alg": ES256, "backup_eligible": be, "uv": uv }),
    );
    publish(
        "mfa-fido2.credential.registered",
        json!({ "user_id": user_id, "credential_ref": cref, "aaguid": aaguid, "trust": trust }),
    );
    Ok(json!({ "data": record, "label": label }))
}

fn usable_records<'a>(input: &'a Value, cfg: &Config) -> R<Vec<&'a Value>> {
    let blocked: Vec<String> = db_query(
        "SELECT credential_id FROM credentials_index \
         WHERE user_id = ? AND status IN ('suspended', 'clone_suspected')",
        json!([s(input, "user_id")]),
    )?
    .iter()
    .map(|r| s(r, "credential_id").to_string())
    .collect();
    Ok(own_records(&input["records"])
        .into_iter()
        .filter(|r| s(&r["data"], "rp_id") == cfg.rp_id)
        .filter(|r| !blocked.iter().any(|b| b == s(&r["data"], "credential_id")))
        .collect())
}

fn begin_challenge(input: &Value) -> R<Value> {
    let cfg = config()?;
    let now = input["now_unix"].as_u64().unwrap_or(0);
    let usable = usable_records(input, &cfg)?;
    if usable.is_empty() {
        return Err(Fail::unavailable("no_usable_credential"));
    }
    let challenge = random32()?;
    let mut options = json!({
        "challenge": B64.encode(&challenge),
        "timeout": cfg.timeout_ms,
        "rpId": cfg.rp_id,
        "allowCredentials": descriptors(&usable),
        "userVerification": cfg.user_verification,
    });
    if cfg.attachment == "cross-platform" {
        options["hints"] = json!(["security-key"]);
    }
    let flow = json!({
        "v": 1,
        "kind": AUTH_KIND,
        "user_id": s(input, "user_id"),
        "challenge_sha256": hex(&sha256(&[&challenge])),
        "rp_id": cfg.rp_id,
        "user_verification": cfg.user_verification,
        "allowed": usable.iter().map(|r| r["data"]["credential_id"].clone()).collect::<Vec<_>>(),
        "purpose": s(input, "purpose"),
        "issued_at": now,
        "expires_at": now + cfg.timeout_ms / 1000 + 30,
    });
    Ok(json!({ "client": { "publicKey": options }, "flow_state": flow.to_string() }))
}

/// `{outcome: failed}` after writing the reason to the audit log (the
/// client only ever sees the host's generic failure).
fn failed(input: &Value, cref: Option<&str>, reason: &str, retryable: bool) -> R<Value> {
    audit(
        "credential.verification_failed",
        json!({ "user_id": s(input, "user_id"), "credential_ref": cref, "reason": reason,
                "purpose": s(input, "purpose"), "retryable": retryable }),
    );
    Ok(json!({ "outcome": "failed", "retryable": retryable }))
}

fn verify(input: &Value) -> R<Value> {
    let cfg = config()?;
    let now = input["now_unix"].as_u64().unwrap_or(0);
    let user_id = s(input, "user_id");
    let flow: Value = input["flow_state"]
        .as_str()
        .and_then(|f| serde_json::from_str(f).ok())
        .unwrap_or(Value::Null);
    if flow["v"] != json!(1) || s(&flow, "kind") != AUTH_KIND || s(&flow, "user_id") != user_id {
        return failed(input, None, "ceremony_invalid", false);
    }
    let resp = &input["response"];
    if serde_json::to_vec(resp)
        .map(|b| b.len())
        .unwrap_or(usize::MAX)
        > MAX_RESPONSE
    {
        return failed(input, None, "response_too_large", true);
    }
    let cred = &resp["credential"];
    let Some(raw_id) = b64d(&cred["rawId"]) else {
        return failed(input, None, "response_malformed", true);
    };
    let cred_id = B64.encode(&raw_id);
    let cref = credential_ref(&raw_id);
    let allowed = flow["allowed"]
        .as_array()
        .is_some_and(|a| a.iter().any(|x| x.as_str() == Some(cred_id.as_str())));
    let record = own_records(&input["records"])
        .into_iter()
        .find(|r| s(&r["data"], "credential_id") == cred_id);
    let Some(record) = record.filter(|_| allowed) else {
        return failed(input, None, "credential_not_allowed", true);
    };
    let data = &record["data"];
    let status = db_query(
        "SELECT status FROM credentials_index WHERE credential_id = ?",
        json!([cred_id]),
    )?;
    if status
        .first()
        .is_some_and(|r| matches!(s(r, "status"), "suspended" | "clone_suspected"))
    {
        return failed(input, Some(&cref), "credential_suspended", false);
    }
    let r = &cred["response"];
    let (Some(cdj), Some(ad_bytes), Some(sig)) = (
        b64d(&r["clientDataJSON"]),
        b64d(&r["authenticatorData"]),
        b64d(&r["signature"]),
    ) else {
        return failed(input, Some(&cref), "response_malformed", true);
    };
    if ad_bytes.len() > MAX_AUTH_DATA {
        return failed(input, Some(&cref), "response_too_large", true);
    }
    if let Some(h) = r["userHandle"].as_str().filter(|h| !h.is_empty()) {
        if !ct_eq(h.as_bytes(), s(data, "user_handle").as_bytes()) {
            return failed(input, Some(&cref), "user_handle_mismatch", true);
        }
    }
    let challenge = s(&flow, "challenge_sha256");
    let expires_at = flow["expires_at"].as_u64().unwrap_or(0);
    if let Err(code) = check_client_data(&cdj, "webauthn.get", challenge, &cfg, now, expires_at) {
        return failed(input, Some(&cref), code, true);
    }
    if !ledger(challenge, "get", user_id, now, expires_at)? {
        return failed(input, Some(&cref), "ceremony_replayed", false);
    }
    let ad = match parse_auth_data(&ad_bytes) {
        Ok(ad) if ad.attested.is_none() => ad,
        _ => return failed(input, Some(&cref), "response_malformed", true),
    };
    if !ct_eq(&ad.rp_id_hash, &sha256(&[cfg.rp_id.as_bytes()])) {
        return failed(input, Some(&cref), "rp_id_hash_mismatch", true);
    }
    if ad.flags & UP == 0 {
        return failed(input, Some(&cref), "user_presence_missing", true);
    }
    if s(&flow, "user_verification") == "required" && ad.flags & UV == 0 {
        return failed(input, Some(&cref), "user_verification_missing", true);
    }
    let be = ad.flags & BE != 0;
    if (ad.flags & BS != 0 && !be) || data["backup_eligible"].as_bool() != Some(be) {
        return failed(input, Some(&cref), "backup_flags_invalid", false);
    }
    let key = b64d(&data["public_key_cose"])
        .and_then(|b| cbor_exact(&b))
        .and_then(|c| cose_es256(&c).ok())
        .ok_or(Fail::internal("record_invalid"))?;
    let msg = [ad_bytes.as_slice(), &sha256(&[&cdj])].concat();
    if !verify_es256(&key, &msg, &sig) {
        return failed(input, Some(&cref), "signature_invalid", true);
    }
    let stored = data["sign_count"].as_u64().unwrap_or(0);
    let received = ad.sign_count as u64;
    if !(stored == 0 && received == 0) && received <= stored {
        // SPEC 9.7: a counter regression suspends the credential.
        db_exec(
            "UPDATE credentials_index SET status = 'clone_suspected', \
             status_reason = 'sign_count_regression' WHERE credential_id = ?",
            json!([cred_id]),
        )?;
        let mode = if cfg.clone_block { "block" } else { "flag" };
        audit(
            "credential.clone_suspected",
            json!({ "user_id": user_id, "credential_ref": cref, "stored": stored,
                    "received": received, "mode": mode }),
        );
        publish(
            "mfa-fido2.credential.clone_suspected",
            json!({ "user_id": user_id, "credential_ref": cref }),
        );
        if cfg.clone_block {
            return failed(input, Some(&cref), "sign_count_regression", false);
        }
    }
    let uv = ad.flags & UV != 0;
    let mut updated = data.clone();
    updated["sign_count"] = json!(stored.max(received));
    updated["backup_state"] = json!(ad.flags & BS != 0);
    updated["uv_initialized"] = json!(data["uv_initialized"] == json!(true) || uv);
    updated["last_used_at"] = json!(now);
    db_exec(
        "UPDATE credentials_index SET last_used_at = ?, last_used_purpose = ?, \
         sign_count_seen = ?, backup_state = ? WHERE credential_id = ?",
        json!([
            now,
            s(input, "purpose"),
            stored.max(received),
            ad.flags & BS != 0,
            cred_id
        ]),
    )?;
    let mut amr = vec![if be { "swk" } else { "hwk" }];
    if uv {
        amr.push("pin");
    }
    Ok(json!({
        "outcome": "verified",
        "amr": amr,
        "updates": [[record["id"], updated]],
    }))
}

// ---- events ------------------------------------------------------------------------

/// H-10: `auth.factor.removed` `{user_id, factor, record_id}` marks the index
/// row of that host record as removed (SPEC 10.5 step 2, immediately).
fn on_event(input: &Value) -> Value {
    let p = &input["payload"];
    if s(input, "type") == "auth.factor.removed" && s(p, "factor") == QUALIFIED_ID {
        let _ = db_exec(
            "UPDATE credentials_index SET status = 'removed' \
             WHERE host_record_id = ? AND user_id = ?",
            json!([s(p, "record_id"), s(p, "user_id")]),
        );
    }
    json!({})
}

// ---- API routes (SPEC 12.2, trimmed to what the panels read) ----------------------

fn route_error(status: u16, code: &str, message: &str) -> Value {
    json!({ "status": status, "body": { "error": { "code": code, "message": message } } })
}

fn credential_json(row: &Value) -> Value {
    let status = s(row, "status");
    let at = |k: &str| row[k].as_i64().map(rfc3339);
    json!({
        "credential_ref": row["credential_ref"],
        "host_record_id": row["host_record_id"],
        "nickname": row["nickname"],
        "authenticator": { "aaguid": row["aaguid"], "vendor": null, "model": null, "fips": false },
        "attestation_trust": row["attestation_trust"],
        "transports": serde_json::from_str::<Value>(s(row, "transports")).unwrap_or(json!([])),
        "discoverable": null,
        "backup_eligible": row["backup_eligible"] == json!(1),
        "backup_state": row["backup_state"] == json!(1),
        "status": status,
        "usable": status == "active",
        "blocked_reason": row["status_reason"],
        "created_at": at("created_at"),
        "last_used_at": at("last_used_at"),
    })
}

const COLUMNS: &str = "credential_ref, host_record_id, user_id, username, nickname, aaguid, \
    attestation_trust, transports, backup_eligible, backup_state, status, status_reason, \
    created_at, last_used_at";

fn route(input: &Value) -> Value {
    // A refused storage call answers 409: a plugin may not answer 503.
    route_inner(input).unwrap_or_else(|_| {
        route_error(
            409,
            "unavailable",
            "The security key store is not available.",
        )
    })
}

fn one(cref: &str) -> R<Value> {
    let rows = db_query(
        &format!("SELECT {COLUMNS} FROM credentials_index WHERE credential_ref = ?"),
        json!([cref]),
    )?;
    Ok(json!({ "body": rows.first().map(credential_json) }))
}

fn route_inner(input: &Value) -> R<Value> {
    let user_id = s(&input["user"], "id");
    let cref = s(&input["params"], "credential_ref");
    let body = &input["body"];
    Ok(match (s(input, "method"), s(input, "route")) {
        ("GET", "/me/credentials") => {
            let cfg = config()?;
            let rows = db_query(
                &format!(
                    "SELECT {COLUMNS} FROM credentials_index \
                     WHERE user_id = ? AND status != 'removed' ORDER BY created_at"
                ),
                json!([user_id]),
            )?;
            let active = rows.iter().filter(|r| s(r, "status") == "active").count();
            json!({ "body": {
                "rp_id": cfg.rp_id,
                "policy": { "max_credentials": cfg.max_credentials,
                            "recommended_min_credentials": cfg.recommended_min,
                            "user_verification": cfg.user_verification,
                            "attachment": cfg.attachment },
                "below_recommended_minimum": active < cfg.recommended_min,
                "credentials": rows.iter().map(credential_json).collect::<Vec<_>>(),
            }})
        }
        ("PATCH", "/me/credentials/{credential_ref}") => {
            let Some(name) = nickname(&body["nickname"]) else {
                return Ok(route_error(
                    422,
                    "nickname_invalid",
                    "A nickname is 1 to 64 characters without control characters.",
                ));
            };
            let n = db_exec(
                "UPDATE credentials_index SET nickname = ? \
                 WHERE credential_ref = ? AND user_id = ? AND status != 'removed'",
                json!([name, cref, user_id]),
            )?;
            if n == 0 {
                return Ok(route_error(404, "not_found", "No such security key."));
            }
            audit(
                "credential.renamed",
                json!({ "user_id": user_id, "credential_ref": cref }),
            );
            one(cref)?
        }
        ("GET", "/admin/credentials") => {
            let rows = db_query(
                &format!(
                    "SELECT {COLUMNS} FROM credentials_index WHERE status != 'removed' \
                     ORDER BY created_at LIMIT 200"
                ),
                json!([]),
            )?;
            let items: Vec<Value> = rows
                .iter()
                .map(|r| {
                    let mut c = credential_json(r);
                    c["user_id"] = r["user_id"].clone();
                    c["username"] = r["username"].clone();
                    c
                })
                .collect();
            json!({ "body": { "items": items, "next_cursor": null } })
        }
        ("GET", "/admin/summary") => {
            let valid = config().is_ok();
            let rows = db_query(
                "SELECT status, COUNT(*) AS n FROM credentials_index \
                 WHERE status != 'removed' GROUP BY status",
                json!([]),
            )?;
            let mut by_status = serde_json::Map::new();
            let mut total = 0;
            for r in &rows {
                total += r["n"].as_u64().unwrap_or(0);
                by_status.insert(s(r, "status").to_string(), r["n"].clone());
            }
            json!({ "body": {
                "config": { "valid": valid, "errors": [], "warnings": [] },
                "totals": { "credentials": total },
                "by_status": by_status,
            }})
        }
        ("GET", "/admin/authenticators") => json!({ "body": { "items": [] } }),
        ("POST", "/admin/credentials/{credential_ref}/suspend") => {
            let Some(reason) = body["reason"]
                .as_str()
                .map(str::trim)
                .filter(|r| (1..=500).contains(&r.chars().count()))
            else {
                return Ok(route_error(400, "invalid_request", "A reason is required."));
            };
            set_status(
                input,
                cref,
                "active",
                "suspended",
                Some(reason),
                "already_suspended",
            )?
        }
        ("POST", "/admin/credentials/{credential_ref}/unsuspend") => {
            set_status(input, cref, "suspended", "active", None, "not_suspended")?
        }
        ("POST", "/admin/policy/preview") => json!({ "body": {
            "valid": true, "errors": [], "warnings": [],
            "impact": { "credentials_blocked": 0, "by_reason": {},
                        "users_without_usable_credential": 0 },
        }}),
        _ => route_error(404, "not_found", "No such route."),
    })
}

fn set_status(
    input: &Value,
    cref: &str,
    from: &str,
    to: &str,
    reason: Option<&str>,
    conflict: &str,
) -> R<Value> {
    let rows = db_query(
        "SELECT status, user_id FROM credentials_index WHERE credential_ref = ?",
        json!([cref]),
    )?;
    let Some(row) = rows.first() else {
        return Ok(route_error(404, "not_found", "No such security key."));
    };
    if s(row, "status") != from {
        return Ok(route_error(409, conflict, "The key is not in that state."));
    }
    db_exec(
        "UPDATE credentials_index SET status = ?, status_reason = ? WHERE credential_ref = ?",
        json!([to, reason, cref]),
    )?;
    let action = if to == "suspended" {
        "credential.suspended"
    } else {
        "credential.unsuspended"
    };
    let details = json!({ "user_id": row["user_id"], "credential_ref": cref,
                          "actor_user_id": input["user"]["id"], "reason": reason });
    audit(action, details.clone());
    publish(&format!("mfa-fido2.{}", action), details);
    one(cref)
}
