//! End-to-end check that the host runs the mfa-fido2 plugin through its HTTP
//! API, with the stand-in guest of tests/fixtures/plugins/mfa-fido2-standin
//! (real WebAuthn and ES256 checks in WASM) and a software authenticator that
//! signs with P-256. No hardware authenticator is involved. `--nocapture`
//! prints the transcript; `OVX_E2E_TRANSCRIPT=<file>` also writes it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};

use axum::{body::Body, http::Request};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use openvox_webui::services::plugins::host::{HostApiImpl, PackageModuleSource};
use openvox_webui::services::plugins::package::build_signed_package;
use openvox_webui::services::plugins::storage::{PluginStorage, StorageLimits};
use openvox_webui::services::plugins::wasm::{WasmConfig, WasmInvoker};
use openvox_webui::services::AuthService;
use p256::ecdsa::{signature::Signer, Signature, SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::common::*;

const ADMIN_ID: &str = "00000000-0000-0000-0000-000000000001";
const ADMIN_PASSWORD: &str = "e2e admin password";
const ALICE_PASSWORD: &str = "e2e alice password";
const ORIGIN: &str = "http://localhost:3000";
const FACTOR: &str = "mfa-fido2:security-key";
const UA: &str = "mfa-fido2-e2e";

// ---- the stand-in package --------------------------------------------------------

fn guest_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/mfa-fido2-standin")
}

/// The guest module: `OVX_MFA_FIDO2_WASM`, else built with cargo (needs the
/// `wasm32-unknown-unknown` target) into its own target directory.
fn guest_wasm() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| {
        if let Ok(p) = std::env::var("OVX_MFA_FIDO2_WASM") {
            return std::fs::read(&p).unwrap_or_else(|e| panic!("OVX_MFA_FIDO2_WASM={p}: {e}"));
        }
        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/mfa-fido2-standin");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let status = Command::new(cargo)
            .args(["build", "--release", "--locked", "--target"])
            .arg("wasm32-unknown-unknown")
            .arg("--manifest-path")
            .arg(guest_dir().join("Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_ENCODED_RUSTFLAGS")
            .status()
            .expect("cargo could not be started");
        assert!(
            status.success(),
            "building the mfa-fido2 stand-in failed; it needs \
             `rustup target add wasm32-unknown-unknown` (or set OVX_MFA_FIDO2_WASM)"
        );
        std::fs::read(target.join("wasm32-unknown-unknown/release/mfa_fido2_standin.wasm")).unwrap()
    })
}

/// The signed `.ovxp`: manifest, module, migrations and the example specs.
pub fn standin_package(seed: [u8; 32]) -> Vec<u8> {
    let dir = guest_dir();
    let read = |p: &str| std::fs::read(dir.join(p)).unwrap();
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/mfa-fido2-ui");
    let spec = |p: &str| std::fs::read(ui.join(p)).unwrap();
    let (yaml, up, down) = (
        read("plugin.yaml"),
        read("migrations/0001_init.up.sql"),
        read("migrations/0001_init.down.sql"),
    );
    let (profile, admin) = (spec("profile-security.json"), spec("admin-auth.json"));
    build_signed_package(
        &[
            ("plugin.yaml", &yaml),
            ("backend/plugin.wasm", guest_wasm()),
            ("migrations/0001_init.up.sql", &up),
            ("migrations/0001_init.down.sql", &down),
            ("ui/profile-security.json", &profile),
            ("ui/admin-auth.json", &admin),
        ],
        "mfa-fido2",
        "0.1.0",
        2,
        seed,
    )
}

// ---- software authenticator (ES256, packed self-attestation) ------------------

fn sha256(b: &[u8]) -> Vec<u8> {
    Sha256::digest(b).to_vec()
}

fn cbor_head(major: u8, n: u64) -> Vec<u8> {
    let m = major << 5;
    match n {
        0..=23 => vec![m | n as u8],
        24..=255 => vec![m | 24, n as u8],
        256..=65535 => [vec![m | 25], (n as u16).to_be_bytes().to_vec()].concat(),
        _ => [vec![m | 26], (n as u32).to_be_bytes().to_vec()].concat(),
    }
}

fn cbor_int(i: i64) -> Vec<u8> {
    if i >= 0 {
        cbor_head(0, i as u64)
    } else {
        cbor_head(1, (-1 - i) as u64)
    }
}

fn cbor_bytes(b: &[u8]) -> Vec<u8> {
    [cbor_head(2, b.len() as u64), b.to_vec()].concat()
}

fn cbor_text(s: &str) -> Vec<u8> {
    [cbor_head(3, s.len() as u64), s.as_bytes().to_vec()].concat()
}

fn cbor_map(entries: Vec<(Vec<u8>, Vec<u8>)>) -> Vec<u8> {
    let mut out = cbor_head(5, entries.len() as u64);
    for (k, v) in entries {
        out.extend(k);
        out.extend(v);
    }
    out
}

/// A software authenticator holding one ES256 credential. Keys are made at
/// run time and never printed.
struct SoftKey {
    key: SigningKey,
    id: Vec<u8>,
    counter: u32,
}

impl SoftKey {
    fn new() -> Self {
        let key = loop {
            if let Ok(k) = SigningKey::from_slice(&rand::random::<[u8; 32]>()) {
                break k;
            }
        };
        SoftKey {
            key,
            id: rand::random::<[u8; 32]>().to_vec(),
            counter: 0,
        }
    }

    fn id(&self) -> String {
        B64.encode(&self.id)
    }

    fn sign(&self, msg: &[u8]) -> Vec<u8> {
        let sig: Signature = self.key.sign(msg);
        sig.to_der().as_bytes().to_vec()
    }

    fn cose(&self) -> Vec<u8> {
        let p = self.key.verifying_key().to_encoded_point(false);
        cbor_map(vec![
            (cbor_int(1), cbor_int(2)),
            (cbor_int(3), cbor_int(-7)),
            (cbor_int(-1), cbor_int(1)),
            (cbor_int(-2), cbor_bytes(p.x().unwrap())),
            (cbor_int(-3), cbor_bytes(p.y().unwrap())),
        ])
    }

    /// `navigator.credentials.create` for the server's creation options.
    fn create(&mut self, options: &Value) -> Value {
        let pk = &options["publicKey"];
        let rp_id = pk["rp"]["id"].as_str().unwrap();
        let cdj = json!({"type": "webauthn.create", "challenge": pk["challenge"],
                         "origin": ORIGIN, "crossOrigin": false})
        .to_string()
        .into_bytes();
        let mut ad = sha256(rp_id.as_bytes());
        ad.push(0x45); // UP, UV, AT
        ad.extend(self.counter.to_be_bytes());
        ad.extend([0u8; 16]);
        ad.extend((self.id.len() as u16).to_be_bytes());
        ad.extend(&self.id);
        ad.extend(self.cose());
        let sig = self.sign(&[ad.as_slice(), &sha256(&cdj)].concat());
        let att = cbor_map(vec![
            (cbor_text("fmt"), cbor_text("packed")),
            (
                cbor_text("attStmt"),
                cbor_map(vec![
                    (cbor_text("alg"), cbor_int(-7)),
                    (cbor_text("sig"), cbor_bytes(&sig)),
                ]),
            ),
            (cbor_text("authData"), cbor_bytes(&ad)),
        ]);
        json!({
            "id": self.id(), "rawId": self.id(), "type": "public-key",
            "authenticatorAttachment": "cross-platform",
            "clientExtensionResults": {"credProps": {"rk": false}},
            "response": {"clientDataJSON": B64.encode(&cdj),
                         "attestationObject": B64.encode(&att), "transports": ["usb"]},
        })
    }

    /// `navigator.credentials.get`; `forge` signs another message, which
    /// gives a well-formed but wrong signature.
    fn get(&mut self, options: &Value, forge: bool) -> Value {
        let pk = &options["publicKey"];
        let listed = pk["allowCredentials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == json!(self.id()));
        assert!(listed, "the credential is not in allowCredentials");
        self.counter += 1;
        let rp_id = pk["rpId"].as_str().unwrap();
        let cdj = json!({"type": "webauthn.get", "challenge": pk["challenge"],
                         "origin": ORIGIN, "crossOrigin": false})
        .to_string()
        .into_bytes();
        let mut ad = sha256(rp_id.as_bytes());
        ad.push(0x05); // UP, UV
        ad.extend(self.counter.to_be_bytes());
        let signed = if forge {
            sha256(b"not the client data")
        } else {
            sha256(&cdj)
        };
        let sig = self.sign(&[ad.as_slice(), &signed].concat());
        json!({
            "id": self.id(), "rawId": self.id(), "type": "public-key",
            "authenticatorAttachment": "cross-platform", "clientExtensionResults": {},
            "response": {"clientDataJSON": B64.encode(&cdj), "authenticatorData": B64.encode(&ad),
                         "signature": B64.encode(&sig), "userHandle": null},
        })
    }
}

// ---- HTTP driver and transcript ---------------------------------------------------

const SECRET_KEYS: &[&str] = &[
    "access_token",
    "refresh_token",
    "password",
    "codes",
    "token",
];

/// Tokens, passwords and recovery codes are redacted; long opaque strings
/// (challenges, credential ids, signatures) are shortened.
fn redact(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let v = if SECRET_KEYS.contains(&k.as_str()) && !v.is_null() {
                        json!("<redacted>")
                    } else {
                        redact(v)
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(redact).collect()),
        Value::String(s) if s.chars().count() > 44 && !s.contains(' ') => {
            json!(format!(
                "{}...({} chars)",
                s.chars().take(12).collect::<String>(),
                s.chars().count()
            ))
        }
        _ => v.clone(),
    }
}

/// Large answers (plugin details with the manifest) are shown as their
/// scalar fields, the lists that matter and the size of everything else.
fn brief(v: &Value) -> Value {
    const KEEP: &[&str] = &[
        "warnings",
        "issues",
        "factors",
        "credentials",
        "details",
        "routes",
    ];
    let Value::Object(m) = v else {
        return v.clone();
    };
    if v.to_string().len() <= 1200 {
        return v.clone();
    }
    Value::Object(
        m.iter()
            .map(|(k, x)| {
                let shown = match x {
                    _ if KEEP.contains(&k.as_str()) => x.clone(),
                    Value::Array(a) => json!(format!("[{} items]", a.len())),
                    Value::Object(o) => json!(format!("{{{} keys}}", o.len())),
                    _ => x.clone(),
                };
                (k.clone(), shown)
            })
            .collect(),
    )
}

struct E2e {
    app: TestApp,
    log: Vec<String>,
}

struct Reply {
    status: u16,
    body: Value,
}

impl E2e {
    fn section(&mut self, title: &str) {
        self.log.push(format!("\n### {title}\n"));
    }

    fn note(&mut self, text: &str) {
        self.log.push(format!("{text}\n"));
    }

    async fn send(
        &mut self,
        token: Option<&str>,
        method: &str,
        uri: &str,
        body: Body,
        shown: String,
        headers: &[(&str, &str)],
    ) -> Reply {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("User-Agent", UA);
        if let Some(t) = token {
            b = b.header("Authorization", format!("Bearer {t}"));
        }
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        let r = self.app.request(b.body(body).unwrap()).await;
        let reply = Reply {
            status: r.status.as_u16(),
            body: serde_json::from_slice(&r.body).unwrap_or(Value::Null),
        };
        let auth = if token.is_some() {
            " (Bearer <redacted>)"
        } else {
            ""
        };
        let mut entry = format!("```text\n> {method} {uri}{auth}\n");
        for (k, v) in headers {
            let v = if k.contains("password") {
                "<redacted>"
            } else {
                v
            };
            entry.push_str(&format!("> {k}: {v}\n"));
        }
        if !shown.is_empty() {
            entry.push_str(&format!("> {shown}\n"));
        }
        entry.push_str(&format!("< {}\n", reply.status));
        if !reply.body.is_null() {
            entry.push_str(&format!("< {}\n", brief(&redact(&reply.body))));
        }
        entry.push_str("```\n");
        self.log.push(entry);
        reply
    }

    async fn call(&mut self, token: Option<&str>, method: &str, uri: &str, body: Value) -> Reply {
        let shown = if body.is_null() {
            String::new()
        } else {
            redact(&body).to_string()
        };
        let payload = if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        };
        let ct = [("Content-Type", "application/json")];
        let headers: &[(&str, &str)] = if body.is_null() { &[] } else { &ct };
        self.send(token, method, uri, payload, shown, headers).await
    }

    async fn audit(&self, action: &str) -> Vec<Value> {
        let rows: Vec<(Option<String>,)> =
            sqlx::query_as("SELECT details FROM audit_log WHERE action = ? ORDER BY rowid")
                .bind(action)
                .fetch_all(&self.app.state.db)
                .await
                .unwrap();
        rows.into_iter()
            .map(|(d,)| {
                d.and_then(|d| serde_json::from_str(&d).ok())
                    .unwrap_or(Value::Null)
            })
            .collect()
    }

    fn finish(&self) {
        let text = self.log.join("");
        println!("{text}");
        if let Ok(path) = std::env::var("OVX_E2E_TRANSCRIPT") {
            std::fs::write(&path, &text).unwrap();
        }
    }
}

fn claims(token: &str) -> Value {
    let payload = token.split('.').nth(1).unwrap();
    serde_json::from_slice(&B64.decode(payload).unwrap()).unwrap()
}

fn token_of(r: &Reply) -> String {
    r.body["access_token"].as_str().unwrap().to_string()
}

async fn host() -> TestApp {
    let app = TestApp::with_plugins(|m| m).await;
    let manager = app.state.plugin_manager.clone();
    let dir = manager.data_dir().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let storage = Arc::new(PluginStorage::new(
        app.state.db.clone(),
        dir.clone(),
        StorageLimits::default(),
    ));
    let host = Arc::new(HostApiImpl::new(
        &manager,
        Default::default(),
        storage.clone(),
        None,
    ));
    let source = Arc::new(PackageModuleSource {
        data_dir: dir,
        max_package_bytes: 8 * 1024 * 1024,
    });
    let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
    manager.attach_wasm(storage, wasm);
    // Event delivery (auth.factor.removed to the plugin) as in the server.
    app.state.plugin_runtime.start();
    let hash = AuthService::hash_password(ADMIN_PASSWORD).unwrap();
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(ADMIN_ID)
        .execute(&app.state.db)
        .await
        .unwrap();
    app
}

/// Login challenge with one key: begin, the authenticator, verify.
async fn challenge(e: &mut E2e, id: &str, key: &mut SoftKey, forge: bool) -> Reply {
    let begin = e
        .call(
            None,
            "POST",
            &format!("/api/v1/auth/challenge/{id}/begin"),
            json!({"factor_id": FACTOR}),
        )
        .await;
    assert_eq!(begin.status, 200, "{}", begin.body);
    let credential = key.get(&begin.body["challenge"], forge);
    e.call(
        None,
        "POST",
        &format!("/api/v1/auth/challenge/{id}/verify"),
        json!({"factor_id": FACTOR, "response": {"credential": credential}}),
    )
    .await
}

async fn enroll(e: &mut E2e, token: &str, key: &mut SoftKey, label: &str) -> Reply {
    let base = format!("/api/v1/users/me/factors/{FACTOR}/enroll");
    let begin = e
        .call(Some(token), "POST", &format!("{base}/begin"), json!({}))
        .await;
    if begin.status != 200 {
        return begin;
    }
    let credential = key.create(&begin.body["enrollment"]);
    e.call(
        Some(token),
        "POST",
        &format!("{base}/complete"),
        json!({"credential": credential, "label": label}),
    )
    .await
}

#[tokio::test]
async fn the_host_runs_the_mfa_fido2_plugin_end_to_end() {
    let mut e = E2e {
        app: host().await,
        log: vec!["## Transcript (software authenticator, HTTP API of the test app)\n".into()],
    };

    let t = std::time::Instant::now();
    WasmInvoker::validate_module(guest_wasm()).unwrap();
    e.note(&format!(
        "Guest module: {} bytes; compiling it took {} ms in this build of the host.",
        guest_wasm().len(),
        t.elapsed().as_millis()
    ));

    // 1. Install, approve, configure and enable the plugin as an administrator.
    e.section("1. Administrator signs in and installs the plugin");
    let r = e
        .call(
            None,
            "POST",
            "/api/v1/auth/login",
            json!({"username": "admin", "password": ADMIN_PASSWORD}),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let admin = token_of(&r);
    let pkg = standin_package(rand::random());
    e.note(&format!(
        "Package: `mfa-fido2` 0.1.0, {} bytes, signed with a key generated for this run.",
        pkg.len()
    ));
    let shown = format!("<.ovxp, {} bytes>", pkg.len());
    let r = e
        .send(
            Some(&admin),
            "POST",
            "/api/v1/plugins/preflight",
            Body::from(pkg.clone()),
            shown.clone(),
            &[],
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["issues"], json!([]), "{}", r.body);
    let key_id = r.body["signature"]["key_id"].as_str().unwrap().to_string();
    let r = e
        .send(
            Some(&admin),
            "POST",
            "/api/v1/plugins",
            Body::from(pkg),
            shown,
            &[
                ("x-trust-key-confirm", &key_id[..8]),
                ("x-trust-key-label", "mfa-fido2 e2e"),
                ("x-reauth-password", ADMIN_PASSWORD),
            ],
        )
        .await;
    assert_eq!(r.status, 201, "{}", r.body);
    let declared: Vec<String> = r.body["capabilities"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let all = json!({ "capabilities": declared });
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/capabilities",
            all.clone(),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let config = json!({"rp": {"id": "localhost", "name": "OpenVox WebUI", "origins": [ORIGIN]}});
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/config",
            config,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/enable",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["state"], "enabled", "{}", r.body);
    let r = e
        .call(None, "GET", "/api/v1/auth/providers", Value::Null)
        .await;
    let listed = r.body["factors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["id"] == FACTOR && f["phishing_resistant"] == true);
    assert!(listed, "{}", r.body);

    // 2. A user with a password.
    e.section("2. Administrator creates alice; alice signs in with her password (no policy yet)");
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/users",
            json!({"username": "alice", "email": "alice@example.com",
                   "password": ALICE_PASSWORD, "role": "viewer"}),
        )
        .await;
    assert!(r.status == 200 || r.status == 201, "{}", r.body);
    let login = json!({"username": "alice", "password": ALICE_PASSWORD});
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let alice = token_of(&r);
    assert_eq!(claims(&alice)["acr"], 1);

    // 3. Enrolment (begin/complete) through the plugin.
    e.section("3. alice enrols a security key (enroll/begin, authenticator, enroll/complete)");
    let mut key1 = SoftKey::new();
    let r = enroll(&mut e, &alice, &mut key1, "Desk key").await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["enrolled"], FACTOR);
    let r = e
        .call(
            Some(&alice),
            "GET",
            "/api/v1/plugins/mfa-fido2/me/credentials",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["credentials"][0]["nickname"], "Desk key");
    assert_eq!(r.body["credentials"][0]["last_used_at"], Value::Null);
    let cref = r.body["credentials"][0]["credential_ref"]
        .as_str()
        .unwrap()
        .to_string();
    let registered = e.audit("plugin.mfa-fido2.credential.registered").await;
    e.note(&format!(
        "Audit `plugin.mfa-fido2.credential.registered`: {}",
        redact(&registered[0])
    ));

    e.section("3b. The profile.security panel: its routes for alice, and a rename");
    let r = e
        .call(
            Some(&alice),
            "GET",
            "/api/v1/plugins/ui/contributions/mfa-fido2/security-keys",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let allowed = r.body["routes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|x| x["allowed"] == true);
    assert!(allowed, "{}", r.body);
    let r = e
        .call(
            Some(&alice),
            "PATCH",
            &format!("/api/v1/plugins/mfa-fido2/me/credentials/{cref}"),
            json!({"nickname": "Desk key (USB-C)"}),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["nickname"], "Desk key (USB-C)");
    let r = e
        .call(
            Some(&alice),
            "GET",
            "/api/v1/plugins/mfa-fido2/admin/summary",
            Value::Null,
        )
        .await;
    assert_eq!(
        r.status, 403,
        "admin routes need plugin.mfa-fido2.admin: {}",
        r.body
    );

    // 4. Step-up on a guarded endpoint: a second key needs step-up (OQ-14).
    e.section("4. Step-up: adding a second key is guarded (401), step-up challenge, retry");
    let mut key2 = SoftKey::new();
    let r = enroll(&mut e, &alice, &mut key2, "Backup key").await;
    assert_eq!(r.status, 401, "{}", r.body);
    assert_eq!(r.body["code"], "step_up_required");
    let r = e
        .call(Some(&alice), "POST", "/api/v1/auth/step-up", Value::Null)
        .await;
    assert_eq!(r.status, 202, "{}", r.body);
    let id = r.body["challenge_id"].as_str().unwrap().to_string();
    let r = challenge(&mut e, &id, &mut key1, false).await;
    assert_eq!(r.status, 200, "{}", r.body);
    let stepped = token_of(&r);
    let c = claims(&stepped);
    e.note(&format!(
        "Stepped-up session claims: amr {} acr {}",
        c["amr"], c["acr"]
    ));
    assert_eq!(c["acr"], 2);
    assert_eq!(c["amr"], json!(["pwd", "hwk", "pin"]));
    let r = e
        .call(Some(&alice), "GET", "/api/v1/users/me/factors", Value::Null)
        .await;
    assert_eq!(
        r.status, 401,
        "the pre-step-up session is revoked: {}",
        r.body
    );
    let r = enroll(&mut e, &stepped, &mut key2, "Backup key").await;
    assert_eq!(r.status, 200, "{}", r.body);

    // 5. Logout.
    e.section("5. alice signs out; the token no longer works");
    let r = e
        .call(Some(&stepped), "POST", "/api/v1/auth/logout", Value::Null)
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e
        .call(
            Some(&stepped),
            "GET",
            "/api/v1/users/me/factors",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 401, "{}", r.body);

    // 6. A policy, then the login challenge.
    e.section("6. Administrator requires a second factor; alice signs in with the challenge");
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/auth/policies",
            json!({"policies": [{"name": "everyone", "conditions": {}, "required_acr": 2}]}),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 202, "{}", r.body);
    let offered: Vec<&str> = r.body["factors"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["id"].as_str())
        .collect();
    assert!(offered.contains(&FACTOR), "{}", r.body);
    let id = r.body["challenge_id"].as_str().unwrap().to_string();
    let r = challenge(&mut e, &id, &mut key2, false).await;
    assert_eq!(r.status, 200, "{}", r.body);
    let alice = token_of(&r);
    assert_eq!(claims(&alice)["acr"], 2);
    let r = e
        .call(
            Some(&alice),
            "GET",
            "/api/v1/plugins/mfa-fido2/me/credentials",
            Value::Null,
        )
        .await;
    assert_eq!(r.body["credentials"].as_array().unwrap().len(), 2);

    // 7. Negative case: a wrong signature fails and is counted.
    e.section("7. Negative: a well-formed assertion with a wrong signature");
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 202, "{}", r.body);
    let id = r.body["challenge_id"].as_str().unwrap().to_string();
    let r = challenge(&mut e, &id, &mut key1, true).await;
    assert_eq!(r.status, 401, "{}", r.body);
    assert_eq!(r.body["code"], "factor_failed");
    assert_eq!(r.body["details"]["attempts_left"], 4);
    let attempts: i64 = sqlx::query_scalar("SELECT attempts FROM auth_challenges WHERE id = ?")
        .bind(&id)
        .fetch_one(&e.app.state.db)
        .await
        .unwrap();
    assert_eq!(attempts, 1);
    let failed = e
        .audit("plugin.mfa-fido2.credential.verification_failed")
        .await;
    let last = failed.last().cloned().unwrap_or_default();
    assert_eq!(last["reason"], "signature_invalid");
    let host_failed = e.audit("auth.challenge_failed").await;
    e.note(&format!(
        "`auth_challenges.attempts` = {attempts}; plugin audit `credential.verification_failed`: {}; \
         host audit `auth.challenge_failed` entries: {}",
        redact(&last),
        host_failed.len()
    ));
    let r = challenge(&mut e, &id, &mut key1, false).await;
    assert_eq!(
        r.status, 200,
        "a retry on the same challenge passes: {}",
        r.body
    );
    let alice = token_of(&r);

    e.section("7b. Suspended keys: the plugin's own error code reaches the API (H-8)");
    let r = e
        .call(
            Some(&admin),
            "GET",
            "/api/v1/plugins/mfa-fido2/admin/credentials",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let refs: Vec<String> = r.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["credential_ref"].as_str().map(str::to_string))
        .collect();
    assert_eq!(refs.len(), 2);
    for cref in &refs {
        let r = e
            .call(
                Some(&admin),
                "POST",
                &format!("/api/v1/plugins/mfa-fido2/admin/credentials/{cref}/suspend"),
                json!({"reason": "Reported lost"}),
            )
            .await;
        assert_eq!(r.status, 200, "{}", r.body);
    }
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 202, "{}", r.body);
    let id = r.body["challenge_id"].as_str().unwrap().to_string();
    let r = e
        .call(
            None,
            "POST",
            &format!("/api/v1/auth/challenge/{id}/begin"),
            json!({"factor_id": FACTOR}),
        )
        .await;
    assert_eq!(r.status, 503, "{}", r.body);
    assert_eq!(
        r.body["details"]["factor_error"],
        json!({"kind": "unavailable", "code": "no_usable_credential"})
    );
    e.note(
        "`unsuspend` is declared with `step_up: true`; the administrator has no second \
         factor, so step-up does not apply to them (a user without a usable factor passes).",
    );
    for cref in &refs {
        let r = e
            .call(
                Some(&admin),
                "POST",
                &format!("/api/v1/plugins/mfa-fido2/admin/credentials/{cref}/unsuspend"),
                json!({}),
            )
            .await;
        assert_eq!(r.status, 200, "{}", r.body);
    }

    // 8. Fail closed when the plugin cannot run or is disabled.
    e.section("8. Fail closed: alice keeps only her security keys (removes her recovery codes)");
    let r = e
        .call(Some(&alice), "GET", "/api/v1/users/me/factors", Value::Null)
        .await;
    let recovery = r.body["factors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["kind"] == "recovery")
        .and_then(|f| f["id"].as_str())
        .unwrap()
        .to_string();
    let r = e
        .call(
            Some(&alice),
            "DELETE",
            &format!("/api/v1/users/me/factors/{recovery}"),
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);

    e.section("8a. Outage: the auth:factor grant is withdrawn while the plugin stays enabled");
    let without: Vec<String> = declared
        .iter()
        .filter(|c| c.as_str() != "auth:factor")
        .cloned()
        .collect();
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/capabilities",
            json!({ "capabilities": without }),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 202, "the factor is still offered: {}", r.body);
    let id = r.body["challenge_id"].as_str().unwrap().to_string();
    let r = e
        .call(
            None,
            "POST",
            &format!("/api/v1/auth/challenge/{id}/begin"),
            json!({"factor_id": FACTOR}),
        )
        .await;
    assert_eq!(r.status, 503, "{}", r.body);
    assert_eq!(r.body["code"], "factor_unavailable");
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/capabilities",
            all,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);

    e.section("8b. Outage: the stored package fails its integrity check (plugin in error)");
    let stored = e
        .app
        .state
        .plugin_manager
        .data_dir()
        .join("packages/mfa-fido2/0.1.0.ovxp");
    let original = std::fs::read(&stored).unwrap();
    let mut tampered = original.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0xff;
    std::fs::write(&stored, &tampered).unwrap();
    e.note("The stored `.ovxp` is altered on disk (one byte), then verified.");
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/verify",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 422, "{}", r.body);
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 403, "{}", r.body);
    assert_eq!(r.body["code"], "factor_unavailable");
    std::fs::write(&stored, &original).unwrap();
    e.note("The original package is put back, verified and the plugin enabled again.");
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/verify",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/enable",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["state"], "enabled", "{}", r.body);

    e.section("8c. The administrator disables the plugin");
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/disable",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["warnings"][0]["code"], "users_without_usable_factor");
    let r = e
        .call(None, "POST", "/api/v1/auth/login", login.clone())
        .await;
    assert_eq!(r.status, 403, "{}", r.body);
    assert_eq!(r.body["code"], "mfa_enrollment_required");

    e.section("8d. Break-glass: the local super administrator still signs in");
    let r = e
        .call(
            None,
            "POST",
            "/api/v1/auth/login",
            json!({"username": "admin", "password": ADMIN_PASSWORD}),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let glass = e.audit("auth.break_glass").await;
    assert_eq!(glass.len(), 1);
    e.note(&format!("Audit `auth.break_glass`: {}", redact(&glass[0])));

    e.section("8e. Re-enabling restores the factor (records were kept)");
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/enable",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e.call(None, "POST", "/api/v1/auth/login", login).await;
    assert_eq!(r.status, 202, "{}", r.body);
    let id = r.body["challenge_id"].as_str().unwrap().to_string();
    let r = challenge(&mut e, &id, &mut key1, false).await;
    assert_eq!(r.status, 200, "{}", r.body);
    let alice = token_of(&r);

    e.section("9. Removing a key reaches the plugin as auth.factor.removed (H-10)");
    let r = e
        .call(Some(&alice), "GET", "/api/v1/users/me/factors", Value::Null)
        .await;
    let backup = r.body["factors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["label"] == "Backup key")
        .and_then(|f| f["id"].as_str())
        .unwrap()
        .to_string();
    let r = e
        .call(
            Some(&alice),
            "DELETE",
            &format!("/api/v1/users/me/factors/{backup}"),
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let started = std::time::Instant::now();
    let mut listed = 2;
    while started.elapsed() < std::time::Duration::from_secs(15) {
        let r = e
            .app
            .request(
                Request::builder()
                    .uri("/api/v1/plugins/mfa-fido2/me/credentials")
                    .header("Authorization", format!("Bearer {alice}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
        let body: Value = r.json();
        listed = body["credentials"].as_array().map_or(0, Vec::len);
        if listed == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert_eq!(listed, 1, "the plugin index still lists the removed key");
    e.note(&format!(
        "The plugin's `on_factor_removed` handler marked the key removed {} ms after the \
         DELETE (event bus dispatch, default tick 1 s).",
        started.elapsed().as_millis()
    ));
    let r = e
        .call(
            Some(&alice),
            "GET",
            "/api/v1/plugins/mfa-fido2/me/credentials",
            Value::Null,
        )
        .await;
    assert_eq!(r.body["credentials"][0]["nickname"], "Desk key (USB-C)");
    e.finish();
}

/// A plugin whose `config_schema` requires settings cannot be enabled without
/// them, invalid settings are refused, and the status endpoint says what is
/// missing.
#[tokio::test]
async fn a_plugin_that_needs_settings_is_not_enabled_without_them() {
    let mut e = E2e {
        app: host().await,
        log: Vec::new(),
    };
    let r = e
        .call(
            None,
            "POST",
            "/api/v1/auth/login",
            json!({"username": "admin", "password": ADMIN_PASSWORD}),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let admin = token_of(&r);
    let pkg = standin_package(rand::random());
    let shown = format!("<.ovxp, {} bytes>", pkg.len());
    let r = e
        .send(
            Some(&admin),
            "POST",
            "/api/v1/plugins/preflight",
            Body::from(pkg.clone()),
            shown.clone(),
            &[],
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let key_id = r.body["signature"]["key_id"].as_str().unwrap().to_string();
    let r = e
        .send(
            Some(&admin),
            "POST",
            "/api/v1/plugins",
            Body::from(pkg),
            shown,
            &[
                ("x-trust-key-confirm", &key_id[..8]),
                ("x-trust-key-label", "mfa-fido2 settings"),
                ("x-reauth-password", ADMIN_PASSWORD),
            ],
        )
        .await;
    assert_eq!(r.status, 201, "{}", r.body);
    let declared: Vec<String> = r.body["capabilities"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| c["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/capabilities",
            json!({ "capabilities": declared }),
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);

    let r = e
        .call(
            Some(&admin),
            "GET",
            "/api/v1/plugins/mfa-fido2/config/status",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["valid"], false, "{}", r.body);
    assert_eq!(r.body["errors"], json!(["'rp' is required"]), "{}", r.body);
    assert!(
        r.body["schema"]["properties"]["rp"].is_object(),
        "{}",
        r.body
    );

    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/enable",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 422, "{}", r.body);
    assert_eq!(r.body["code"], "config_invalid", "{}", r.body);

    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/config",
            json!({"rp": {"id": "Not A Host", "origins": ["ftp://x"]}}),
        )
        .await;
    assert_eq!(r.status, 422, "{}", r.body);
    assert_eq!(r.body["code"], "config_invalid", "{}", r.body);

    let config = json!({"rp": {"id": "localhost", "name": "OpenVox WebUI", "origins": [ORIGIN]}});
    let r = e
        .call(
            Some(&admin),
            "PUT",
            "/api/v1/plugins/mfa-fido2/config",
            config,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    let r = e
        .call(
            Some(&admin),
            "POST",
            "/api/v1/plugins/mfa-fido2/enable",
            Value::Null,
        )
        .await;
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(r.body["state"], "enabled", "{}", r.body);
}

/// Writes the signed stand-in package for the browser run of
/// scripts/e2e-mfa-fido2-browser.mjs: `OVX_E2E_EXPORT=<file>`.
#[tokio::test]
#[ignore = "export helper for scripts/e2e-mfa-fido2-browser.mjs"]
async fn export_the_standin_package() {
    let path = std::env::var("OVX_E2E_EXPORT").expect("set OVX_E2E_EXPORT=<file>");
    std::fs::write(&path, standin_package(rand::random())).unwrap();
}
