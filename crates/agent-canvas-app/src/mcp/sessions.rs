use std::{collections::HashMap, sync::Arc};

use parking_lot::Mutex;
use rusqlite::{Connection, params};
use tokio::sync::mpsc;

use super::notifications::JsonRpcNotification;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSession {
    pub session_id: String,
    pub persona: String,
    pub agent: String,
    pub project: String,
    pub connected_at: i64,
}

#[derive(Clone)]
pub struct Subscription {
    pub artifact_updated: bool,
    pub artifact_focused: bool,
    pub tx: mpsc::UnboundedSender<JsonRpcNotification>,
}

#[derive(Clone, Default)]
pub struct SubscriptionRegistry {
    inner: Arc<Mutex<HashMap<String, Subscription>>>,
}

impl SubscriptionRegistry {
    pub fn register_default(
        &self,
        session_id: String,
        tx: mpsc::UnboundedSender<JsonRpcNotification>,
    ) {
        self.inner.lock().insert(
            session_id,
            Subscription {
                artifact_updated: true,
                artifact_focused: false,
                tx,
            },
        );
    }

    pub fn remove(&self, session_id: &str) {
        self.inner.lock().remove(session_id);
    }

    pub fn subscribe(&self, session_id: &str, artifact_updated: bool, artifact_focused: bool) {
        if let Some(subscription) = self.inner.lock().get_mut(session_id) {
            subscription.artifact_updated = artifact_updated;
            subscription.artifact_focused = artifact_focused;
        }
    }

    #[cfg(test)]
    pub fn get(&self, session_id: &str) -> Option<Subscription> {
        self.inner.lock().get(session_id).cloned()
    }

    pub fn dispatch_artifact_updated(&self, notification: JsonRpcNotification) -> usize {
        self.dispatch(notification, |subscription| subscription.artifact_updated)
    }

    pub fn dispatch_artifact_focused(&self, notification: JsonRpcNotification) -> usize {
        self.dispatch(notification, |subscription| subscription.artifact_focused)
    }

    pub fn dispatch_all(&self, notification: JsonRpcNotification) -> usize {
        self.dispatch(notification, |_| true)
    }

    fn dispatch(
        &self,
        notification: JsonRpcNotification,
        predicate: impl Fn(&Subscription) -> bool,
    ) -> usize {
        let targets = self
            .inner
            .lock()
            .iter()
            .filter_map(|(session_id, subscription)| {
                predicate(subscription).then(|| (session_id.clone(), subscription.tx.clone()))
            })
            .collect::<Vec<_>>();

        let mut sent = 0;
        let mut stale = Vec::new();
        for (session_id, tx) in targets {
            if tx.send(notification.clone()).is_ok() {
                sent += 1;
            } else {
                stale.push(session_id);
            }
        }
        if !stale.is_empty() {
            let mut subscriptions = self.inner.lock();
            for session_id in stale {
                subscriptions.remove(&session_id);
            }
        }
        sent
    }
}

pub const AGENT_SESSIONS_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS agent_sessions (
  session_id      TEXT NOT NULL,
  source          TEXT NOT NULL,
  persona         TEXT NOT NULL,
  agent           TEXT NOT NULL,
  project         TEXT NOT NULL,
  connected_at    INTEGER NOT NULL,
  disconnected_at INTEGER,
  PRIMARY KEY (session_id, connected_at)
);
"#;

pub const USER_MESSAGES_MIGRATION_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS user_messages (
  id           TEXT PRIMARY KEY,
  session_id   TEXT NOT NULL,
  path         TEXT NOT NULL,
  note         TEXT,
  action_verb  TEXT,
  created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_user_messages_session ON user_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_user_messages_created ON user_messages(created_at);
"#;

pub fn migrate_manual_agent_sessions_if_needed(conn: &Connection) -> Result<(), String> {
    let columns = table_columns(conn, "agent_sessions")?;
    if columns.iter().any(|column| column == "backbone") {
        conn.execute_batch(
            r#"
            ALTER TABLE agent_sessions RENAME TO manual_agent_sessions;
            "#,
        )
        .map_err(|error| error.to_string())?;
    }

    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS manual_agent_sessions (
          id TEXT PRIMARY KEY,
          persona TEXT NOT NULL,
          backbone TEXT NOT NULL,
          context TEXT,
          connected_at INTEGER NOT NULL,
          last_active INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

pub fn migrate_agent_sessions(conn: &Connection) -> Result<(), String> {
    migrate_manual_agent_sessions_if_needed(conn)?;
    conn.execute_batch(AGENT_SESSIONS_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

pub fn migrate_user_messages(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(USER_MESSAGES_MIGRATION_SQL)
        .map_err(|error| error.to_string())
}

pub fn insert_agent_session(
    conn: &Connection,
    session_id: &str,
    persona: &str,
    agent: &str,
    project: &str,
    connected_at: i64,
) -> Result<McpSession, String> {
    conn.execute(
        r#"
        INSERT INTO agent_sessions(session_id, source, persona, agent, project, connected_at)
        VALUES (?1, 'mcp', ?2, ?3, ?4, ?5)
        "#,
        params![session_id, persona, agent, project, connected_at],
    )
    .map_err(|error| error.to_string())?;

    Ok(McpSession {
        session_id: session_id.to_owned(),
        persona: persona.to_owned(),
        agent: agent.to_owned(),
        project: project.to_owned(),
        connected_at,
    })
}

pub fn disconnect_agent_session(
    conn: &Connection,
    session_id: &str,
    connected_at: i64,
    disconnected_at: i64,
) -> Result<(), String> {
    conn.execute(
        r#"
        UPDATE agent_sessions
        SET disconnected_at = ?3
        WHERE session_id = ?1 AND connected_at = ?2 AND disconnected_at IS NULL
        "#,
        params![session_id, connected_at, disconnected_at],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}
