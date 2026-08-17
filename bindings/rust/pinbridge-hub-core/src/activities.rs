use serde::Serialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const CAPACITY: usize = 256;

#[derive(Clone, Debug, Serialize)]
pub struct Activity {
    pub operation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_operation_id: Option<String>,
    pub actor: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    pub started_at_ms: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at_ms: Option<String>,
    pub outcome: String,
    pub resource_refs: Value,
}

#[derive(Clone)]
pub struct Journal {
    inner: Arc<Mutex<Inner>>,
}
struct Inner {
    next: u64,
    rows: VecDeque<Activity>,
    capacity: usize,
}
#[derive(Clone)]
pub struct ActivityHandle {
    journal: Journal,
    id: String,
}

fn now_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

impl Default for Journal {
    fn default() -> Self {
        Self::new(CAPACITY)
    }
}
impl Journal {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            inner: Arc::new(Mutex::new(Inner {
                next: 1,
                rows: VecDeque::with_capacity(capacity),
                capacity,
            })),
        }
    }
    pub fn begin(
        &self,
        actor: impl Into<String>,
        action: impl Into<String>,
        purpose: Option<String>,
        parent: Option<String>,
    ) -> ActivityHandle {
        let mut i = self.inner.lock().expect("journal poisoned");
        let id = format!("op-{:016x}", i.next);
        i.next = i.next.wrapping_add(1).max(1);
        i.rows.push_back(Activity {
            operation_id: id.clone(),
            parent_operation_id: parent,
            actor: actor.into(),
            action: action.into(),
            purpose,
            started_at_ms: now_ms(),
            completed_at_ms: None,
            outcome: "in_progress".into(),
            resource_refs: serde_json::json!({}),
        });
        while i.rows.len() > i.capacity {
            i.rows.pop_front();
        }
        ActivityHandle {
            journal: self.clone(),
            id,
        }
    }
    pub fn list(&self, limit: usize) -> Vec<Activity> {
        let i = self.inner.lock().expect("journal poisoned");
        i.rows
            .iter()
            .rev()
            .take(limit.min(i.capacity))
            .cloned()
            .collect()
    }
    pub fn get(&self, id: &str) -> Option<Activity> {
        self.inner
            .lock()
            .expect("journal poisoned")
            .rows
            .iter()
            .find(|r| r.operation_id == id)
            .cloned()
    }
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
}
impl ActivityHandle {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn finish(self, outcome: impl Into<String>, refs: Value) {
        let mut i = self.journal.inner.lock().expect("journal poisoned");
        if let Some(row) = i.rows.iter_mut().find(|r| r.operation_id == self.id) {
            row.completed_at_ms = Some(now_ms());
            row.outcome = outcome.into();
            row.resource_refs = refs;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_string_ids() {
        let j = Journal::new(256);
        j.begin("ai", "x", None, None)
            .finish("ok", serde_json::json!({}));
        assert_eq!(j.list(1)[0].operation_id, "op-0000000000000001");
    }

    #[test]
    fn custom_capacity_evicts_oldest_rows() {
        let j = Journal::new(2);
        let first = j.begin("ai", "first", None, None);
        first.finish("ok", serde_json::json!({}));
        let second = j.begin("ai", "second", None, None);
        second.finish("ok", serde_json::json!({}));
        let third = j.begin("ai", "third", None, None);
        third.finish("ok", serde_json::json!({}));

        let rows = j.list(10);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].action, "third");
        assert_eq!(rows[1].action, "second");
        assert!(j.get("op-0000000000000001").is_none());
    }
}
