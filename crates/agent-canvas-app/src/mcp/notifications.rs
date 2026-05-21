use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::sessions::SubscriptionRegistry;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_owned(),
            method: method.into(),
            params,
        }
    }

    pub fn artifact_updated(
        path: impl Into<String>,
        by: impl Into<String>,
        note: Option<String>,
        action_verb: Option<String>,
    ) -> Self {
        let mut params = serde_json::Map::new();
        params.insert("path".to_owned(), json!(path.into()));
        params.insert("by".to_owned(), json!(by.into()));
        if let Some(note) = note {
            params.insert("note".to_owned(), json!(note));
        }
        if let Some(action_verb) = action_verb {
            params.insert("action_verb".to_owned(), json!(action_verb));
        }
        Self::new("notifications/artifact_updated", Value::Object(params))
    }

    pub fn artifact_focused(path: impl Into<String>) -> Self {
        Self::new(
            "notifications/artifact_focused",
            json!({ "path": path.into() }),
        )
    }

    pub fn shutdown() -> Self {
        Self::new("notifications/shutdown", json!({}))
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({}))
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SubscribeRequest {
    pub artifact_updated: bool,
    pub artifact_focused: bool,
}

pub fn parse_subscribe_request(params: &Value) -> SubscribeRequest {
    let mut request = SubscribeRequest::default();
    let Some(events) = params.get("events").and_then(Value::as_array) else {
        return request;
    };
    for event in events.iter().filter_map(Value::as_str) {
        match event {
            "artifact_updated" => request.artifact_updated = true,
            "artifact_focused" => request.artifact_focused = true,
            _ => {}
        }
    }
    request
}

pub fn dispatch_artifact_updated(
    subscriptions: &SubscriptionRegistry,
    path: String,
    by: &str,
    note: Option<String>,
    action_verb: Option<String>,
) -> usize {
    let notification = JsonRpcNotification::artifact_updated(path, by, note, action_verb);
    subscriptions.dispatch_artifact_updated(notification)
}

pub fn dispatch_artifact_focused(subscriptions: &SubscriptionRegistry, path: String) -> usize {
    let notification = JsonRpcNotification::artifact_focused(path);
    subscriptions.dispatch_artifact_focused(notification)
}
