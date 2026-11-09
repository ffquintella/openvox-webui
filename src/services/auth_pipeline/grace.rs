//! Enrollment grace windows (OQ-12).
//!
//! A window lets a user with nothing enrolled sign in with the primary
//! factor to an enrollment-only session. It is granted by an administrator's
//! factor reset (or once per account by policy), has a fixed expiry, admits a
//! bounded number of logins and closes at the first enrolled factor. The user
//! can never open, extend or re-open one.

use anyhow::Result;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

pub const REASON_FACTOR_RESET: &str = "factor_reset";
pub const REASON_POLICY: &str = "policy";

pub const CLOSED_ENROLLED: &str = "enrolled";
pub const CLOSED_SUPERSEDED: &str = "superseded";
pub const CLOSED_EXPIRED: &str = "expired";
pub const CLOSED_EXHAUSTED: &str = "exhausted";

/// Enrollment-only logins one window admits.
pub const MAX_LOGINS: i64 = 10;
/// Longest life of one enrollment-only session (never past the window).
pub const SESSION_MINUTES: i64 = 15;

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GraceWindow {
    pub id: String,
    pub user_id: String,
    pub reason: String,
    pub granted_by: Option<String>,
    pub expires_at: String,
    pub logins: i64,
    pub closed_at: Option<String>,
    pub closed_reason: Option<String>,
    pub created_at: String,
}

impl GraceWindow {
    pub fn expiry(&self) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.expires_at)
            .ok()
            .map(|t| t.with_timezone(&Utc))
    }

    /// An unreadable expiry counts as expired.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expiry().is_none_or(|e| e <= now)
    }
}

/// Outcome of trying to spend one login of a window.
#[derive(Debug, PartialEq, Eq)]
pub enum LoginSpend {
    /// The login was counted; the window expires at this instant.
    Granted(DateTime<Utc>),
    Expired,
    Exhausted,
    /// Closed meanwhile (an enrollment, a newer reset).
    Closed,
}

pub struct GraceStore<'a> {
    pool: &'a SqlitePool,
}

impl<'a> GraceStore<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
    }

    /// The user's open window, expired or not.
    pub async fn open_window(&self, user: Uuid) -> Result<Option<GraceWindow>> {
        Ok(sqlx::query_as(
            "SELECT id, user_id, reason, granted_by, expires_at, logins, closed_at, closed_reason, \
             created_at FROM auth_enrollment_grace WHERE user_id = ? AND closed_at IS NULL",
        )
        .bind(user.to_string())
        .fetch_optional(self.pool)
        .await?)
    }

    pub async fn get(&self, id: &str) -> Result<Option<GraceWindow>> {
        Ok(sqlx::query_as(
            "SELECT id, user_id, reason, granted_by, expires_at, logins, closed_at, closed_reason, \
             created_at FROM auth_enrollment_grace WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(self.pool)
        .await?)
    }

    /// Grants a window after a factor reset. An open window is closed as
    /// `superseded` first, so each reset yields exactly one window. Returns
    /// the new window and the id of the one it replaced.
    pub async fn grant_after_reset(
        &self,
        user: Uuid,
        actor: Uuid,
        ttl: Duration,
    ) -> Result<(GraceWindow, Option<String>)> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let mut tx = self.pool.begin().await?;
        let superseded: Option<String> = sqlx::query_scalar(
            "SELECT id FROM auth_enrollment_grace WHERE user_id = ? AND closed_at IS NULL",
        )
        .bind(user.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE auth_enrollment_grace SET closed_at = ?, closed_reason = ? \
             WHERE user_id = ? AND closed_at IS NULL",
        )
        .bind(ts(now))
        .bind(CLOSED_SUPERSEDED)
        .bind(user.to_string())
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO auth_enrollment_grace (id, user_id, reason, granted_by, expires_at, created_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(user.to_string())
        .bind(REASON_FACTOR_RESET)
        .bind(actor.to_string())
        .bind(ts(now + ttl))
        .bind(ts(now))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        let window = self
            .get(&id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("grace window vanished"))?;
        Ok((window, superseded))
    }

    /// Grants the one policy window of an account that never had a window of
    /// any kind. Atomic: `None` otherwise, so an expired or used window is
    /// never followed by another one without an administrator.
    pub async fn grant_policy_once(
        &self,
        user: Uuid,
        ttl: Duration,
    ) -> Result<Option<GraceWindow>> {
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let inserted = sqlx::query(
            "INSERT INTO auth_enrollment_grace (id, user_id, reason, granted_by, expires_at, created_at) \
             SELECT ?, ?, ?, NULL, ?, ? WHERE NOT EXISTS ( \
               SELECT 1 FROM auth_enrollment_grace WHERE user_id = ?)",
        )
        .bind(&id)
        .bind(user.to_string())
        .bind(REASON_POLICY)
        .bind(ts(now + ttl))
        .bind(ts(now))
        .bind(user.to_string())
        .execute(self.pool)
        .await;
        match inserted {
            Ok(r) if r.rows_affected() == 1 => self.get(&id).await,
            Ok(_) => Ok(None),
            // A concurrent grant won the unique open-window index.
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Spends one login, compare-and-set: concurrent logins can neither
    /// exceed the cap nor use a window that just closed or expired.
    pub async fn spend_login(&self, window: &GraceWindow) -> Result<LoginSpend> {
        let now = Utc::now();
        let r = sqlx::query(
            "UPDATE auth_enrollment_grace SET logins = logins + 1 \
             WHERE id = ? AND closed_at IS NULL AND expires_at > ? AND logins < ?",
        )
        .bind(&window.id)
        .bind(ts(now))
        .bind(MAX_LOGINS)
        .execute(self.pool)
        .await?;
        if r.rows_affected() == 1 {
            return Ok(match window.expiry() {
                Some(e) => LoginSpend::Granted(e),
                None => LoginSpend::Expired,
            });
        }
        let Some(current) = self.get(&window.id).await? else {
            return Ok(LoginSpend::Closed);
        };
        Ok(if current.closed_at.is_some() {
            LoginSpend::Closed
        } else if current.is_expired(now) {
            LoginSpend::Expired
        } else {
            LoginSpend::Exhausted
        })
    }

    /// Closes a window if it is still open; true when this call closed it.
    pub async fn close(&self, id: &str, reason: &str) -> Result<bool> {
        let r = sqlx::query(
            "UPDATE auth_enrollment_grace SET closed_at = ?, closed_reason = ? \
             WHERE id = ? AND closed_at IS NULL",
        )
        .bind(ts(Utc::now()))
        .bind(reason)
        .bind(id)
        .execute(self.pool)
        .await?;
        Ok(r.rows_affected() == 1)
    }

    /// Closes the user's open window, if any, and returns it.
    pub async fn close_open(&self, user: Uuid, reason: &str) -> Result<Option<GraceWindow>> {
        let Some(w) = self.open_window(user).await? else {
            return Ok(None);
        };
        Ok(self.close(&w.id, reason).await?.then_some(w))
    }
}

/// Expiry of an enrollment-only session opened now under a window.
pub fn session_expiry(window_expiry: DateTime<Utc>, now: DateTime<Utc>) -> DateTime<Utc> {
    window_expiry.min(now + Duration::minutes(SESSION_MINUTES))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_user() -> (SqlitePool, Uuid) {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, username, email, password_hash, role) VALUES (?, 'u', 'u@x', 'x', 'viewer')",
        )
        .bind(id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        (pool, id)
    }

    #[tokio::test]
    async fn a_reset_supersedes_the_open_window() {
        let (pool, user) = pool_with_user().await;
        let s = GraceStore::new(&pool);
        let admin = Uuid::new_v4();
        let (first, none) = s
            .grant_after_reset(user, admin, Duration::hours(1))
            .await
            .unwrap();
        assert!(none.is_none());
        assert_eq!(first.reason, REASON_FACTOR_RESET);
        assert_eq!(
            first.granted_by.as_deref(),
            Some(admin.to_string().as_str())
        );
        let (second, replaced) = s
            .grant_after_reset(user, admin, Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(replaced.as_deref(), Some(first.id.as_str()));
        assert_eq!(s.open_window(user).await.unwrap().unwrap().id, second.id);
        let old = s.get(&first.id).await.unwrap().unwrap();
        assert_eq!(old.closed_reason.as_deref(), Some(CLOSED_SUPERSEDED));
    }

    #[tokio::test]
    async fn logins_are_capped_and_stop_at_expiry_or_closure() {
        let (pool, user) = pool_with_user().await;
        let s = GraceStore::new(&pool);
        let (w, _) = s
            .grant_after_reset(user, Uuid::new_v4(), Duration::hours(1))
            .await
            .unwrap();
        for _ in 0..MAX_LOGINS {
            assert!(matches!(
                s.spend_login(&w).await.unwrap(),
                LoginSpend::Granted(_)
            ));
        }
        assert_eq!(s.spend_login(&w).await.unwrap(), LoginSpend::Exhausted);

        let (w, _) = s
            .grant_after_reset(user, Uuid::new_v4(), Duration::hours(1))
            .await
            .unwrap();
        sqlx::query(
            "UPDATE auth_enrollment_grace SET expires_at = '2000-01-01T00:00:00.000Z' WHERE id = ?",
        )
        .bind(&w.id)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(s.spend_login(&w).await.unwrap(), LoginSpend::Expired);

        let (w, _) = s
            .grant_after_reset(user, Uuid::new_v4(), Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(
            s.close_open(user, CLOSED_ENROLLED)
                .await
                .unwrap()
                .unwrap()
                .id,
            w.id
        );
        assert_eq!(s.spend_login(&w).await.unwrap(), LoginSpend::Closed);
        assert!(
            !s.close(&w.id, CLOSED_EXPIRED).await.unwrap(),
            "closed once"
        );
    }

    #[tokio::test]
    async fn the_policy_window_is_granted_once_per_account() {
        let (pool, user) = pool_with_user().await;
        let s = GraceStore::new(&pool);
        let w = s
            .grant_policy_once(user, Duration::hours(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(w.reason, REASON_POLICY);
        assert!(w.granted_by.is_none());
        assert!(s
            .grant_policy_once(user, Duration::hours(1))
            .await
            .unwrap()
            .is_none());
        s.close(&w.id, CLOSED_EXPIRED).await.unwrap();
        assert!(
            s.grant_policy_once(user, Duration::hours(1))
                .await
                .unwrap()
                .is_none(),
            "never re-armed"
        );
    }

    #[tokio::test]
    async fn no_policy_window_for_an_account_that_had_a_reset_window() {
        let (pool, user) = pool_with_user().await;
        let s = GraceStore::new(&pool);
        let (w, _) = s
            .grant_after_reset(user, Uuid::new_v4(), Duration::hours(1))
            .await
            .unwrap();
        assert!(s
            .grant_policy_once(user, Duration::hours(1))
            .await
            .unwrap()
            .is_none());
        s.close(&w.id, CLOSED_EXPIRED).await.unwrap();
        assert!(
            s.grant_policy_once(user, Duration::hours(1))
                .await
                .unwrap()
                .is_none(),
            "an expired reset window is not followed by a policy window"
        );
    }

    #[test]
    fn a_session_never_outlives_its_window() {
        let now = Utc::now();
        assert_eq!(
            session_expiry(now + Duration::hours(2), now),
            now + Duration::minutes(SESSION_MINUTES)
        );
        assert_eq!(
            session_expiry(now + Duration::minutes(3), now),
            now + Duration::minutes(3)
        );
    }
}
