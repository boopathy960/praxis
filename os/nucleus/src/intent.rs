//! Transactional intent syscalls — the syscall interface, inverted.
//!
//! A Linux syscall is an *instruction*: do this one opaque thing, trap, return.
//! The kernel cannot optimize across instructions because it never sees the
//! goal. An Praxis intent is a *goal*: a batch of operations plus a
//! machine-checkable **postcondition** that defines success. Because the
//! kernel sees the whole batch and the goal, it can do what Linux never may:
//!
//!   * **Fuse** the batch — dead-store elimination, append coalescing — the
//!     caller stated the goal, so any op sequence reaching it is legal.
//!   * **Execute transactionally** — ops apply to a shadow of the object
//!     store; the postcondition is checked *before* commit. If reality does
//!     not match the goal, the world is untouched and the caller learns why.
//!     No syscall ever half-happens.
//!
//! `io_uring` batches to save traps; this batches to *prove*. The postcondition
//! is the same `Check` idea the proof economy runs on — one verification
//! grammar from the market all the way down into the kernel.

use alloc::borrow::ToOwned;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The kernel object store the demo intents act on (Phase-2 seed: the same
/// shape later fronts files, sockets, and device state).
#[derive(Default, Clone)]
pub struct ObjectStore {
    map: BTreeMap<String, String>,
}

impl ObjectStore {
    pub const fn new() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.map.get(key)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Put { key: String, value: String },
    Append { key: String, value: String },
    Delete { key: String },
}

impl Op {
    fn key(&self) -> &str {
        match self {
            Op::Put { key, .. } | Op::Append { key, .. } | Op::Delete { key } => key,
        }
    }
}

/// The postcondition grammar — the in-kernel `Check`.
#[derive(Debug, Clone)]
pub enum Post {
    KeyEquals { key: String, value: String },
    KeyContains { key: String, value: String },
    KeyAbsent { key: String },
    All(Vec<Post>),
}

impl Post {
    fn holds(&self, store: &ObjectStore) -> bool {
        match self {
            Post::KeyEquals { key, value } => store.get(key).map(|v| v == value).unwrap_or(false),
            Post::KeyContains { key, value } => store
                .get(key)
                .map(|v| v.contains(value.as_str()))
                .unwrap_or(false),
            Post::KeyAbsent { key } => store.get(key).is_none(),
            Post::All(posts) => posts.iter().all(|p| p.holds(store)),
        }
    }
}

pub struct Intent {
    pub goal: String,
    pub ops: Vec<Op>,
    pub post: Post,
}

#[derive(Debug)]
pub struct IntentReport {
    pub goal: String,
    pub ops_submitted: usize,
    /// Ops actually executed after fusion — the optimization the kernel may
    /// perform *because* it saw the goal.
    pub ops_executed: usize,
    pub committed: bool,
    pub detail: String,
}

/// Fuse a batch: later `Put`/`Delete` on a key kills every earlier op on it
/// (dead-store elimination); consecutive effects on one key collapse into one.
/// Cross-key order is preserved by final-op position, which is sound here
/// because ops on distinct keys commute in the object store.
fn fuse(ops: &[Op]) -> Vec<Op> {
    // Left-fold each key to its net effect.
    let mut net: BTreeMap<String, Op> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for op in ops {
        let key = op.key().to_owned();
        let fused = match (net.remove(&key), op) {
            // A Put or Delete supersedes anything before it.
            (_, Op::Put { .. }) | (_, Op::Delete { .. }) => op.clone(),
            // Append onto a prior Put folds into the Put.
            (Some(Op::Put { value: mut v, .. }), Op::Append { value, .. }) => {
                v.push_str(value);
                Op::Put {
                    key: key.clone(),
                    value: v,
                }
            }
            // Append after Delete = Put of just the appended text.
            (Some(Op::Delete { .. }), Op::Append { value, .. }) => Op::Put {
                key: key.clone(),
                value: value.clone(),
            },
            // Append onto Append coalesces.
            (Some(Op::Append { value: mut v, .. }), Op::Append { value, .. }) => {
                v.push_str(value);
                Op::Append {
                    key: key.clone(),
                    value: v,
                }
            }
            (None, Op::Append { .. }) => op.clone(),
        };
        if !order.contains(&key) {
            order.push(key.clone());
        }
        net.insert(key, fused);
    }
    order.into_iter().filter_map(|k| net.remove(&k)).collect()
}

fn apply(store: &mut ObjectStore, op: &Op) {
    match op {
        Op::Put { key, value } => {
            store.map.insert(key.clone(), value.clone());
        }
        Op::Append { key, value } => {
            store.map.entry(key.clone()).or_default().push_str(value);
        }
        Op::Delete { key } => {
            store.map.remove(key);
        }
    }
}

/// Submit an intent: fuse, apply to a shadow, prove the postcondition, then —
/// and only then — commit. A failed goal leaves the world untouched.
pub fn submit(store: &mut ObjectStore, intent: Intent) -> IntentReport {
    let fused = fuse(&intent.ops);
    let mut shadow = store.clone();
    for op in &fused {
        apply(&mut shadow, op);
    }
    let committed = intent.post.holds(&shadow);
    let detail = if committed {
        *store = shadow;
        String::from("postcondition proven; committed")
    } else {
        String::from("postcondition failed on shadow; rolled back — world untouched")
    };
    IntentReport {
        goal: intent.goal,
        ops_submitted: intent.ops.len(),
        ops_executed: fused.len(),
        committed,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn put(key: &str, value: &str) -> Op {
        Op::Put {
            key: key.to_string(),
            value: value.to_string(),
        }
    }
    fn append(key: &str, value: &str) -> Op {
        Op::Append {
            key: key.to_string(),
            value: value.to_string(),
        }
    }

    #[test]
    fn fusion_eliminates_dead_stores_and_coalesces_appends() {
        let ops = [
            put("log", "a"),
            append("log", "b"),
            append("log", "c"),
            put("tmp", "x"),
            Op::Delete {
                key: "tmp".to_string(),
            },
            put("cfg", "old"),
            put("cfg", "new"),
        ];
        let fused = fuse(&ops);
        // log: one Put("abc"); tmp: one Delete; cfg: one Put("new").
        assert_eq!(fused.len(), 3);
        assert!(fused.contains(&put("log", "abc")));
        assert!(fused.contains(&put("cfg", "new")));
    }

    #[test]
    fn intent_commits_when_postcondition_proven() {
        let mut store = ObjectStore::new();
        let report = submit(
            &mut store,
            Intent {
                goal: "write config".into(),
                ops: alloc::vec![put("cfg", "v1"), append("cfg", "+patch")],
                post: Post::KeyEquals {
                    key: "cfg".into(),
                    value: "v1+patch".into(),
                },
            },
        );
        assert!(report.committed);
        assert_eq!(report.ops_submitted, 2);
        assert_eq!(report.ops_executed, 1, "fused into a single Put");
        assert_eq!(store.get("cfg").unwrap(), "v1+patch");
    }

    #[test]
    fn failed_postcondition_rolls_back_the_whole_intent() {
        let mut store = ObjectStore::new();
        submit(
            &mut store,
            Intent {
                goal: "seed".into(),
                ops: alloc::vec![put("cfg", "stable")],
                post: Post::KeyEquals {
                    key: "cfg".into(),
                    value: "stable".into(),
                },
            },
        );
        let report = submit(
            &mut store,
            Intent {
                goal: "botched upgrade".into(),
                ops: alloc::vec![put("cfg", "broken"), put("other", "side-effect")],
                post: Post::KeyEquals {
                    key: "cfg".into(),
                    value: "expected-something-else".into(),
                },
            },
        );
        assert!(!report.committed);
        // NOTHING from the failed intent landed — no half-applied syscalls.
        assert_eq!(store.get("cfg").unwrap(), "stable");
        assert!(store.get("other").is_none());
    }
}
