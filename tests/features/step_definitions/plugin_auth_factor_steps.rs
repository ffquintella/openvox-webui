//! Step definitions for plugin second factors (T116: H-1, H-2, H-9). They
//! run the real plugin manager, WASM runtime (with WAT guests) and
//! authentication pipeline on an in-memory database.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use base64::Engine;
use cucumber::{given, then, when};
use ed25519_dalek::SigningKey;
use openvox_webui::config::AppConfig;
use openvox_webui::models::{ApproveCapabilitiesRequest, PluginState, User};
use openvox_webui::services::auth_pipeline::policy::{self, PolicyInput};
use openvox_webui::services::auth_pipeline::{
    factors::{now_unix, ChallengeContext, ChallengePurpose, FactorRecord},
    AuthPipeline, ClientInfo, LoginOutcome, PipelineError, PrimaryIdentity, StepUpCheck,
    VerifyOutcome,
};
use openvox_webui::services::plugins::host::{HostApiImpl, PackageModuleSource};
use openvox_webui::services::plugins::invoke::{HandlerInvoker, HandlerKind};
use openvox_webui::services::plugins::manifest::parse_and_validate;
use openvox_webui::services::plugins::modules::{sri_of, SriAlgorithm};
use openvox_webui::services::plugins::package::build_signed_package;
use openvox_webui::services::plugins::storage::{PluginStorage, StorageLimits};
use openvox_webui::services::plugins::wasm::{WasmConfig, WasmInvoker};
use openvox_webui::services::plugins::PluginManager;
use openvox_webui::services::AuthService;
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::features::support::TestWorld;

const SEED: [u8; 32] = [61; 32];

/// What one scenario built and observed.
pub struct FactorScenario {
    manager: Arc<PluginManager>,
    pool: SqlitePool,
    config: AppConfig,
    user: User,
    challenge: Option<String>,
    offered: Vec<String>,
    denied: Option<String>,
    answer: Option<Result<(), String>>,
    echoed: Option<Value>,
    records: HashMap<String, Vec<String>>,
    outputs: Vec<Value>,
    manifest: Option<Result<Vec<String>, Vec<String>>>,
    session: Option<String>,
}

impl std::fmt::Debug for FactorScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FactorScenario")
            .field("offered", &self.offered)
            .field("denied", &self.denied)
            .finish_non_exhaustive()
    }
}

impl FactorScenario {
    fn pipeline(&self) -> AuthPipeline<'_> {
        AuthPipeline::new(&self.pool, &self.config).with_plugin_factors(self.manager.auth_factors())
    }

    async fn sessions(&self) -> Vec<(String, i64)> {
        sqlx::query_as("SELECT amr, acr FROM auth_sessions WHERE user_id = ?")
            .bind(self.user.id.to_string())
            .fetch_all(&self.pool)
            .await
            .unwrap()
    }
}

fn scenario(world: &mut TestWorld) -> &mut FactorScenario {
    world
        .plugin_factors
        .as_mut()
        .expect("the background builds the plugin host")
}

fn code_of(e: &PipelineError) -> String {
    match e {
        PipelineError::Unavailable(_) => "factor_unavailable",
        PipelineError::InvalidChallenge => "invalid_challenge",
        PipelineError::Locked => "locked",
        PipelineError::Invalid(_) => "invalid",
        PipelineError::Internal(_) => "internal",
        PipelineError::Forbidden { code, .. } => code,
        PipelineError::Factor { code, .. } => code.as_str(),
    }
    .to_string()
}

// ---- WAT guests ------------------------------------------------------------

#[derive(Clone, Copy)]
enum Out {
    Json(&'static str),
    /// `{"ok":{"client":<the request>,"flow_state":null}}` (begin_challenge).
    Echo,
    /// `{"ok":{"client":<the request>,"mode":{"pending":{}}}}` (begin_enroll).
    EchoEnroll,
    Trap,
    Spin,
}

const ECHO_PREFIX: &str = r#"{"ok":{"client":"#;
const ECHO_SUFFIX: &str = r#","flow_state":null}}"#;
const ECHO_ENROLL_SUFFIX: &str = r#","mode":{"pending":{}}}}"#;
const EMPTY_OK: &str = r#"{"ok":{}}"#;
const VERIFIED: &str = r#"{"ok":{"outcome":"verified","amr":"hwk"}}"#;

fn wat_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A factor guest. Requests start with `{"kind":"factor","handler":"`, so the
/// handler name is at offset 28; other kinds (lifecycle hooks) answer `{}`.
fn factor_guest(challenge: Out, verify: Out) -> Vec<u8> {
    factor_guest_with(
        Out::Json(r#"{"ok":{"client":{},"mode":{"pending":{}}}}"#),
        challenge,
        verify,
    )
}

fn factor_guest_with(enroll: Out, challenge: Out, verify: Out) -> Vec<u8> {
    let outs = [
        enroll,
        Out::Json(r#"{"ok":{"data":{},"label":"Key"}}"#),
        challenge,
        verify,
    ];
    let pre = ECHO_PREFIX.len();
    let mut data = String::new();
    let mut funcs = String::new();
    for (i, out) in outs.iter().enumerate() {
        let off = 1024 + i * 8192;
        let body = match out {
            Out::Json(s) => {
                data += &format!("  (data (i32.const {off}) \"{}\")\n", wat_str(s));
                format!(
                    "(i64.or (i64.shl (i64.const {off}) (i64.const 32)) (i64.const {}))",
                    s.len()
                )
            }
            Out::Trap => "(unreachable)".to_string(),
            Out::Spin => "(loop $spin (br $spin)) (unreachable)".to_string(),
            Out::Echo | Out::EchoEnroll => {
                let (at, suf) = match out {
                    Out::Echo => (41000, ECHO_SUFFIX.len()),
                    _ => (41200, ECHO_ENROLL_SUFFIX.len()),
                };
                format!(
                    "(local.set $out (call $alloc (i32.add (local.get $l) (i32.const {n})))) \
                     (memory.copy (local.get $out) (i32.const 40960) (i32.const {pre})) \
                     (memory.copy (i32.add (local.get $out) (i32.const {pre})) (local.get $p) (local.get $l)) \
                     (memory.copy (i32.add (i32.add (local.get $out) (i32.const {pre})) (local.get $l)) (i32.const {at}) (i32.const {suf})) \
                     (i64.or (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32)) \
                             (i64.extend_i32_u (i32.add (local.get $l) (i32.const {n}))))",
                    n = pre + suf
                )
            }
        };
        funcs += &format!(
            "  (func $emit{i} (param $p i32) (param $l i32) (result i64) (local $out i32) {body})\n"
        );
    }
    let wat = format!(
        r#"(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 4)
  (global $bump (mut i32) (i32.const 65536))
{data}  (data (i32.const 40960) "{prefix}")
  (data (i32.const 41000) "{suffix}")
  (data (i32.const 41100) "{empty}")
  (data (i32.const 41200) "{enroll_suffix}")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $r i32)
    (local.set $r (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $r))
{funcs}  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (if (i32.ne (i32.load8_u (i32.add (local.get $p) (i32.const 13))) (i32.const 111))
      (then (return (i64.or (i64.shl (i64.const 41100) (i64.const 32)) (i64.const {empty_len})))))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 28))) (i32.const 118))
      (then (return (call $emit3 (local.get $p) (local.get $l)))))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 28))) (i32.const 99))
      (then (return (call $emit1 (local.get $p) (local.get $l)))))
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 34))) (i32.const 101))
      (then (return (call $emit0 (local.get $p) (local.get $l)))))
    (call $emit2 (local.get $p) (local.get $l))))"#,
        prefix = wat_str(ECHO_PREFIX),
        suffix = wat_str(ECHO_SUFFIX),
        enroll_suffix = wat_str(ECHO_ENROLL_SUFFIX),
        empty = wat_str(EMPTY_OK),
        empty_len = EMPTY_OK.len(),
    );
    wat::parse_str(wat).unwrap()
}

/// A guest whose every handler makes one host call and answers `{"ok": <envelope>}`.
fn host_call_guest(op: &str, req: &str) -> Vec<u8> {
    let wat = format!(
        r#"(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 2)
  (global $bump (mut i32) (i32.const 8192))
  (data (i32.const 256) "{op}")
  (data (i32.const 1024) "{req}")
  (data (i32.const 512) "{{\"ok\":")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $p))
  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (local $packed i64) (local $rp i32) (local $rl i32) (local $out i32)
    (local.set $packed (call $hc (i32.const 256) (i32.const {oplen}) (i32.const 1024) (i32.const {reqlen})))
    (local.set $rp (i32.wrap_i64 (i64.shr_u (local.get $packed) (i64.const 32))))
    (local.set $rl (i32.wrap_i64 (local.get $packed)))
    (local.set $out (call $alloc (i32.add (local.get $rl) (i32.const 7))))
    (memory.copy (local.get $out) (i32.const 512) (i32.const 6))
    (memory.copy (i32.add (local.get $out) (i32.const 6)) (local.get $rp) (local.get $rl))
    (i32.store8 (i32.add (i32.add (local.get $out) (i32.const 6)) (local.get $rl)) (i32.const 125))
    (i64.or (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32))
            (i64.extend_i32_u (i32.add (local.get $rl) (i32.const 7))))))"#,
        op = wat_str(op),
        req = wat_str(req),
        oplen = op.len(),
        reqlen = req.len(),
    );
    wat::parse_str(wat).unwrap()
}

async fn install(s: &FactorScenario, id: &str, yaml: &str, wasm: &[u8], approve: &[&str]) {
    let pkg = build_signed_package(
        &[("plugin.yaml", yaml.as_bytes()), ("plugin.wasm", wasm)],
        id,
        "1.0.0",
        2,
        SEED,
    );
    s.manager.install(&pkg, "admin", None).await.unwrap();
    s.manager
        .approve_capabilities(
            id,
            &ApproveCapabilitiesRequest {
                version: None,
                capabilities: approve.iter().map(|c| c.to_string()).collect(),
            },
            "admin",
        )
        .await
        .unwrap();
    s.manager.enable(id).await.unwrap();
}

fn factor_yaml(id: &str, limits: &str) -> String {
    format!(
        "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
         runtime: {{ kind: wasm, entry: plugin.wasm, limits: {{ {limits} }} }}\n\
         capabilities:\n  - auth:factor\n\
         contributes:\n  auth_factors:\n    - {{ id: key, kind: webauthn, label: Security key, amr: hwk, enrollment: host-managed, fields: [credential] }}\n"
    )
}

// ---- background ------------------------------------------------------------

#[given(expr = "a plugin host with the user {string} and a policy that requires a second factor")]
async fn given_host(world: &mut TestWorld, username: String) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let dir = std::env::temp_dir().join(format!("ovx_bdd_factor_{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
    let manager = Arc::new(
        PluginManager::new(pool.clone(), dir.clone(), 8).with_builtin_keys(vec![("T".into(), pk)]),
    );
    manager.reload().await.unwrap();
    let storage = Arc::new(PluginStorage::new(
        pool.clone(),
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

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role) VALUES (?, ?, ?, 'x', 'viewer')",
    )
    .bind(id.to_string())
    .bind(&username)
    .bind(format!("{username}@example.com"))
    .execute(&pool)
    .await
    .unwrap();
    let user = AuthService::new(pool.clone())
        .get_user_by_id(&id)
        .await
        .unwrap()
        .unwrap();
    policy::replace(
        &pool,
        user.organization_id,
        &[PolicyInput {
            name: "everyone".into(),
            enabled: true,
            priority: 100,
            conditions: Default::default(),
            required_acr: 2,
            require_phishing_resistant: false,
        }],
    )
    .await
    .unwrap();
    world.plugin_factors = Some(FactorScenario {
        manager,
        pool,
        config: AppConfig::default(),
        user,
        challenge: None,
        offered: Vec::new(),
        denied: None,
        answer: None,
        echoed: None,
        records: HashMap::new(),
        outputs: Vec::new(),
        manifest: None,
        session: None,
    });
}

// ---- manifest --------------------------------------------------------------

const MANIFEST: &str = "api_version: 2\nid: mfa-fido2\nname: FIDO2\nversion: 0.1.0\npublisher: ACME\n\
    runtime: { kind: wasm, entry: plugin.wasm }\ncapabilities: [auth:factor, crypto:random]\n\
    contributes:\n  auth_factors:\n    - { id: security-key, kind: webauthn, label: Security key, amr: hwk, enrollment: host-managed, fields: [credential] }\n";

#[when(regex = r"^I validate a factor plugin manifest with (.+)$")]
async fn when_validate(world: &mut TestWorld, change: String) {
    let caps = "capabilities: [auth:factor, crypto:random]";
    let yaml = match change.trim() {
        "nothing changed" => MANIFEST.to_string(),
        "no auth:factor capability" => MANIFEST.replace(caps, "capabilities: [crypto:random]"),
        "an optional auth:factor" => {
            MANIFEST.replace(caps, "capabilities: [\"auth:factor\": { optional: true }]")
        }
        "the amr \"rcv\"" => MANIFEST.replace("amr: hwk", "amr: rcv"),
        "the amr list [pin]" => MANIFEST.replace("amr: hwk", "amr: [pin]"),
        "the amr list [hwk, hwk]" => MANIFEST.replace("amr: hwk", "amr: [hwk, hwk]"),
        "the factor id \"pa:key\"" => MANIFEST.replace("id: security-key", "id: \"pa:key\""),
        "the remote runtime" => MANIFEST.replace(
            "{ kind: wasm, entry: plugin.wasm }",
            "{ kind: remote, endpoint: \"https://f.example.com\" }",
        ),
        "a trusted module page" => {
            let sri = sri_of(SriAlgorithm::Sha384, b"x");
            MANIFEST
                .replace(caps, "capabilities: [auth:factor, ui:slot:page]")
                .replace(
                    "contributes:\n",
                    &format!(
                        "contributes:\n  slots:\n    - {{ slot: page, id: c, render: module, src: ui/remoteEntry.js, integrity: \"{sri}\", path: /p/mfa-fido2/c }}\n"
                    ),
                )
        }
        other => panic!("unknown manifest change '{other}'"),
    };
    let files: BTreeSet<String> = ["plugin.wasm", "ui/remoteEntry.js"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let result = parse_and_validate(yaml.as_bytes(), &files).map(|m| {
        m.contributes
            .auth_factors
            .into_iter()
            .map(|f| f.id)
            .collect()
    });
    scenario(world).manifest = Some(result);
}

#[then(expr = "the manifest should be refused with {string}")]
async fn then_manifest_refused(world: &mut TestWorld, problem: String) {
    match scenario(world).manifest.as_ref().expect("a manifest") {
        Err(p) => assert!(
            p.iter().any(|m| m.contains(&problem)),
            "expected '{problem}' in {p:?}"
        ),
        Ok(_) => panic!("the manifest was accepted"),
    }
}

#[then(expr = "the manifest should be accepted with the factor {string}")]
async fn then_manifest_accepted(world: &mut TestWorld, id: String) {
    match scenario(world).manifest.as_ref().expect("a manifest") {
        Ok(ids) => assert_eq!(ids, &vec![id]),
        Err(p) => panic!("refused: {p:?}"),
    }
}

// ---- factor plugins --------------------------------------------------------

#[given(regex = r#"^an enabled WASM factor plugin "([a-z0-9-]+)"(.*) whose verify (.+)$"#)]
async fn given_factor_plugin(world: &mut TestWorld, id: String, quota: String, behaviour: String) {
    let mut limits = "timeout_ms: 5000".to_string();
    if let Some(n) = quota
        .trim()
        .strip_prefix("limited to ")
        .and_then(|r| r.strip_suffix(" calls per minute"))
    {
        limits += &format!(", invocations_per_minute: {n}");
    }
    let verify = match behaviour.trim() {
        "succeeds" => Out::Json(VERIFIED),
        "traps" => Out::Trap,
        "runs past its timeout" => {
            limits = "timeout_ms: 300, fuel_per_call: 5000000000".into();
            Out::Spin
        }
        "answers verified with the amr \"otp\"" => {
            Out::Json(r#"{"ok":{"outcome":"verified","amr":"otp"}}"#)
        }
        "answers verified for a record it was not given" => {
            Out::Json(r#"{"ok":{"outcome":"verified","amr":"hwk","updates":[["not-given",{}]]}}"#)
        }
        other => panic!("unknown behaviour '{other}'"),
    };
    let challenge = Out::Json(r#"{"ok":{"client":{"publicKey":{}},"flow_state":"fs"}}"#);
    let s = scenario(world);
    install(
        s,
        &id,
        &factor_yaml(&id, &limits),
        &factor_guest(challenge, verify),
        &["auth:factor"],
    )
    .await;
}

#[given(
    regex = r#"^an enabled WASM factor plugin "([a-z0-9-]+)" that echoes its challenge input$"#
)]
async fn given_echo_plugin(world: &mut TestWorld, id: String) {
    let s = scenario(world);
    install(
        s,
        &id,
        &factor_yaml(&id, "timeout_ms: 5000"),
        &factor_guest(Out::Echo, Out::Json(VERIFIED)),
        &["auth:factor"],
    )
    .await;
}

#[given(
    regex = r#"^an enabled WASM factor plugin "([a-z0-9-]+)" with the amr (.+) that verifies with the amr (.+)$"#
)]
async fn given_amr_plugin(world: &mut TestWorld, id: String, declared: String, answered: String) {
    let yaml = factor_yaml(&id, "timeout_ms: 5000")
        .replace("amr: hwk,", &format!("amr: {},", declared.trim()));
    let verify: &'static str = Box::leak(
        format!(
            r#"{{"ok":{{"outcome":"verified","amr":{}}}}}"#,
            answered.trim()
        )
        .into_boxed_str(),
    );
    let challenge = Out::Json(r#"{"ok":{"client":{"publicKey":{}},"flow_state":"fs"}}"#);
    let s = scenario(world);
    install(
        s,
        &id,
        &yaml,
        &factor_guest(challenge, Out::Json(verify)),
        &["auth:factor"],
    )
    .await;
}

#[given(
    regex = r#"^an enabled WASM factor plugin "([a-z0-9-]+)" whose challenge answers the error "([a-z]+)" "([^"]+)"$"#
)]
async fn given_error_plugin(world: &mut TestWorld, id: String, kind: String, code: String) {
    let challenge: &'static str = Box::leak(
        format!(r#"{{"ok":{{"error":{{"kind":"{kind}","code":"{code}"}}}}}}"#).into_boxed_str(),
    );
    let s = scenario(world);
    install(
        s,
        &id,
        &factor_yaml(&id, "timeout_ms: 5000"),
        &factor_guest(Out::Json(challenge), Out::Json(VERIFIED)),
        &["auth:factor"],
    )
    .await;
}

#[given(
    regex = r#"^an enabled WASM factor plugin "([a-z0-9-]+)" that echoes its enrolment input$"#
)]
async fn given_enroll_echo_plugin(world: &mut TestWorld, id: String) {
    let s = scenario(world);
    install(
        s,
        &id,
        &factor_yaml(&id, "timeout_ms: 5000"),
        &factor_guest_with(Out::EchoEnroll, Out::Echo, Out::Json(VERIFIED)),
        &["auth:factor"],
    )
    .await;
}

#[when(expr = "alice begins enrolling the factor {string}")]
async fn when_begin_enroll(world: &mut TestWorld, factor: String) {
    let s = scenario(world);
    let client = s.pipeline().begin_enroll(&s.user, &factor).await.unwrap();
    s.echoed = Some(client);
}

#[then("the plugin should have received the host time in its enrolment input")]
async fn then_enroll_time(world: &mut TestWorld) {
    let echoed = scenario(world).echoed.clone().expect("an echoed request");
    assert_eq!(echoed["handler"], "begin_enroll");
    let sent = echoed["input"]["now_unix"].as_u64().expect("now_unix");
    assert!(now_unix().abs_diff(sent) <= 60, "{sent}");
}

#[given(regex = r#"^alice has enrolled the factor "([^"]+)"(.*)$"#)]
async fn given_enrolled(world: &mut TestWorld, kind: String, times: String) {
    let n: usize = times
        .trim()
        .strip_suffix(" times")
        .map(|n| n.parse().unwrap())
        .unwrap_or(1);
    let s = scenario(world);
    for i in 0..n {
        let rec = s
            .pipeline()
            .store()
            .insert(s.user.id, &kind, "k", &json!({"n": i}), true)
            .await
            .unwrap();
        s.records.entry(kind.clone()).or_default().push(rec.id);
    }
}

#[when(expr = "the plugin {string} is disabled")]
async fn when_disabled(world: &mut TestWorld, id: String) {
    scenario(world).manager.disable(&id).await.unwrap();
}

#[when(expr = "the factor {string} is verified {int} times")]
async fn when_verified_n(world: &mut TestWorld, factor: String, n: usize) {
    let s = scenario(world);
    let ids = s.records.get(&factor).cloned().unwrap_or_default();
    let f = s
        .manager
        .auth_factors()
        .into_iter()
        .find(|f| f.descriptor().id == factor)
        .expect("the factor is registered");
    let ctx = ChallengeContext {
        user_id: s.user.id,
        username: s.user.username.clone(),
        records: ids
            .into_iter()
            .map(|id| FactorRecord {
                id,
                data: json!({}),
            })
            .collect(),
        flow_state: None,
        now_unix: now_unix(),
        purpose: ChallengePurpose::Login,
    };
    for _ in 0..n {
        assert!(f.verify(&ctx, json!({})).await.is_err(), "a failure");
    }
}

// ---- sign-in ---------------------------------------------------------------

#[when("alice signs in with her password")]
async fn when_sign_in(world: &mut TestWorld) {
    let s = scenario(world);
    let outcome = s
        .pipeline()
        .start_login(
            PrimaryIdentity {
                user: s.user.clone(),
                provider: "local".into(),
                amr: vec!["pwd".into()],
            },
            &ClientInfo::default(),
        )
        .await
        .unwrap();
    s.challenge = None;
    s.offered.clear();
    s.denied = None;
    match outcome {
        LoginOutcome::Challenge(o) => {
            s.challenge = Some(o.challenge_id);
            s.offered = o.factors.into_iter().map(|f| f.id).collect();
        }
        LoginOutcome::Denied { code, .. } => s.denied = Some(code.to_string()),
        LoginOutcome::Session(_) => panic!("a session without a second factor"),
    }
}

#[when("alice steps up a password session")]
async fn when_step_up(world: &mut TestWorld) {
    let s = scenario(world);
    let p = s.pipeline();
    p.issue_session(&s.user, &["pwd".into()], 1).await.unwrap();
    let session: String = sqlx::query_scalar(
        "SELECT id FROM auth_sessions WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(s.user.id.to_string())
    .fetch_one(&s.pool)
    .await
    .unwrap();
    let result = p
        .start_step_up(&s.user, &session, &ClientInfo::default())
        .await;
    drop(p);
    s.challenge = None;
    s.offered.clear();
    s.denied = None;
    match result {
        Ok(offer) => {
            s.challenge = Some(offer.challenge_id);
            s.offered = offer.factors.into_iter().map(|f| f.id).collect();
        }
        Err(e) => s.denied = Some(code_of(&e)),
    }
}

#[given(expr = "step-up freshness is {int} seconds")]
async fn given_freshness(world: &mut TestWorld, secs: u64) {
    scenario(world).config.auth.step_up.max_age_secs = secs;
}

#[given(expr = "alice holds a level 2 session verified {int} seconds ago")]
async fn given_old_session(world: &mut TestWorld, age: i64) {
    let s = scenario(world);
    s.pipeline()
        .issue_session(&s.user, &["pwd".into(), "hwk".into()], 2)
        .await
        .unwrap();
    let id: String = sqlx::query_scalar(
        "SELECT id FROM auth_sessions WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(s.user.id.to_string())
    .fetch_one(&s.pool)
    .await
    .unwrap();
    let at = (chrono::Utc::now() - chrono::Duration::seconds(age)).to_rfc3339();
    sqlx::query("UPDATE auth_sessions SET created_at = ? WHERE id = ?")
        .bind(at)
        .bind(&id)
        .execute(&s.pool)
        .await
        .unwrap();
    s.session = Some(id);
}

#[then(
    regex = r"^a sensitive action should (pass without step-up|need step-up because the verification is too old)$"
)]
async fn then_sensitive_action(world: &mut TestWorld, outcome: String) {
    let s = scenario(world);
    let id = s.session.clone().expect("a session");
    let got = s
        .pipeline()
        .step_up_check(s.user.id, s.user.organization_id, &id)
        .await
        .unwrap();
    let want = if outcome.starts_with("pass") {
        StepUpCheck::Passed
    } else {
        StepUpCheck::Required {
            required_acr: 2,
            stale: true,
        }
    };
    assert_eq!(got, want);
}

#[given("the policy requires a phishing-resistant factor")]
async fn given_pr_policy(world: &mut TestWorld) {
    let s = scenario(world);
    policy::replace(
        &s.pool,
        s.user.organization_id,
        &[PolicyInput {
            name: "keys".into(),
            enabled: true,
            priority: 100,
            conditions: Default::default(),
            required_acr: 2,
            require_phishing_resistant: true,
        }],
    )
    .await
    .unwrap();
}

#[then(expr = "the plugin should have received the purpose {string}")]
async fn then_purpose(world: &mut TestWorld, purpose: String) {
    let echoed = scenario(world).echoed.clone().expect("an echoed request");
    assert_eq!(echoed["handler"], "begin_challenge");
    assert_eq!(echoed["input"]["purpose"], purpose.as_str());
}

#[when(expr = "alice begins the challenge with the factor {string}")]
async fn when_begin(world: &mut TestWorld, factor: String) {
    let s = scenario(world);
    let id = s.challenge.clone().expect("a challenge");
    let client = s
        .pipeline()
        .begin_challenge(&id, &factor, &ClientInfo::default())
        .await
        .unwrap();
    s.echoed = Some(client);
}

#[when(expr = "alice answers the challenge with the factor {string}")]
async fn when_answer(world: &mut TestWorld, factor: String) {
    let s = scenario(world);
    let id = s.challenge.clone().expect("a challenge");
    let p = s.pipeline();
    let c = ClientInfo::default();
    let result = match p.begin_challenge(&id, &factor, &c).await {
        Err(e) => Err(code_of(&e)),
        Ok(_) => match p
            .verify_challenge(&id, Some(&factor), json!({"credential": {}}), &c)
            .await
        {
            Ok(VerifyOutcome::Session(_)) => Ok(()),
            Ok(VerifyOutcome::Failed { .. }) => Err("factor_failed".into()),
            Ok(VerifyOutcome::Pending) => Err("pending".into()),
            Err(e) => Err(code_of(&e)),
        },
    };
    s.answer = Some(result);
}

#[then(expr = "a challenge should be offered with the factors {string}")]
async fn then_offered(world: &mut TestWorld, factors: String) {
    let want: Vec<String> = factors.split(',').map(|f| f.trim().to_string()).collect();
    assert_eq!(scenario(world).offered, want);
}

#[then(expr = "the sign-in should be refused with {string}")]
async fn then_sign_in_refused(world: &mut TestWorld, code: String) {
    let s = scenario(world);
    assert_eq!(s.denied.as_deref(), Some(code.as_str()), "{:?}", s.offered);
}

#[then(expr = "the answer should be refused with {string}")]
async fn then_answer_refused(world: &mut TestWorld, code: String) {
    assert_eq!(scenario(world).answer, Some(Err(code)));
}

#[then(expr = "alice should hold a session with the methods {string} at level {int}")]
async fn then_session(world: &mut TestWorld, methods: String, acr: i64) {
    let s = scenario(world);
    assert_eq!(s.answer, Some(Ok(())));
    let want: Vec<&str> = methods.split(',').collect();
    let sessions = s.sessions().await;
    assert_eq!(sessions.len(), 1, "exactly one new session");
    let amr: Vec<String> = serde_json::from_str(&sessions[0].0).unwrap();
    assert_eq!(amr, want);
    assert_eq!(sessions[0].1, acr);
}

#[then("alice should hold no session")]
async fn then_no_session(world: &mut TestWorld) {
    assert!(scenario(world).sessions().await.is_empty());
}

#[then(expr = "the factor {string} should not be registered")]
async fn then_not_registered(world: &mut TestWorld, factor: String) {
    let s = scenario(world);
    assert!(!s.manager.auth_factor_registry().ids().contains(&factor));
    assert!(!s.pipeline().descriptors().iter().any(|d| d.id == factor));
}

#[then(expr = "the plugin {string} should be in the state {string}")]
async fn then_state(world: &mut TestWorld, id: String, state: String) {
    let got = scenario(world)
        .manager
        .get(&id)
        .await
        .unwrap()
        .summary
        .state;
    let want = match state.as_str() {
        "error" => PluginState::Error,
        "enabled" => PluginState::Enabled,
        "disabled" => PluginState::Disabled,
        other => panic!("unknown state '{other}'"),
    };
    assert_eq!(got, want);
}

#[then(expr = "the plugin should have received exactly alice's {string} records")]
async fn then_own_records(world: &mut TestWorld, factor: String) {
    let s = scenario(world);
    let echoed = s.echoed.clone().expect("an echoed request");
    assert_eq!(echoed["kind"], "factor");
    assert_eq!(echoed["handler"], "begin_challenge");
    let input = &echoed["input"];
    assert_eq!(input["user_id"], s.user.id.to_string());
    assert_eq!(input["factor"], factor.split(':').nth(1).unwrap());
    let got: HashSet<String> = input["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    let want: HashSet<String> = s.records[&factor].iter().cloned().collect();
    assert_eq!(got, want);
    let text = echoed.to_string();
    for (kind, ids) in &s.records {
        if kind != &factor {
            for id in ids {
                assert!(
                    !text.contains(id.as_str()),
                    "{factor} received a {kind} record"
                );
            }
        }
    }
}

// ---- crypto.random ---------------------------------------------------------

#[given(
    regex = r#"^an enabled WASM plugin "([a-z0-9-]+)" (with|without) crypto:random that asks for (-?\d+) random bytes$"#
)]
async fn given_random_plugin(world: &mut TestWorld, id: String, with: String, len: String) {
    let caps = if with == "with" {
        "  - crypto:random\n"
    } else {
        "  - kv:rw\n"
    };
    let yaml = format!(
        "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
         runtime: {{ kind: wasm, entry: plugin.wasm, limits: {{ timeout_ms: 5000 }} }}\n\
         capabilities:\n{caps}"
    );
    let approve = if with == "with" {
        "crypto:random"
    } else {
        "kv:rw"
    };
    let s = scenario(world);
    let wasm = host_call_guest("crypto.random", &format!(r#"{{"len":{len}}}"#));
    install(s, &id, &yaml, &wasm, &[approve]).await;
    s.records.insert("plugin".into(), vec![id]);
}

#[when(expr = "the plugin runs {int} times")]
async fn when_plugin_runs(world: &mut TestWorld, n: usize) {
    let s = scenario(world);
    let id = s.records["plugin"][0].clone();
    for _ in 0..n {
        let call =
            s.manager
                .hooks()
                .get(&id)
                .unwrap()
                .call(HandlerKind::Event, "on_event", json!({}));
        let out = s.manager.invoke(call).await.unwrap();
        s.outputs.push(out);
    }
}

#[then(expr = "each run should receive {int} random bytes in base64url and the runs should differ")]
async fn then_random(world: &mut TestWorld, len: usize) {
    let s = scenario(world);
    let decoded: Vec<Vec<u8>> = s
        .outputs
        .iter()
        .map(|o| {
            let b64 = o["ok"]["bytes"].as_str().expect("bytes");
            assert!(!b64.contains(['=', '+', '/']), "{b64}");
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(b64)
                .unwrap()
        })
        .collect();
    assert!(decoded.len() >= 2);
    assert!(decoded.iter().all(|d| d.len() == len));
    assert_ne!(decoded[0], decoded[1]);
}

#[then(expr = "the host call should fail with {string}")]
async fn then_host_call_failed(world: &mut TestWorld, code: String) {
    let s = scenario(world);
    let out = s.outputs.last().expect("a run");
    assert_eq!(out["error"]["code"], code.as_str(), "{out}");
}
