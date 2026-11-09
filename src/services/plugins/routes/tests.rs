//! Unit tests of plugin API routes (H-7): declarations, matching, limits,
//! answers, the governed invocation (builtin and WASM) and the audit actor.

use super::*;
use crate::models::ApproveCapabilitiesRequest;
use crate::services::plugins::host::{HostApiImpl, PackageModuleSource};
use crate::services::plugins::invoke::HandlerInvoker;
use crate::services::plugins::manager::BuiltinPlugin;
use crate::services::plugins::manifest::parse_and_validate;
use crate::services::plugins::package::build_signed_package;
use crate::services::plugins::storage::{PluginStorage, StorageLimits};
use crate::services::plugins::wasm::{WasmConfig, WasmInvoker};
use ed25519_dalek::SigningKey;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::collections::BTreeSet;

const SEED: [u8; 32] = [71; 32];

fn decl(method: &str, path: &str) -> ApiRouteDecl {
    ApiRouteDecl {
        method: method.into(),
        path: path.into(),
        handler: None,
        requires: None,
        rate_limit: None,
        step_up: false,
    }
}

fn route(method: &str, path: &str) -> PluginRoute {
    PluginRoute::from_decl(&decl(method, path)).unwrap()
}

fn segs(p: &str) -> Vec<String> {
    p.split('/').map(str::to_string).collect()
}

// ---- declarations -----------------------------------------------------------

#[test]
fn route_paths_are_parsed_and_unsafe_ones_refused() {
    assert_eq!(
        parse_route_path("/me/credentials/{credential_ref}").unwrap(),
        vec![
            Segment::Static("me".into()),
            Segment::Static("credentials".into()),
            Segment::Param("credential_ref".into()),
        ]
    );
    assert!(parse_route_path("/admin/policy/preview").is_ok());
    for bad in [
        "",
        "me",
        "/",
        "/me/",
        "//me",
        "/me//x",
        "/Me",
        "/me/../admin",
        "/me/./x",
        "/me/{}",
        "/me/{Ref}",
        "/me/{a}/{a}",
        "/me/{a}x",
        "/me/{*rest}",
        "/{x}/me",
        "/config",
        "/jobs/x",
        "/webhooks/{hook}",
        "/a/b/c/d/e/f/g/h/i",
        "/me%2fx",
        "/me x",
    ] {
        assert!(parse_route_path(bad).is_err(), "{bad:?} must be refused");
    }
    assert!(parse_route_path(&format!("/{}", "a".repeat(130))).is_err());
}

#[test]
fn rate_limits_are_parsed_strictly() {
    let r = parse_rate_limit("60/min").unwrap();
    assert_eq!((r.count, r.period), (60, RatePeriod::Minute));
    assert_eq!(parse_rate_limit("10/s").unwrap().period, RatePeriod::Second);
    assert_eq!(parse_rate_limit("100/h").unwrap().period, RatePeriod::Hour);
    for bad in [
        "0/min", "60", "60/day", "x/min", "-1/s", "+5/s", "10001/s", " 5/s", "5/ s", "",
    ] {
        assert!(parse_rate_limit(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn declarations_default_their_handler_and_rate() {
    let r = route("GET", "/me/credentials");
    assert_eq!(r.handler, DEFAULT_ROUTE_HANDLER);
    assert_eq!(r.rate, parse_rate_limit(DEFAULT_RATE_LIMIT).unwrap());
    assert!(!r.step_up);
    for m in ["HEAD", "OPTIONS", "TRACE", "get", "CONNECT"] {
        assert!(PluginRoute::from_decl(&decl(m, "/x")).is_err(), "{m}");
    }
    let mut d = decl("GET", "/x");
    d.handler = Some("Bad-Name".into());
    assert!(PluginRoute::from_decl(&d).is_err());
}

#[test]
fn fixed_segments_win_and_parameters_are_extracted() {
    let routes = vec![
        route("GET", "/a/{x}/c"),
        route("GET", "/a/b/{y}"),
        route("GET", "/me/credentials/{credential_ref}"),
        route("PATCH", "/me/credentials/{credential_ref}"),
        route("GET", "/me/credentials/summary"),
    ];
    let (r, p) = find_route(&routes, "GET", &segs("a/b/c")).unwrap();
    assert_eq!(r.path, "/a/b/{y}");
    assert_eq!(p.get("y").map(String::as_str), Some("c"));
    let (r, _) = find_route(&routes, "GET", &segs("me/credentials/summary")).unwrap();
    assert_eq!(r.path, "/me/credentials/summary");
    let (r, p) = find_route(&routes, "PATCH", &segs("me/credentials/k1")).unwrap();
    assert_eq!(r.method, "PATCH");
    assert_eq!(p.get("credential_ref").map(String::as_str), Some("k1"));
    assert!(find_route(&routes, "DELETE", &segs("me/credentials/k1")).is_none());
    assert!(find_route(&routes, "GET", &segs("me/credentials")).is_none());
    assert!(find_route(&routes, "GET", &segs("me/credentials/k1/x")).is_none());
}

#[test]
fn request_paths_never_smuggle_separators() {
    assert_eq!(
        request_segments(
            "/api/v1/plugins/p/me/credentials/k%201",
            "me/credentials/k 1"
        ),
        Some(segs("me/credentials/k 1"))
    );
    for (raw, rest) in [
        ("/api/v1/plugins/p/me%2Fcredentials", "me/credentials"),
        ("/api/v1/plugins/p/me%2fcredentials", "me/credentials"),
        ("/api/v1/plugins/p/me%5Cx", "me\\x"),
        ("/api/v1/plugins/p/me/%00", "me/\0"),
        ("/api/v1/plugins/p/me/", "me/"),
        ("/api/v1/plugins/p/me//x", "me//x"),
        ("/api/v1/plugins/p/me/..", "me/.."),
        ("/api/v1/plugins/p/me/%2e%2e", "me/.."),
        ("/api/v1/plugins/p/me/.", "me/."),
        ("/api/v1/plugins/p/me/%0a", "me/\n"),
    ] {
        assert_eq!(request_segments(raw, rest), None, "{raw}");
    }
}

#[test]
fn credential_query_keys_are_dropped_and_limits_enforced() {
    let q = route_query(vec![
        ("limit".into(), "5".into()),
        ("token".into(), "eyJ.secret".into()),
        ("Access_Token".into(), "x".into()),
        ("api_key".into(), "k".into()),
    ])
    .unwrap();
    assert_eq!(q.len(), 1);
    assert_eq!(q.get("limit").map(String::as_str), Some("5"));
    assert!(route_query(vec![("a".into(), "1".into()), ("a".into(), "2".into())]).is_err());
    assert!(route_query(vec![("".into(), "1".into())]).is_err());
    assert!(route_query(vec![("a".into(), "x".repeat(1025))]).is_err());
    assert!(route_query(vec![("a".into(), "x\ny".into())]).is_err());
    let many = (0..33)
        .map(|i| (format!("k{i}"), "v".to_string()))
        .collect();
    assert!(route_query(many).is_err());
}

#[test]
fn replies_are_bounded_to_the_allowed_statuses() {
    assert_eq!(
        parse_route_reply(json!({})).unwrap(),
        RouteReply {
            status: 200,
            body: None
        }
    );
    let conflict = json!({"status": 409, "body": {"error": {"code": "already_suspended"}}});
    assert_eq!(parse_route_reply(conflict).unwrap().status, 409);
    assert_eq!(
        parse_route_reply(json!({"status": 204})).unwrap(),
        RouteReply {
            status: 204,
            body: None
        }
    );
    for s in ALLOWED_STATUSES {
        assert!(parse_route_reply(json!({ "status": s })).is_ok(), "{s}");
    }
    for bad in [
        json!({"status": 302, "body": {"location": "https://evil.example"}}),
        json!({"status": 401, "body": {"code": "step_up_required"}}),
        json!({"status": 500}),
        json!({"status": 503}),
        json!({"status": 100}),
        json!({"status": 65736}),
        json!({"status": "200"}),
        json!({"status": 200, "headers": {"Set-Cookie": "a=b"}}),
        json!({"status": 204, "body": {}}),
        json!([200]),
        json!("ok"),
    ] {
        assert!(parse_route_reply(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn rate_limits_are_per_route_and_per_user() {
    let rt = RouteRuntime::default();
    let mut d = decl("GET", "/limited");
    d.rate_limit = Some("2/min".into());
    let limited = PluginRoute::from_decl(&d).unwrap();
    let other = route("GET", "/other");
    let (alice, bob) = (Uuid::new_v4(), Uuid::new_v4());
    assert!(rt.check("p", &limited, alice).is_ok());
    assert!(rt.check("p", &limited, alice).is_ok());
    let wait = rt.check("p", &limited, alice).unwrap_err();
    assert!(
        wait > Duration::ZERO && wait <= Duration::from_secs(30),
        "{wait:?}"
    );
    assert!(rt.check("p", &limited, bob).is_ok(), "another user");
    assert!(rt.check("p", &other, alice).is_ok(), "another route");
    assert!(rt.check("q", &limited, alice).is_ok(), "another plugin");
    // Pruning drops the state of routes no enabled plugin declares.
    rt.prune(&HookIndex::default());
    assert!(rt.check("p", &limited, alice).is_ok());
}

// ---- manifests --------------------------------------------------------------

fn yaml(id: &str, runtime: &str, caps: &str, routes: &str) -> String {
    format!(
        "api_version: 2\nid: {id}\nname: {id}\nversion: 1.0.0\npublisher: ACME\n\
         runtime: {runtime}\ncapabilities:\n{caps}contributes:\n  api_routes:\n{routes}"
    )
}

const BUILTIN: &str = "{ kind: builtin, entry: probe }";
const CAP: &str = "  - api:routes\n";
const ONE: &str = "    - { method: GET, path: /me/credentials, rate_limit: 60/min }\n";

fn problems(y: &str) -> Vec<String> {
    parse_and_validate(y.as_bytes(), &BTreeSet::new())
        .err()
        .unwrap_or_default()
}

#[test]
fn route_manifests_are_validated() {
    assert!(problems(&yaml("probe", BUILTIN, CAP, ONE)).is_empty());
    let spec_like = "    - { method: PATCH, path: \"/me/credentials/{credential_ref}\", rate_limit: 10/min }\n    \
        - method: POST\n      path: \"/admin/credentials/{credential_ref}/suspend\"\n      \
        requires: { resource: plugin.probe.admin, action: write }\n      rate_limit: 20/min\n      \
        step_up: true\n      handler: suspend\n    \
        - { method: GET, path: /admin/summary, requires: { resource: nodes, action: read } }\n";
    assert_eq!(
        problems(&yaml("probe", BUILTIN, CAP, spec_like)),
        Vec::<String>::new()
    );

    let route_yaml = |r: &str| yaml("probe", BUILTIN, CAP, r);
    let cases: Vec<(String, &str)> = vec![
        (
            yaml("probe", BUILTIN, "  - kv:rw\n", ONE),
            "api_routes need the capability api:routes",
        ),
        (
            yaml("probe", BUILTIN, "  - api:routes: { optional: true }\n", ONE),
            "api:routes cannot be optional",
        ),
        (
            yaml(
                "probe",
                "{ kind: remote, endpoint: \"https://x.example\" }",
                CAP,
                ONE,
            ),
            "api_routes need the wasm or builtin runtime",
        ),
        (
            yaml("trust", BUILTIN, CAP, ONE),
            "plugin id 'trust' cannot serve api_routes",
        ),
        (
            route_yaml("    - { method: GET, path: /config }\n"),
            "would shadow the core routes",
        ),
        (
            route_yaml("    - { method: TRACE, path: /x }\n"),
            "method 'TRACE' is not supported",
        ),
        (
            route_yaml("    - { method: GET, path: /x, rate_limit: 60/day }\n"),
            "rate_limit '60/day' is invalid",
        ),
        (
            route_yaml(
                "    - { method: GET, path: \"/x/{a}\" }\n    - { method: GET, path: \"/x/{b}\" }\n",
            ),
            "is declared twice",
        ),
        (
            route_yaml(
                "    - { method: GET, path: /x, requires: { resource: nodes, action: fly } }\n",
            ),
            "is not a core permission",
        ),
        (
            route_yaml("    - { method: GET, path: /x, requires: { resource: plugin.other.admin, action: read } }\n"),
            "must be a core resource or start with plugin.probe.",
        ),
        (
            route_yaml("    - { method: GET, path: /x, headers: true }\n"),
            "unknown field",
        ),
    ];
    for (y, want) in cases {
        let p = problems(&y);
        assert!(
            p.iter().any(|m| m.contains(want)),
            "want {want:?}, got {p:?}"
        );
    }
    let many: String = (0..33)
        .map(|i| format!("    - {{ method: GET, path: /r{i} }}\n"))
        .collect();
    assert!(problems(&route_yaml(&many))
        .iter()
        .any(|m| m.contains("more than 32 api_routes")));
}

// ---- environment ------------------------------------------------------------

#[derive(Default)]
struct Capture {
    calls: Mutex<Vec<HandlerCall>>,
}

impl BuiltinPlugin for Capture {
    fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
        self.calls.lock().unwrap().push(call.clone());
        Ok(match call.handler.as_str() {
            "conflict" => {
                json!({"status": 409, "body": {"error": {"code": "already_suspended"}}})
            }
            "redirect" => json!({"status": 302, "body": {}}),
            "fail" => anyhow::bail!("boom"),
            _ => json!({"status": 200, "body": {"seen": call.input["params"]}}),
        })
    }
}

struct Env {
    manager: Arc<PluginManager>,
    pool: SqlitePool,
    capture: Arc<Capture>,
}

async fn env(policy: crate::config::PluginPolicyConfig) -> Env {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let dir = std::env::temp_dir().join(format!("ovx_routes_{}", Uuid::new_v4().simple()));
    std::fs::create_dir_all(&dir).unwrap();
    let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
    let capture = Arc::new(Capture::default());
    let manager = Arc::new(
        PluginManager::new(pool.clone(), dir.clone(), 8)
            .with_builtin_keys(vec![("T".into(), pk)])
            .with_builtin_runtime("probe", capture.clone())
            .with_policy(policy.clone()),
    );
    manager.reload().await.unwrap();
    let storage = Arc::new(PluginStorage::new(
        pool.clone(),
        dir.clone(),
        StorageLimits::default(),
    ));
    let host = Arc::new(HostApiImpl::new(&manager, policy, storage.clone(), None));
    let source = Arc::new(PackageModuleSource {
        data_dir: dir,
        max_package_bytes: 8 * 1024 * 1024,
    });
    let wasm = Arc::new(WasmInvoker::new(WasmConfig::default(), source, host).unwrap());
    manager.attach_wasm(storage, wasm);
    Env {
        manager,
        pool,
        capture,
    }
}

const PROBE_ROUTES: &str =
    "    - { method: PATCH, path: \"/me/credentials/{credential_ref}\" }\n    \
    - { method: POST, path: /conflict, handler: conflict }\n    \
    - { method: GET, path: /redirect, handler: redirect }\n    \
    - { method: GET, path: /fail, handler: fail }\n";

impl Env {
    async fn install_probe(&self) {
        let y = yaml("probe", BUILTIN, CAP, PROBE_ROUTES);
        let pkg = build_signed_package(&[("plugin.yaml", y.as_bytes())], "probe", "1.0.0", 2, SEED);
        self.manager.install(&pkg, "admin", None).await.unwrap();
        self.manager.enable("probe").await.unwrap();
    }

    async fn install_wasm(&self, id: &str, routes: &str, wasm: &[u8]) {
        let y = yaml(
            id,
            "{ kind: wasm, entry: plugin.wasm, limits: { timeout_ms: 300 } }",
            "  - api:routes\n  - audit:write\n",
            routes,
        );
        let pkg = build_signed_package(
            &[("plugin.yaml", y.as_bytes()), ("plugin.wasm", wasm)],
            id,
            "1.0.0",
            2,
            SEED,
        );
        self.manager.install(&pkg, "admin", None).await.unwrap();
        self.manager
            .approve_capabilities(
                id,
                &ApproveCapabilitiesRequest {
                    version: None,
                    capabilities: vec!["api:routes".into(), "audit:write".into()],
                },
                "admin",
            )
            .await
            .unwrap();
        self.manager.enable(id).await.unwrap();
    }

    async fn call(
        &self,
        plugin: &str,
        method: &str,
        path: &str,
        caller: &RouteCaller,
    ) -> Result<RouteReply, RouteFailure> {
        let r = self
            .manager
            .resolve_route(plugin, method, &segs(path))
            .ok_or(RouteFailure::NotFound)?;
        let req = RouteRequest {
            query: BTreeMap::from([("limit".to_string(), "5".to_string())]),
            body: Some(json!({"nickname": "Desk"})),
            request_id: Uuid::new_v4(),
        };
        self.manager.invoke_route(&r, caller, &req).await
    }
}

fn alice() -> RouteCaller {
    RouteCaller {
        user_id: Uuid::new_v4(),
        organization_id: crate::models::default_organization_uuid(),
        username: "alice".into(),
        roles: vec!["viewer".into()],
    }
}

// ---- builtin routes ---------------------------------------------------------

#[tokio::test]
async fn a_route_call_carries_the_delegated_identity_and_nothing_else() {
    let e = env(Default::default()).await;
    e.install_probe().await;
    let caller = alice();
    let reply = e
        .call("probe", "PATCH", "me/credentials/k1", &caller)
        .await
        .unwrap();
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, Some(json!({"seen": {"credential_ref": "k1"}})));
    let calls = e.capture.calls.lock().unwrap();
    let c = calls.last().unwrap();
    assert_eq!(c.kind, HandlerKind::Route);
    assert_eq!(c.handler, DEFAULT_ROUTE_HANDLER);
    assert_eq!(
        c.actor,
        Some(DelegatedActor {
            user_id: caller.user_id,
            organization_id: caller.organization_id,
            username: "alice".into(),
        })
    );
    let keys: BTreeSet<&str> = c
        .input
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        BTreeSet::from([
            "method",
            "route",
            "params",
            "query",
            "body",
            "user",
            "request_id",
            "now_unix"
        ])
    );
    assert_eq!(c.input["method"], "PATCH");
    assert_eq!(c.input["route"], "/me/credentials/{credential_ref}");
    assert_eq!(c.input["query"], json!({"limit": "5"}));
    assert_eq!(c.input["body"], json!({"nickname": "Desk"}));
    assert_eq!(
        c.input["user"],
        json!({
            "id": caller.user_id,
            "username": "alice",
            "organization_id": caller.organization_id,
            "roles": ["viewer"],
        })
    );
}

#[tokio::test]
async fn statuses_outside_the_allow_set_and_handler_errors_fail_closed() {
    let e = env(Default::default()).await;
    e.install_probe().await;
    let r = e.call("probe", "POST", "conflict", &alice()).await.unwrap();
    assert_eq!(r.status, 409);
    assert_eq!(
        e.call("probe", "GET", "redirect", &alice()).await,
        Err(RouteFailure::Unavailable)
    );
    assert_eq!(
        e.call("probe", "GET", "fail", &alice()).await,
        Err(RouteFailure::Unavailable)
    );
}

#[tokio::test]
async fn routes_follow_the_enabled_set() {
    let e = env(Default::default()).await;
    let resolve = || {
        e.manager
            .resolve_route("probe", "PATCH", &segs("me/credentials/k1"))
    };
    assert!(resolve().is_none());
    e.install_probe().await;
    let r = resolve().expect("served once enabled");
    assert!(e.manager.route_admitted(&r).await);
    e.manager.disable("probe").await.unwrap();
    assert!(resolve().is_none());
    // A route resolved before the disable is no longer admitted.
    assert!(!e.manager.route_admitted(&r).await);
    assert_eq!(
        e.manager
            .invoke_route(&r, &alice(), &RouteRequest::default())
            .await,
        Err(RouteFailure::NotFound)
    );
    e.manager.enable("probe").await.unwrap();
    assert!(resolve().is_some());
    e.manager.uninstall("probe", true).await.unwrap();
    assert!(resolve().is_none());
    // Core segments and reserved plugin ids never resolve.
    assert!(e
        .manager
        .resolve_route("probe", "GET", &segs("config"))
        .is_none());
    assert!(e
        .manager
        .resolve_route("trust", "GET", &segs("keys"))
        .is_none());
}

#[tokio::test]
async fn a_policy_that_denies_api_routes_closes_every_route() {
    let policy = crate::config::PluginPolicyConfig {
        denied_capabilities: vec!["api:*".into()],
    };
    let e = env(policy).await;
    e.install_probe().await;
    let r = e
        .manager
        .resolve_route("probe", "PATCH", &segs("me/credentials/k1"))
        .unwrap();
    assert!(!e.manager.route_admitted(&r).await);
    assert_eq!(
        e.manager
            .invoke_route(&r, &alice(), &RouteRequest::default())
            .await,
        Err(RouteFailure::NotFound)
    );
    assert!(e.capture.calls.lock().unwrap().is_empty());
}

// ---- WASM routes ------------------------------------------------------------

#[derive(Clone, Copy)]
enum Out<'a> {
    Json(&'a str),
    Echo,
    Trap,
    Spin,
    /// `audit.write` naming a forged user, then the JSON.
    Audit(&'a str),
}

fn wat_str(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

const ECHO_PREFIX: &str = r#"{"ok":{"status":200,"body":"#;
const EMPTY_OK: &str = r#"{"ok":{}}"#;
const AUDIT_OP: &str = "audit.write";
const FORGED: &str = "00000000-0000-0000-0000-000000000001";

fn audit_request() -> String {
    format!(
        r#"{{"action":"probe","user_id":"{FORGED}","actor":"{FORGED}","organization_id":"{FORGED}","details":{{"user_id":"{FORGED}"}}}}"#
    )
}

/// A guest dispatching on the handler's first letter, at offset 27 for the
/// kinds `route` and `event`; lifecycle hooks answer `{}`. `Echo` answers
/// the whole guest request as the body.
fn route_guest(handlers: &[(char, Out)]) -> Vec<u8> {
    let mut data = String::new();
    let mut funcs = String::new();
    let mut dispatch = String::new();
    let pre = ECHO_PREFIX.len();
    let req = audit_request();
    for (i, (letter, out)) in handlers.iter().enumerate() {
        let off = 1024 + i * 4096;
        let mut fixed = |s: &str| {
            data += &format!("  (data (i32.const {off}) \"{}\")\n", wat_str(s));
            format!(
                "(i64.or (i64.shl (i64.const {off}) (i64.const 32)) (i64.const {}))",
                s.len()
            )
        };
        let body = match out {
            Out::Json(s) => fixed(s),
            Out::Trap => "(unreachable)".to_string(),
            Out::Spin => "(loop $spin (br $spin)) (unreachable)".to_string(),
            Out::Echo => format!(
                "(local.set $out (call $alloc (i32.add (local.get $l) (i32.const {n})))) \
                 (memory.copy (local.get $out) (i32.const 60000) (i32.const {pre})) \
                 (memory.copy (i32.add (local.get $out) (i32.const {pre})) (local.get $p) (local.get $l)) \
                 (memory.copy (i32.add (i32.add (local.get $out) (i32.const {pre})) (local.get $l)) (i32.const 60100) (i32.const 2)) \
                 (i64.or (i64.shl (i64.extend_i32_u (local.get $out)) (i64.const 32)) \
                         (i64.extend_i32_u (i32.add (local.get $l) (i32.const {n}))))",
                n = pre + 2
            ),
            Out::Audit(s) => format!(
                "(drop (call $hc (i32.const 60300) (i32.const {oplen}) (i32.const 60400) (i32.const {reqlen}))) {}",
                fixed(s),
                oplen = AUDIT_OP.len(),
                reqlen = req.len(),
            ),
        };
        funcs += &format!(
            "  (func $h{i} (param $p i32) (param $l i32) (result i64) (local $out i32) {body})\n"
        );
        dispatch += &format!(
            "    (if (i32.eq (local.get $c) (i32.const {})) (then (return (call $h{i} (local.get $p) (local.get $l)))))\n",
            *letter as u32
        );
    }
    let wat = format!(
        r#"(module
  (import "openvox" "host_call" (func $hc (param i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 4)
  (global $bump (mut i32) (i32.const 131072))
{data}  (data (i32.const 60000) "{prefix}")
  (data (i32.const 60100) "}}}}")
  (data (i32.const 60200) "{empty}")
  (data (i32.const 60300) "{op}")
  (data (i32.const 60400) "{req}")
  (func $alloc (export "alloc") (param $n i32) (result i32)
    (local $r i32)
    (local.set $r (global.get $bump))
    (global.set $bump (i32.add (global.get $bump) (local.get $n)))
    (local.get $r))
{funcs}  (func (export "handle") (param $p i32) (param $l i32) (result i64)
    (local $c i32)
    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 9))) (i32.const 108))
      (then (return (i64.or (i64.shl (i64.const 60200) (i64.const 32)) (i64.const {empty_len})))))
    (local.set $c (i32.load8_u (i32.add (local.get $p) (i32.const 27))))
{dispatch}    (unreachable)))"#,
        prefix = wat_str(ECHO_PREFIX),
        empty = wat_str(EMPTY_OK),
        empty_len = EMPTY_OK.len(),
        op = AUDIT_OP,
        req = wat_str(&req),
    );
    wat::parse_str(wat).unwrap()
}

const WASM_ROUTES: &str =
    "    - { method: PATCH, path: \"/me/credentials/{credential_ref}\", handler: echo }\n    \
     - { method: GET, path: /boom, handler: boom }\n    \
     - { method: GET, path: /spin, handler: spin }\n    \
     - { method: GET, path: /headers, handler: headers }\n    \
     - { method: POST, path: /audit, handler: audit }\n";

fn wasm_guest() -> Vec<u8> {
    route_guest(&[
        ('e', Out::Echo),
        ('b', Out::Trap),
        ('s', Out::Spin),
        (
            'h',
            Out::Json(r#"{"ok":{"status":200,"body":{},"headers":{"Set-Cookie":"a=b"}}}"#),
        ),
        (
            'a',
            Out::Audit(r#"{"ok":{"status":201,"body":{"audited":true}}}"#),
        ),
    ])
}

#[tokio::test]
async fn a_wasm_route_echoes_its_input_and_fails_closed() {
    let e = env(Default::default()).await;
    e.install_wasm("keys", WASM_ROUTES, &wasm_guest()).await;
    let caller = alice();
    let r = e
        .call("keys", "PATCH", "me/credentials/k1", &caller)
        .await
        .unwrap();
    assert_eq!(r.status, 200);
    let request = r.body.unwrap();
    assert_eq!(request["kind"], "route");
    assert_eq!(request["handler"], "echo");
    assert_eq!(request["input"]["params"], json!({"credential_ref": "k1"}));
    assert_eq!(request["input"]["user"]["username"], "alice");
    for path in ["boom", "spin", "headers"] {
        assert_eq!(
            e.call("keys", "GET", path, &caller).await,
            Err(RouteFailure::Unavailable),
            "{path}"
        );
    }
}

async fn audit_users(pool: &SqlitePool, action: &str) -> Vec<Option<String>> {
    sqlx::query_scalar("SELECT user_id FROM audit_log WHERE action = ? ORDER BY rowid")
        .bind(action)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn audit_entries_of_a_route_call_carry_the_calling_user_only() {
    let e = env(Default::default()).await;
    e.install_wasm("keys", WASM_ROUTES, &wasm_guest()).await;
    let caller = alice();
    sqlx::query(
        "INSERT INTO users (id, username, email, password_hash, role) \
         VALUES (?, 'alice', 'alice@example.com', 'x', 'viewer')",
    )
    .bind(caller.user_id.to_string())
    .execute(&e.pool)
    .await
    .unwrap();
    let r = e.call("keys", "POST", "audit", &caller).await.unwrap();
    assert_eq!(r.status, 201);
    assert_eq!(
        audit_users(&e.pool, "plugin.keys.probe").await,
        vec![Some(caller.user_id.to_string())],
        "the actor is the caller, never the user the plugin named"
    );

    // A call of another kind runs as the plugin even if it carries an actor.
    let mut call =
        e.manager
            .hooks()
            .get("keys")
            .unwrap()
            .call(HandlerKind::Event, "audit", json!({}));
    call.actor = Some(DelegatedActor {
        user_id: caller.user_id,
        organization_id: caller.organization_id,
        username: "alice".into(),
    });
    e.manager.invoke(call).await.unwrap();
    assert_eq!(
        audit_users(&e.pool, "plugin.keys.probe").await,
        vec![Some(caller.user_id.to_string()), None]
    );
}
