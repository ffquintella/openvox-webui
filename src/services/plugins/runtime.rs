//! Wiring of the M21 services around one plugin manager.

use std::sync::Arc;

use super::events::{start_dispatcher, Dispatcher, EventBus};
use super::jobs::{start_job_scheduler, JobScheduler};
use super::manager::PluginManager;
use super::webhooks::InboundWebhooks;
use crate::config::PluginEventsConfig;

pub struct PluginRuntime {
    pub events: EventBus,
    pub dispatcher: Arc<Dispatcher>,
    pub jobs: Arc<JobScheduler>,
    pub webhooks: InboundWebhooks,
    cfg: PluginEventsConfig,
}

impl PluginRuntime {
    /// Builds the services without starting any background loop.
    pub fn new(manager: Arc<PluginManager>, cfg: PluginEventsConfig) -> Arc<Self> {
        Arc::new(Self {
            events: EventBus::new(manager.pool().clone()),
            dispatcher: Dispatcher::new(manager.clone(), cfg.clone()),
            jobs: JobScheduler::new(manager.clone()),
            webhooks: InboundWebhooks::new(manager.clone(), cfg.clone()),
            cfg,
        })
    }

    /// Starts the dispatcher and the job scheduler.
    pub fn start(&self) {
        start_dispatcher(self.dispatcher.clone(), self.cfg.clone());
        start_job_scheduler(self.jobs.clone(), self.cfg.job_tick_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PluginEventsRepository;
    use crate::models::EventActor;
    use crate::services::plugins::invoke::{
        invoke_with_timeout, HandlerCall, HandlerInvoker, HandlerKind, InvokeError,
    };
    use crate::services::plugins::manager::BuiltinPlugin;
    use crate::services::plugins::package::build_signed_package;
    use crate::services::plugins::webhooks::{sign, WebhookError};
    use ed25519_dalek::SigningKey;
    use serde_json::{json, Value};
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    const SEED: [u8; 32] = [3; 32];

    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<HandlerCall>>,
        fail: AtomicBool,
    }

    impl BuiltinPlugin for Recorder {
        fn handle(&self, call: &HandlerCall) -> anyhow::Result<Value> {
            self.calls.lock().unwrap().push(call.clone());
            if self.fail.load(Ordering::SeqCst) {
                anyhow::bail!("boom");
            }
            Ok(json!({"handled": call.handler}))
        }
    }

    const YAML: &str = r#"api_version: 2
id: demo
name: Demo
version: 1.0.0
publisher: ACME
runtime: { kind: builtin, entry: demo }
capabilities:
  - events:subscribe
  - events:publish:demo.*
events:
  subscribe:
    - { type: node.classified, priority: 50 }
    - { type: "report.*", filter: "status == 'failed'", handler: on_report }
  publish: [demo.synced]
contributes:
  jobs:
    - { id: nightly, cron: "0 3 * * *", handler: on_nightly }
  webhooks:
    - { id: inbound, auth: hmac-sha256, handler: on_hook }
"#;

    struct Env {
        rt: Arc<PluginRuntime>,
        manager: Arc<PluginManager>,
        rec: Arc<Recorder>,
        pool: sqlx::SqlitePool,
    }

    async fn env(cfg: PluginEventsConfig) -> Env {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let rec = Arc::new(Recorder::default());
        let pk = SigningKey::from_bytes(&SEED).verifying_key().to_bytes();
        let dir = std::env::temp_dir().join(format!("ovx_m21_{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let manager = Arc::new(
            PluginManager::new(pool.clone(), dir, 8)
                .with_builtin_keys(vec![("OpenVox".into(), pk)])
                .with_builtin_runtime("demo", rec.clone()),
        );
        manager.reload().await.unwrap();
        let pkg = build_signed_package(
            &[("plugin.yaml", YAML.as_bytes())],
            "demo",
            "1.0.0",
            2,
            SEED,
        );
        manager.install(&pkg, "admin", None).await.unwrap();
        manager.enable("demo").await.unwrap();
        let rt = PluginRuntime::new(manager.clone(), cfg);
        Env {
            rt,
            manager,
            rec,
            pool,
        }
    }

    fn fast() -> PluginEventsConfig {
        PluginEventsConfig {
            retry_base_secs: 0,
            max_attempts: 3,
            ..Default::default()
        }
    }

    fn handlers(e: &Env) -> Vec<(String, HandlerKind)> {
        e.rec
            .calls
            .lock()
            .unwrap()
            .iter()
            .map(|c| (c.handler.clone(), c.kind))
            .collect()
    }

    #[tokio::test]
    async fn manifest_rules_for_events_jobs_and_webhooks() {
        let bad = |extra: &str| {
            let y = format!(
                "api_version: 2\nid: demo\nname: Demo\nversion: 1.0.0\npublisher: ACME\n\
                 runtime: {{ kind: remote, endpoint: \"https://demo.example.com\" }}\n{extra}"
            );
            match crate::services::plugins::manifest::parse_and_validate(
                y.as_bytes(),
                &Default::default(),
            ) {
                Ok(_) => String::new(),
                Err(p) => p.join("; "),
            }
        };
        assert!(bad("events: { subscribe: [{ type: node.classified }] }\n")
            .contains("events:subscribe"));
        let ns = "capabilities: [events:subscribe, \"events:publish:other.*\"]\nevents: { publish: [other.x] }\n";
        assert!(bad(ns).contains("under 'demo.*'"));
        assert!(
            bad("capabilities: [events:subscribe]\nevents: { publish: [demo.x] }\n")
                .contains("events:publish")
        );
        assert!(bad("capabilities: [events:subscribe]\nevents: { subscribe: [{ type: a.b, filter: \"x ==\" }] }\n").contains("filter"));
        assert!(
            bad("contributes: { jobs: [{ id: j, cron: \"* * *\", handler: h }] }\n")
                .contains("5 fields")
        );
        assert!(
            bad("contributes: { webhooks: [{ id: w, auth: none, handler: h }] }\n")
                .contains("hmac-sha256")
        );
        assert!(bad("capabilities: [events:subscribe, \"events:publish:demo.*\"]\nevents: { subscribe: [{ type: a.b }], publish: [demo.x] }\ncontributes: { jobs: [{ id: j, cron: \"*/5 * * * *\", handler: h }], webhooks: [{ id: w, auth: hmac-sha256, handler: h }] }\n").is_empty());
    }

    #[tokio::test]
    async fn events_reach_subscribers_by_pattern_filter_and_priority() {
        let e = env(fast()).await;
        let bus = &e.rt.events;
        bus.publish(
            "node.classified",
            json!({"certname": "a"}),
            EventActor::system(),
        )
        .await
        .unwrap();
        bus.publish(
            "report.received",
            json!({"status": "failed"}),
            EventActor::system(),
        )
        .await
        .unwrap();
        bus.publish(
            "report.received",
            json!({"status": "changed"}),
            EventActor::system(),
        )
        .await
        .unwrap();
        bus.publish("unrelated.thing", json!({}), EventActor::system())
            .await
            .unwrap();
        e.rt.dispatcher.run_once().await.unwrap();
        let mut got = handlers(&e);
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            got,
            vec![
                ("on_event".to_string(), HandlerKind::Event),
                ("on_report".to_string(), HandlerKind::Event)
            ]
        );
        let stats = PluginEventsRepository::new(&e.pool).stats().await.unwrap();
        assert_eq!(stats.delivered, 2);
        assert_eq!(stats.pending_events, 0);
        let input = &e.rec.calls.lock().unwrap()[0].input;
        assert!(
            input["id"].is_string() && input["type"].is_string() && input["schema_version"] == 1
        );
    }

    #[tokio::test]
    async fn failing_handlers_are_retried_then_dead_lettered_and_can_be_requeued() {
        let e = env(fast()).await;
        e.rec.fail.store(true, Ordering::SeqCst);
        e.rt.events
            .publish("node.classified", json!({}), EventActor::system())
            .await
            .unwrap();
        for _ in 0..3 {
            e.rt.dispatcher.run_once().await.unwrap();
        }
        let repo = PluginEventsRepository::new(&e.pool);
        let dead = repo.list_deliveries(Some("dead_letter"), 10).await.unwrap();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].attempts, 3);
        assert!(dead[0].last_error.as_deref().unwrap().contains("boom"));
        assert_eq!(e.rec.calls.lock().unwrap().len(), 3);

        e.rec.fail.store(false, Ordering::SeqCst);
        assert!(repo.retry_dead_letter(dead[0].id).await.unwrap());
        e.rt.dispatcher.run_once().await.unwrap();
        assert_eq!(repo.stats().await.unwrap().dead_letters, 0);
        assert_eq!(
            repo.list_deliveries(Some("delivered"), 10)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn backoff_doubles_and_is_capped() {
        let e = env(PluginEventsConfig::default()).await;
        let d = &e.rt.dispatcher;
        assert_eq!(d.backoff_secs(0), 30);
        assert_eq!(d.backoff_secs(1), 60);
        assert_eq!(d.backoff_secs(3), 240);
        assert_eq!(d.backoff_secs(30), 3600);
    }

    #[tokio::test]
    async fn a_disabled_plugin_drops_its_pending_deliveries() {
        let e = env(fast()).await;
        e.rt.events
            .publish("node.classified", json!({}), EventActor::system())
            .await
            .unwrap();
        e.rt.dispatcher.fan_out_for_test().await;
        e.manager.disable("demo").await.unwrap();
        e.rt.dispatcher.run_once().await.unwrap();
        assert!(e.rec.calls.lock().unwrap().is_empty());
        let repo = PluginEventsRepository::new(&e.pool);
        assert_eq!(
            repo.list_deliveries(Some("dropped"), 10)
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn outbox_write_is_atomic_with_the_callers_transaction() {
        let e = env(fast()).await;
        let mut tx = e.pool.begin().await.unwrap();
        crate::services::plugins::events::EventBus::publish_in_tx(
            &mut tx,
            "node.classified",
            json!({}),
            EventActor::system(),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            PluginEventsRepository::new(&e.pool)
                .stats()
                .await
                .unwrap()
                .pending_events,
            0
        );
        assert!(e
            .rt
            .events
            .publish("Bad Type", json!({}), EventActor::system())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn plugins_can_only_publish_declared_events() {
        let e = env(fast()).await;
        let hooks = e.manager.hooks();
        let bus = &e.rt.events;
        assert!(bus
            .publish_from_plugin(&hooks, "demo", "demo.synced", json!({}))
            .await
            .is_ok());
        assert!(bus
            .publish_from_plugin(&hooks, "demo", "demo.other", json!({}))
            .await
            .is_err());
        assert!(bus
            .publish_from_plugin(&hooks, "demo", "node.classified", json!({}))
            .await
            .is_err());
        assert!(bus
            .publish_from_plugin(&hooks, "ghost", "demo.synced", json!({}))
            .await
            .is_err());
    }

    struct Slow;
    #[async_trait::async_trait]
    impl HandlerInvoker for Slow {
        async fn invoke(&self, _c: HandlerCall) -> Result<Value, InvokeError> {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(Value::Null)
        }
    }

    #[tokio::test]
    async fn slow_handlers_hit_the_timeout() {
        let e = env(fast()).await;
        let call =
            e.manager
                .hooks()
                .get("demo")
                .unwrap()
                .call(HandlerKind::Event, "on_event", json!({}));
        let r = invoke_with_timeout(&Slow, call, 20).await;
        assert!(matches!(r, Err(InvokeError::Timeout(20))));
    }

    #[tokio::test]
    async fn jobs_run_on_schedule_manually_and_never_overlap() {
        let e = env(fast()).await;
        let jobs = &e.rt.jobs;
        let repo = PluginEventsRepository::new(&e.pool);

        // First tick only computes the next run.
        assert!(jobs.tick().await.unwrap().is_empty());
        let st = repo.job_state("demo", "nightly").await.unwrap().unwrap();
        assert!(st.next_run_at.unwrap() > chrono::Utc::now());

        // Make it due: it runs and gets rescheduled.
        repo.set_next_run(
            "demo",
            "nightly",
            Some(chrono::Utc::now() - chrono::Duration::seconds(5)),
        )
        .await
        .unwrap();
        for h in jobs.tick().await.unwrap() {
            h.await.unwrap();
        }
        let st = repo.job_state("demo", "nightly").await.unwrap().unwrap();
        assert_eq!((st.run_count, st.last_status.as_deref()), (1, Some("ok")));
        assert!(st.next_run_at.unwrap() > chrono::Utc::now());
        assert!(handlers(&e).contains(&("on_nightly".to_string(), HandlerKind::Job)));

        // Manual run, then a failing one is recorded as an error.
        assert_eq!(
            jobs.run_now("demo", "nightly").await.unwrap().trigger,
            "manual"
        );
        e.rec.fail.store(true, Ordering::SeqCst);
        let run = jobs.run_now("demo", "nightly").await.unwrap();
        assert_eq!(run.status, "error");
        assert!(run.error.unwrap().contains("boom"));
        let st = repo.job_state("demo", "nightly").await.unwrap().unwrap();
        assert_eq!((st.run_count, st.failure_count), (3, 1));
        assert!(matches!(
            jobs.run_now("demo", "missing").await,
            Err(crate::services::plugins::jobs::JobError::NotFound)
        ));
        assert_eq!(jobs.runs("demo", "nightly", 10).await.unwrap().len(), 3);
    }

    fn call_hook<'a>(
        e: &'a Env,
        secret: &'a str,
        ts: i64,
        body: &'a [u8],
        sig_for: Option<&'a str>,
    ) -> impl std::future::Future<Output = Result<Value, WebhookError>> + 'a {
        let ts = ts.to_string();
        let sig = format!("sha256={}", sign(sig_for.unwrap_or(secret), &ts, body));
        let body = body.to_vec();
        async move {
            e.rt.webhooks
                .handle(
                    "demo",
                    "inbound",
                    Some(&ts),
                    Some(&sig),
                    Some("application/json"),
                    &body,
                )
                .await
        }
    }

    #[tokio::test]
    async fn inbound_webhooks_need_a_valid_fresh_unreplayed_signature() {
        let e = env(PluginEventsConfig {
            webhook_rate_per_minute: 100,
            ..fast()
        })
        .await;
        let now = chrono::Utc::now().timestamp();
        // No secret yet: the hook does not exist as far as callers can tell.
        assert!(matches!(
            call_hook(&e, "x", now, b"{}", None).await,
            Err(WebhookError::NotFound)
        ));
        assert!(matches!(
            e.rt.webhooks.rotate_secret("demo", "nope").await,
            Err(WebhookError::NotFound)
        ));
        let secret =
            e.rt.webhooks
                .rotate_secret("demo", "inbound")
                .await
                .unwrap();
        assert!(secret.starts_with("whsec_"));
        let listed = e.rt.webhooks.list("demo").await.unwrap();
        assert!(listed[0].secret_configured && listed[0].path.ends_with("/demo/webhooks/inbound"));

        let ok = call_hook(&e, &secret, now, br#"{"a":1}"#, None)
            .await
            .unwrap();
        assert_eq!(ok["handled"], "on_hook");
        let input = e.rec.calls.lock().unwrap().last().unwrap().input.clone();
        assert_eq!(input["body"], r#"{"a":1}"#);
        assert_eq!(input["body_encoding"], "utf8");

        let unauth = |r: Result<Value, WebhookError>| matches!(r, Err(WebhookError::Unauthorized));
        assert!(unauth(
            call_hook(&e, &secret, now, b"{}", Some("wrong")).await
        ));
        assert!(unauth(
            call_hook(&e, &secret, now - 3600, b"{}", None).await
        ));
        // The same signed request twice is a replay.
        assert!(call_hook(&e, &secret, now, b"{\"n\":2}", None)
            .await
            .is_ok());
        assert!(unauth(
            call_hook(&e, &secret, now, b"{\"n\":2}", None).await
        ));
        assert!(matches!(
            e.rt.webhooks
                .handle("demo", "inbound", None, None, None, b"{}")
                .await,
            Err(WebhookError::Unauthorized)
        ));

        // Rotating invalidates the old secret.
        let new =
            e.rt.webhooks
                .rotate_secret("demo", "inbound")
                .await
                .unwrap();
        assert_ne!(new, secret);
        assert!(unauth(
            call_hook(&e, &secret, now, b"{\"n\":3}", None).await
        ));
    }

    #[tokio::test]
    async fn inbound_webhooks_are_rate_limited() {
        let e = env(PluginEventsConfig {
            webhook_rate_per_minute: 2,
            ..fast()
        })
        .await;
        let secret =
            e.rt.webhooks
                .rotate_secret("demo", "inbound")
                .await
                .unwrap();
        let now = chrono::Utc::now().timestamp();
        for n in 0..2 {
            call_hook(&e, &secret, now, format!("{{\"n\":{n}}}").as_bytes(), None)
                .await
                .unwrap();
        }
        assert!(matches!(
            call_hook(&e, &secret, now, b"{\"n\":9}", None).await,
            Err(WebhookError::RateLimited)
        ));
    }

    #[tokio::test]
    async fn outbound_webhooks_are_signed_and_retried() {
        use axum::{extract::State, http::HeaderMap, routing::post, Router};
        type Seen = Arc<Mutex<Vec<(HeaderMap, Vec<u8>)>>>;
        let seen: Seen = Default::default();
        let ok = Arc::new(AtomicBool::new(false));
        async fn hook(
            State((seen, ok)): State<(Seen, Arc<AtomicBool>)>,
            h: HeaderMap,
            b: axum::body::Bytes,
        ) -> axum::http::StatusCode {
            seen.lock().unwrap().push((h, b.to_vec()));
            if ok.load(Ordering::SeqCst) {
                axum::http::StatusCode::OK
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            }
        }
        let app = Router::new()
            .route("/h", post(hook))
            .with_state((seen.clone(), ok.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let e = env(fast()).await;
        let rec = crate::services::plugins::webhooks::endpoint_record(
            "crm".into(),
            format!("http://127.0.0.1:{port}/h"),
            vec!["node.*".into()],
            Some("certname == 'a'".into()),
            None,
        );
        let repo = PluginEventsRepository::new(&e.pool);
        repo.insert_endpoint(&rec).await.unwrap();
        let bus = &e.rt.events;
        bus.publish(
            "node.classified",
            json!({"certname": "b"}),
            EventActor::system(),
        )
        .await
        .unwrap();
        bus.publish(
            "node.classified",
            json!({"certname": "a"}),
            EventActor::system(),
        )
        .await
        .unwrap();
        e.rt.dispatcher.run_once().await.unwrap();
        // Failing endpoint: one attempt recorded, delivery queued for retry.
        assert_eq!(seen.lock().unwrap().len(), 1);
        ok.store(true, Ordering::SeqCst);
        e.rt.dispatcher.run_once().await.unwrap();
        assert_eq!(seen.lock().unwrap().len(), 2);

        let (h, body) = seen.lock().unwrap().last().cloned().unwrap();
        let ts = h["x-openvox-timestamp"].to_str().unwrap();
        let sig = h["x-openvox-signature"].to_str().unwrap();
        assert!(crate::services::plugins::webhooks::verify(
            &rec.secret,
            ts,
            &body,
            sig
        ));
        assert_eq!(h["x-openvox-event"], "node.classified");
        let env_json: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(env_json["payload"]["certname"], "a");
        let webhook_deliveries: Vec<_> = repo
            .list_deliveries(Some("delivered"), 10)
            .await
            .unwrap()
            .into_iter()
            .filter(|d| d.target_kind == "webhook")
            .collect();
        assert_eq!(webhook_deliveries.len(), 1);
        assert_eq!(webhook_deliveries[0].attempts, 2);
    }
}
