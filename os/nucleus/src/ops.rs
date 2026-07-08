//! The operator engine — command primitives that do not exist in any Unix.
//!
//! A shell's job is to reduce the friction between a human's intent and the
//! machine's state. Unix shells inherited a 1970s model where that friction is
//! enormous and self-inflicted: commands are imperative (you script *steps*,
//! not the *goal*), they half-succeed and leave rubble, they are irreversible
//! (one `rm` is forever), you cannot know what a command *will* do before it
//! does it, "why is it like this?" means archaeology through logs, and you
//! recompute the same checks forever. Praxis's substrate — transactional
//! intents, a journalled object store, causal ledgers — lets the shell delete
//! those problems outright rather than paper over them. This module is that
//! substrate exposed as commands:
//!
//!   * [`ensure`](OpsEngine::ensure) — declare the goal, not the steps. It is
//!     idempotent (re-running when it already holds is an instant, proven
//!     no-op) and transactional (it commits only once its postcondition is
//!     proven true), so scripts stop half-succeeding.
//!   * [`undo`](OpsEngine::undo) / [`redo`](OpsEngine::redo) — every mutation
//!     journals its pre-image, so the terminal becomes reversible. The single
//!     scariest property of every existing shell — irreversibility — is gone.
//!   * [`dry_run`](OpsEngine::dry_run) — preview a mutation with a guarantee it
//!     touched nothing, because it only *reads*. Not a best-effort `--dry-run`
//!     flag each tool reimplements and gets wrong; a kernel guarantee.
//!   * [`why_key`](OpsEngine::why_key) — the causal history of a value,
//!     answered from the journal, not reconstructed from logs.
//!   * [`prove`](OpsEngine::prove) / [`recall`](OpsEngine::recall) —
//!     proof-memory: a checked fact is cached with the tick it was proven, so
//!     the same check is never paid for twice.
//!   * [`arm`](OpsEngine::arm) / [`take_fired`](OpsEngine::take_fired) —
//!     reactive `when <predicate> do <command>` triggers, so you never write a
//!     poll-sleep loop again.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::fs::Vfs;
use crate::intent::{submit, Intent, ObjectStore, Op, Post};

/// A machine-checkable predicate over the object store and filesystem — the
/// shared grammar behind `prove`, `recall`, and `when`. Written as a single
/// token so it composes on a command line:
///
///   `kv:<key>=<value>`   the object-store key equals the value
///   `kv:<key>~<text>`    the key's value contains the text
///   `file:<path>`        the file exists
///   `file:<path>~<text>` the file exists and contains the text
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pred {
    KvEquals(String, String),
    KvContains(String, String),
    FileExists(String),
    FileContains(String, String),
}

impl Pred {
    /// Parse the single-token predicate grammar. Returns `None` on malformed
    /// input so the shell can print usage rather than guess.
    pub fn parse(token: &str) -> Option<Pred> {
        if let Some(rest) = token.strip_prefix("kv:") {
            if let Some((key, value)) = rest.split_once('=') {
                return Some(Pred::KvEquals(key.to_string(), value.to_string()));
            }
            if let Some((key, value)) = rest.split_once('~') {
                return Some(Pred::KvContains(key.to_string(), value.to_string()));
            }
            return None;
        }
        if let Some(rest) = token.strip_prefix("file:") {
            if let Some((path, text)) = rest.split_once('~') {
                return Some(Pred::FileContains(path.to_string(), text.to_string()));
            }
            return Some(Pred::FileExists(rest.to_string()));
        }
        None
    }

    /// Evaluate for real against the live object store and filesystem.
    pub fn eval(&self, store: &ObjectStore, fs: &Vfs) -> bool {
        match self {
            Pred::KvEquals(key, value) => store.get(key).map(|v| v == value).unwrap_or(false),
            Pred::KvContains(key, text) => store
                .get(key)
                .map(|v| v.contains(text.as_str()))
                .unwrap_or(false),
            Pred::FileExists(path) => fs.stat(path).is_ok(),
            Pred::FileContains(path, text) => fs
                .read_all(path)
                .ok()
                .and_then(|bytes| {
                    core::str::from_utf8(&bytes)
                        .ok()
                        .map(|s| s.contains(text.as_str()))
                })
                .unwrap_or(false),
        }
    }
}

/// One reversible mutation, holding both its pre-image (`prior`) and its
/// post-image (`next`) so undo and redo are symmetric.
#[derive(Debug, Clone)]
struct JournalEntry {
    key: String,
    prior: Option<String>,
    next: Option<String>,
    description: String,
    at_tick: u64,
}

/// A fact proven at a moment in time — proof-memory so it is never re-checked.
#[derive(Debug, Clone)]
pub struct Fact {
    pub predicate: String,
    pub holds: bool,
    pub proven_at_tick: u64,
    pub checks: u64,
}

/// A reactive trigger: fire `command` once when `predicate` becomes true.
#[derive(Debug, Clone)]
pub struct Trigger {
    pub id: u64,
    pub predicate: Pred,
    pub predicate_text: String,
    pub command: String,
    pub armed_at_tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnsureOutcome {
    /// The goal already held; nothing was done (an instant, proven no-op).
    AlreadyHeld,
    /// The goal did not hold and was established, committed under proof.
    Converged,
    /// The postcondition could not be proven; the world was left untouched.
    Failed,
}

#[derive(Debug, Clone)]
pub struct EnsureReport {
    pub outcome: EnsureOutcome,
    pub key: String,
    pub value: String,
    pub prior: Option<String>,
    pub detail: String,
}

/// What `dry` reports without touching anything.
#[derive(Debug, Clone)]
pub struct DryReport {
    pub would_change: bool,
    pub key: String,
    pub current: Option<String>,
    pub proposed: String,
}

/// A predicate string that did not parse (so the shell can print usage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadPredicate;

/// The operator engine. Owns the undo/redo journals, the proof-memory cache,
/// and the reactive triggers. Cheap; holds only bookkeeping.
pub struct OpsEngine {
    undo_stack: Vec<JournalEntry>,
    redo_stack: Vec<JournalEntry>,
    facts: BTreeMap<String, Fact>,
    triggers: Vec<Trigger>,
    next_trigger: u64,
    /// Guards the reactive pass against re-entrancy (a fired command must not
    /// recursively trigger another reactive pass).
    pub reacting: bool,
}

impl OpsEngine {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            facts: BTreeMap::new(),
            triggers: Vec::new(),
            next_trigger: 1,
            reacting: false,
        }
    }

    // ── ensure: declarative, idempotent, proven convergence ─────────────

    /// Make the object-store `key` equal `value`, and prove it. If it already
    /// holds, this is an instant no-op (the idempotency that makes re-running a
    /// script safe). Otherwise it commits transactionally — only once the
    /// postcondition `key == value` is proven on the shadow — and journals the
    /// pre-image so it can be undone.
    pub fn ensure(
        &mut self,
        store: &mut ObjectStore,
        key: &str,
        value: &str,
        tick: u64,
    ) -> EnsureReport {
        let prior = store.get(key).cloned();
        if prior.as_deref() == Some(value) {
            return EnsureReport {
                outcome: EnsureOutcome::AlreadyHeld,
                key: key.to_string(),
                value: value.to_string(),
                prior,
                detail: "goal already holds — instant no-op, nothing recomputed".to_string(),
            };
        }
        let report = submit(
            store,
            Intent {
                goal: alloc::format!("ensure {key} == {value}"),
                ops: alloc::vec![Op::Put {
                    key: key.to_string(),
                    value: value.to_string(),
                }],
                post: Post::KeyEquals {
                    key: key.to_string(),
                    value: value.to_string(),
                },
            },
        );
        if report.committed {
            self.undo_stack.push(JournalEntry {
                key: key.to_string(),
                prior: prior.clone(),
                next: Some(value.to_string()),
                description: alloc::format!("ensure {key}={value}"),
                at_tick: tick,
            });
            self.redo_stack.clear(); // a new action forks history
            EnsureReport {
                outcome: EnsureOutcome::Converged,
                key: key.to_string(),
                value: value.to_string(),
                prior,
                detail: "converged and proven; committed atomically".to_string(),
            }
        } else {
            EnsureReport {
                outcome: EnsureOutcome::Failed,
                key: key.to_string(),
                value: value.to_string(),
                prior,
                detail: report.detail,
            }
        }
    }

    // ── undo / redo: the reversible terminal ────────────────────────────

    /// Reverse the last journaled mutation, restoring the pre-image. Pushes the
    /// entry onto the redo stack so it can be replayed.
    pub fn undo(&mut self, store: &mut ObjectStore, tick: u64) -> Option<String> {
        let entry = self.undo_stack.pop()?;
        Self::apply(store, &entry.key, entry.prior.as_deref());
        let description = alloc::format!(
            "undid '{}': {} restored to {:?} (was {:?})",
            entry.description,
            entry.key,
            entry.prior,
            entry.next
        );
        self.redo_stack.push(JournalEntry {
            at_tick: tick,
            ..entry
        });
        Some(description)
    }

    /// Replay the last undone mutation, restoring the post-image.
    pub fn redo(&mut self, store: &mut ObjectStore, tick: u64) -> Option<String> {
        let entry = self.redo_stack.pop()?;
        Self::apply(store, &entry.key, entry.next.as_deref());
        let description = alloc::format!(
            "redid '{}': {} = {:?}",
            entry.description,
            entry.key,
            entry.next
        );
        self.undo_stack.push(JournalEntry {
            at_tick: tick,
            ..entry
        });
        Some(description)
    }

    /// Set or delete a key through a transactional intent (the shared write
    /// path for undo/redo). `None` deletes.
    fn apply(store: &mut ObjectStore, key: &str, value: Option<&str>) {
        let (ops, post) = match value {
            Some(v) => (
                alloc::vec![Op::Put {
                    key: key.to_string(),
                    value: v.to_string(),
                }],
                Post::KeyEquals {
                    key: key.to_string(),
                    value: v.to_string(),
                },
            ),
            None => (
                alloc::vec![Op::Delete {
                    key: key.to_string(),
                }],
                Post::KeyAbsent {
                    key: key.to_string(),
                },
            ),
        };
        submit(
            store,
            Intent {
                goal: alloc::format!("restore {key}"),
                ops,
                post,
            },
        );
    }

    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    // ── dry: preview with a zero-touch guarantee ────────────────────────

    /// Report what `ensure key=value` *would* do, reading only — so it cannot
    /// change the world. The guarantee is structural: this function has no
    /// write path.
    pub fn dry_run(&self, store: &ObjectStore, key: &str, value: &str) -> DryReport {
        let current = store.get(key).cloned();
        DryReport {
            would_change: current.as_deref() != Some(value),
            key: key.to_string(),
            current,
            proposed: value.to_string(),
        }
    }

    // ── why: causal history from the journal ────────────────────────────

    /// Explain a key's current value from the journal: who last set it, when,
    /// and what it was before. Not reconstructed from logs — recorded at the
    /// moment of the change.
    pub fn why_key(&self, key: &str) -> Option<String> {
        self.undo_stack
            .iter()
            .rev()
            .chain(self.redo_stack.iter().rev())
            .find(|e| e.key == key)
            .map(|e| {
                alloc::format!(
                    "'{key}' was set by `{}` at tick {} (previously {:?})",
                    e.description,
                    e.at_tick,
                    e.prior
                )
            })
    }

    // ── prove / recall: proof-memory ────────────────────────────────────

    /// Check a predicate for real and cache the verdict with the tick it was
    /// proven. `recheck` forces re-evaluation even if cached.
    pub fn prove(
        &mut self,
        predicate: &str,
        store: &ObjectStore,
        fs: &Vfs,
        tick: u64,
        recheck: bool,
    ) -> Result<(bool, bool), BadPredicate> {
        let pred = Pred::parse(predicate).ok_or(BadPredicate)?;
        if !recheck {
            if let Some(fact) = self.facts.get(predicate) {
                return Ok((fact.holds, true)); // cached — nothing recomputed
            }
        }
        let holds = pred.eval(store, fs);
        let entry = self.facts.entry(predicate.to_string()).or_insert(Fact {
            predicate: predicate.to_string(),
            holds,
            proven_at_tick: tick,
            checks: 0,
        });
        entry.holds = holds;
        entry.proven_at_tick = tick;
        entry.checks += 1;
        Ok((holds, false))
    }

    /// Return a cached fact without re-evaluating it.
    pub fn recall(&self, predicate: &str) -> Option<&Fact> {
        self.facts.get(predicate)
    }

    // ── when: reactive triggers ─────────────────────────────────────────

    /// Arm a one-shot trigger: when `predicate` next becomes true, `command`
    /// fires once. Returns the trigger id.
    pub fn arm(&mut self, predicate: &str, command: &str, tick: u64) -> Result<u64, BadPredicate> {
        let pred = Pred::parse(predicate).ok_or(BadPredicate)?;
        let id = self.next_trigger;
        self.next_trigger += 1;
        self.triggers.push(Trigger {
            id,
            predicate: pred,
            predicate_text: predicate.to_string(),
            command: command.to_string(),
            armed_at_tick: tick,
        });
        Ok(id)
    }

    /// Evaluate every armed trigger; return the command lines of those whose
    /// predicate now holds, removing them (one-shot). The shell runs the
    /// returned commands as a reactive cascade.
    pub fn take_fired(&mut self, store: &ObjectStore, fs: &Vfs) -> Vec<String> {
        let mut fired = Vec::new();
        let mut kept = Vec::new();
        for trigger in core::mem::take(&mut self.triggers) {
            if trigger.predicate.eval(store, fs) {
                fired.push(trigger.command);
            } else {
                kept.push(trigger);
            }
        }
        self.triggers = kept;
        fired
    }

    pub fn armed(&self) -> &[Trigger] {
        &self.triggers
    }
}

impl Default for OpsEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_and_fs() -> (ObjectStore, Vfs) {
        (ObjectStore::new(), Vfs::new())
    }

    #[test]
    fn ensure_is_idempotent_and_proven() {
        let (mut store, _fs) = store_and_fs();
        let mut ops = OpsEngine::new();

        // First run converges.
        let r1 = ops.ensure(&mut store, "state", "ready", 10);
        assert_eq!(r1.outcome, EnsureOutcome::Converged);
        assert_eq!(store.get("state").unwrap(), "ready");

        // Second run is an instant, proven no-op — the property Unix scripts
        // lack, and why re-running a converged script is safe here.
        let r2 = ops.ensure(&mut store, "state", "ready", 11);
        assert_eq!(r2.outcome, EnsureOutcome::AlreadyHeld);
        // No spurious journal entry was pushed for the no-op.
        assert_eq!(ops.undo_depth(), 1);
    }

    #[test]
    fn undo_and_redo_make_the_terminal_reversible() {
        let (mut store, _fs) = store_and_fs();
        let mut ops = OpsEngine::new();
        ops.ensure(&mut store, "cfg", "v1", 1);
        ops.ensure(&mut store, "cfg", "v2", 2);
        assert_eq!(store.get("cfg").unwrap(), "v2");

        // Step back in time.
        let msg = ops.undo(&mut store, 3).unwrap();
        assert!(msg.contains("restored"));
        assert_eq!(store.get("cfg").unwrap(), "v1");
        // Step back again → the key never existed before v1.
        ops.undo(&mut store, 4).unwrap();
        assert!(store.get("cfg").is_none());

        // Replay forward.
        ops.redo(&mut store, 5).unwrap();
        assert_eq!(store.get("cfg").unwrap(), "v1");
        assert_eq!(ops.redo_depth(), 1);
    }

    #[test]
    fn dry_run_previews_without_touching_anything() {
        let (mut store, _fs) = store_and_fs();
        let ops = OpsEngine::new();
        let preview = ops.dry_run(&store, "k", "want");
        assert!(preview.would_change);
        assert!(preview.current.is_none());
        // The store was genuinely untouched by the preview.
        assert!(store.get("k").is_none());
        let _ = &mut store; // no write happened
    }

    #[test]
    fn why_explains_a_values_causal_history() {
        let (mut store, _fs) = store_and_fs();
        let mut ops = OpsEngine::new();
        ops.ensure(&mut store, "port", "8080", 42);
        let why = ops.why_key("port").unwrap();
        assert!(why.contains("tick 42"));
        assert!(why.contains("port"));
        assert!(ops.why_key("nonexistent").is_none());
    }

    #[test]
    fn prove_caches_and_recall_never_recomputes() {
        let (mut store, fs) = store_and_fs();
        store_put(&mut store, "ready", "yes");
        let mut ops = OpsEngine::new();

        // First prove evaluates for real.
        let (holds, from_cache) = ops.prove("kv:ready=yes", &store, &fs, 100, false).unwrap();
        assert!(holds && !from_cache);
        // Recall returns the cached verdict — nothing recomputed.
        let (holds2, from_cache2) = ops.prove("kv:ready=yes", &store, &fs, 101, false).unwrap();
        assert!(holds2 && from_cache2);
        assert_eq!(
            ops.recall("kv:ready=yes").unwrap().checks,
            1,
            "checked once, recalled since"
        );

        // Malformed predicates are rejected.
        assert!(ops.prove("garbage", &store, &fs, 0, false).is_err());
    }

    #[test]
    fn when_triggers_fire_once_on_the_predicate() {
        let (mut store, fs) = store_and_fs();
        let mut ops = OpsEngine::new();
        ops.arm("kv:deploy=done", "log deployment finished", 1)
            .unwrap();

        // Predicate false → nothing fires.
        assert!(ops.take_fired(&store, &fs).is_empty());

        // The world changes; now it fires exactly once.
        store_put(&mut store, "deploy", "done");
        let fired = ops.take_fired(&store, &fs);
        assert_eq!(fired, alloc::vec!["log deployment finished".to_string()]);
        // One-shot: it does not fire again.
        assert!(ops.take_fired(&store, &fs).is_empty());
    }

    // Small helper: write straight into the store for predicate tests.
    fn store_put(store: &mut ObjectStore, key: &str, value: &str) {
        submit(
            store,
            Intent {
                goal: "seed".into(),
                ops: alloc::vec![Op::Put {
                    key: key.to_string(),
                    value: value.to_string(),
                }],
                post: Post::KeyEquals {
                    key: key.to_string(),
                    value: value.to_string(),
                },
            },
        );
    }
}
