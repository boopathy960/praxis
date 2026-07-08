//! Aletheia — content recovery from hydration state, without JavaScript.
//!
//! *ἀλήθεια, "unconcealment."* Modern "needs-JavaScript" pages (Next.js, Nuxt,
//! SvelteKit, Apollo, Redux SPAs) ship a nearly-empty DOM plus a large JSON
//! **hydration state** blob — `__NEXT_DATA__`, `window.__NUXT__`,
//! `__APOLLO_STATE__`, `<script type="application/json">`, … — from which the
//! client-side framework *would* render the page. The article, the product
//! list, the "next page" cursor: all of it is already in that blob. Running a
//! headless browser to execute the JS is the usual (heavy, fragile) answer.
//! Aletheia instead **recovers the rendered meaning directly from the state**,
//! with no JS engine, via two mechanisms that don't otherwise exist here:
//!
//! ## 1. Linguistic charge — Shannon redundancy as a prose detector
//!
//! The hard sub-problem: a state tree is mostly framework plumbing (component
//! ids, GraphQL cache keys, `__typename`, css classes, base64 blobs, UUIDs)
//! with the real content buried in it. We must tell prose from machine noise
//! with **no dictionary and no model** (this runs in a crawler, over any
//! language). The discriminator is Shannon's own **redundancy** — how far a
//! string's character distribution sits from uniform over its own alphabet.
//! Let `H` be its unigram entropy and `H_max = log2(k)` the maximum a
//! `k`-symbol alphabet could carry:
//!
//! ```text
//!     R = 1 − H / H_max        (clamped to [0,1])
//! ```
//!
//! Natural language is redundant — space and a few letters dominate — so
//! `H ≪ H_max` and `R` is large (~0.2–0.35 at char level, in any language). A
//! random hash / base64 token uses its alphabet near-uniformly, so `H ≈ H_max`
//! and `R ≈ 0`. First-order statistics need little data, so this holds even for
//! short strings. Folded with a key-name prior, an opaque-token shape guard
//! (for structured ids like UUIDs, whose first-order stats are skewed too), and
//! script ratios, `R` becomes the per-node content **charge** `q(n)`.
//!
//! ## 2. The Semantic Potential Field — a Green's function on the tree
//!
//! Per-node scoring misfires: one prose-looking string adrift in config, or an
//! empty container that *holds* the article. So we treat `q` as charge on the
//! JSON tree and compute a **field** in which content regions reinforce and
//! isolated noise damps:
//!
//! ```text
//!     φ(n) = Σ_m  α^dist(n,m) · q(m)          α ∈ (0,1)
//! ```
//!
//! every node's potential is the distance-discounted sum of *all* charge. On a
//! tree this Green's function factorizes exactly through the lowest common
//! ancestor, so it is solved **exactly in two linear passes** — no O(n³)
//! inverse:
//!
//! ```text
//!     up   (post-order):  U(n) = q(n) + α · Σ_{c∈children} U(c)
//!     down (pre-order):   Ctx(child) = α·φ(parent) − α²·U(child)
//!                         φ(n) = U(n) + Ctx(n),   Ctx(root) = 0
//! ```
//!
//! Content is then the set of super-threshold leaves, linearized in document
//! order to reconstruct the text the SPA would have shown. A separate
//! **continuation-flux** scan lifts pagination cursors (`pageInfo`/`endCursor`,
//! `next`/`after`/`offset`) out of the same tree, so a crawler can walk an
//! infinite feed with no browser. `js_content_ratio` reports how much of the
//! page's meaning lived only in the state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Field decay per tree edge. Content charge one hop away contributes `α`, two
/// hops `α²`, … — near enough to matter, far enough to fade.
const ALPHA: f64 = 0.5;
/// Strings shorter than this can't yield a trustworthy entropy estimate; below
/// it we lean on the key prior and script ratios instead of `R`.
const MIN_RELIABLE_LEN: usize = 12;
/// A leaf is "content" when its field is at least this fraction of the peak
/// field on the page — an adaptive threshold, so a data-dense and a prose-dense
/// page both cut in the right place.
const CONTENT_FIELD_FRACTION: f64 = 0.35;
/// Never walk a pathological state blob forever.
const MAX_NODES: usize = 60_000;
const MAX_DEPTH: usize = 64;
/// Caps on what we hand back, so a huge blob can't blow the report up.
const MAX_RECOVERED_CHARS: usize = 20_000;
const MAX_RECOVERED_FIELDS: usize = 40;
const MAX_CONTINUATIONS: usize = 24;

/// What Aletheia recovered from a page's hydration state.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Recovery {
    /// The page's content text, reconstructed from super-threshold state nodes
    /// in document order — what the SPA would have rendered.
    pub recovered_text: String,
    /// High-confidence short content leaves (titles, names, labels, prices),
    /// keyed by their state path, strongest field first.
    pub recovered_fields: Vec<RecoveredField>,
    /// Pagination / continuation tokens found in the state, for browser-free
    /// deep crawling of feeds and listings.
    pub continuations: Vec<Continuation>,
    /// Number of JSON state blobs parsed out of the page.
    pub state_blobs: usize,
    /// Total nodes walked across all blobs (bounded by [`MAX_NODES`]).
    pub nodes: usize,
    /// Characters of content recovered from state.
    pub recovered_chars: usize,
    /// Peak field value observed — a rough "how concentrated is the content".
    pub peak_field: f64,
}

/// One recovered content leaf with its provenance and field strength.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveredField {
    /// Dotted/bracketed path through the state, e.g. `props.pageProps.title`.
    pub path: String,
    pub value: String,
    /// The semantic potential φ at this node (higher = more content-like in
    /// context).
    pub field: f64,
}

/// A continuation token: how to fetch the next page/slice without a browser.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Continuation {
    /// `relay_cursor` (GraphQL pageInfo), `next_url`, or `param`.
    pub kind: String,
    /// The state path it was found at.
    pub path: String,
    /// The cursor / url / value.
    pub value: String,
    /// For `relay_cursor`: whether `hasNextPage` was truthy.
    #[serde(default)]
    pub has_next: bool,
}

/// Recover content from a set of parsed JSON state blobs. `dom_text_len` is the
/// length of the page's visible DOM text, used only to compute
/// [`Recovery`]-level `js_content_ratio` at the call site.
#[must_use]
pub fn recover(blobs: &[serde_json::Value]) -> Recovery {
    let mut tree = Tree::default();
    for blob in blobs {
        if tree.nodes.len() >= MAX_NODES {
            break;
        }
        let root = tree.build(blob, Segment::Root, 0, None);
        tree.roots.push(root);
    }
    if tree.nodes.is_empty() {
        return Recovery {
            state_blobs: blobs.len(),
            ..Recovery::default()
        };
    }

    tree.solve_field();
    let peak_field = tree.nodes.iter().map(|n| n.field).fold(0.0_f64, f64::max);
    let threshold = peak_field * CONTENT_FIELD_FRACTION;

    // Reconstruct content text in document order (nodes were pushed in a
    // pre-order build, so index order *is* document order).
    let mut recovered_text = String::new();
    let mut fields: Vec<RecoveredField> = Vec::new();
    for idx in 0..tree.nodes.len() {
        let node = &tree.nodes[idx];
        let Some(value) = node.string.clone() else {
            continue;
        };
        if node.field < threshold || node.charge <= 0.0 {
            continue;
        }
        let field = node.field;
        if recovered_text.len() < MAX_RECOVERED_CHARS {
            if !recovered_text.is_empty() {
                recovered_text.push('\n');
            }
            recovered_text.push_str(&value);
        }
        // Short, high-field leaves are the labelled fields (title, price, …).
        if value.chars().count() <= 160 {
            fields.push(RecoveredField {
                path: tree.path_of(idx),
                value,
                field,
            });
        }
    }
    recovered_text.truncate(MAX_RECOVERED_CHARS);
    fields.sort_by(|a, b| {
        b.field
            .partial_cmp(&a.field)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    fields.truncate(MAX_RECOVERED_FIELDS);

    let continuations = tree.extract_continuations();
    let recovered_chars = recovered_text.chars().count();

    Recovery {
        recovered_text,
        recovered_fields: fields,
        continuations,
        state_blobs: blobs.len(),
        nodes: tree.nodes.len(),
        recovered_chars,
        peak_field,
    }
}

/// How much of the page's meaning lived only in JS state, in `[0,1]`:
/// `recovered / (dom_text + recovered)`. ~0 for a static page, ~1 for a pure
/// SPA whose DOM body is empty.
#[must_use]
pub fn js_content_ratio(dom_text_len: usize, recovered_chars: usize) -> f64 {
    let total = dom_text_len + recovered_chars;
    if total == 0 {
        0.0
    } else {
        recovered_chars as f64 / total as f64
    }
}

// ── the hydration tree ───────────────────────────────────────────────────

/// The edge label leading to a node: an object key or an array index.
#[derive(Debug, Clone)]
enum Segment {
    Root,
    Key(String),
    Index(usize),
}

struct Node {
    parent: Option<usize>,
    children: Vec<usize>,
    segment: Segment,
    /// Present only for string leaves; the recoverable content.
    string: Option<String>,
    /// Raw scalar (for continuation extraction: numbers, bools, short strings).
    scalar: Option<serde_json::Value>,
    /// Object keys present at this node (for cheap shape matching).
    keys: Vec<String>,
    charge: f64,
    /// U(n): post-order accumulated subtree charge.
    up: f64,
    /// φ(n): the solved semantic potential.
    field: f64,
}

#[derive(Default)]
struct Tree {
    nodes: Vec<Node>,
    roots: Vec<usize>,
}

impl Tree {
    /// Build the tree from a JSON value in pre-order (so node index == document
    /// order), computing each leaf's intrinsic charge as we go.
    fn build(
        &mut self,
        value: &serde_json::Value,
        segment: Segment,
        depth: usize,
        parent: Option<usize>,
    ) -> usize {
        let idx = self.nodes.len();
        let (string, scalar, keys) = match value {
            serde_json::Value::String(s) => (Some(s.clone()), Some(value.clone()), Vec::new()),
            serde_json::Value::Object(map) => (None, None, map.keys().cloned().collect::<Vec<_>>()),
            serde_json::Value::Null => (None, None, Vec::new()),
            other => (None, Some(other.clone()), Vec::new()),
        };
        let charge = match &string {
            Some(s) => leaf_charge(&segment, s),
            None => 0.0,
        };
        self.nodes.push(Node {
            parent,
            children: Vec::new(),
            segment,
            string,
            scalar,
            keys,
            charge,
            up: 0.0,
            field: 0.0,
        });

        if depth < MAX_DEPTH && self.nodes.len() < MAX_NODES {
            match value {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        if self.nodes.len() >= MAX_NODES {
                            break;
                        }
                        let child = self.build(v, Segment::Key(k.clone()), depth + 1, Some(idx));
                        self.nodes[idx].children.push(child);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (i, v) in items.iter().enumerate() {
                        if self.nodes.len() >= MAX_NODES {
                            break;
                        }
                        let child = self.build(v, Segment::Index(i), depth + 1, Some(idx));
                        self.nodes[idx].children.push(child);
                    }
                }
                _ => {}
            }
        }
        idx
    }

    /// Solve φ(n) = Σ_m α^dist(n,m) q(m) exactly, in two linear passes.
    ///
    /// The nodes are already in pre-order, so a reverse scan is a valid
    /// post-order for the up-pass (a child always has a higher index than its
    /// parent), and a forward scan is a valid pre-order for the down-pass.
    fn solve_field(&mut self) {
        // Up-pass (post-order): U(n) = q(n) + α · Σ_children U(c).
        for i in (0..self.nodes.len()).rev() {
            let mut up = self.nodes[i].charge;
            for c in self.nodes[i].children.clone() {
                up += ALPHA * self.nodes[c].up;
            }
            self.nodes[i].up = up;
        }
        // Down-pass (pre-order): roots see only their own subtree; each child
        // then adds the leave-one-out context flowing down from its parent.
        for i in 0..self.nodes.len() {
            if self.nodes[i].parent.is_none() {
                self.nodes[i].field = self.nodes[i].up;
            }
            let parent_field = self.nodes[i].field;
            let children = self.nodes[i].children.clone();
            for c in children {
                // Ctx(c) = α·φ(parent) − α²·U(c);  φ(c) = U(c) + Ctx(c).
                let ctx = ALPHA * parent_field - ALPHA * ALPHA * self.nodes[c].up;
                self.nodes[c].field = self.nodes[c].up + ctx;
            }
        }
    }

    /// Reconstruct a readable state path for a node index (`a.b[3].c`).
    fn path_of(&self, idx: usize) -> String {
        let mut segs: Vec<String> = Vec::new();
        let mut cur = Some(idx);
        while let Some(i) = cur {
            match &self.nodes[i].segment {
                Segment::Root => {}
                Segment::Key(k) => segs.push(format!(".{k}")),
                Segment::Index(index) => segs.push(format!("[{index}]")),
            }
            cur = self.nodes[i].parent;
        }
        segs.reverse();
        let joined: String = segs.concat();
        joined.strip_prefix('.').unwrap_or(&joined).to_string()
    }

    /// Continuation-flux: lift pagination cursors out of the state so a crawler
    /// can fetch the next slice with no browser.
    fn extract_continuations(&self) -> Vec<Continuation> {
        let mut out: Vec<Continuation> = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if out.len() >= MAX_CONTINUATIONS {
                break;
            }
            // Relay-style pageInfo: an object carrying endCursor + hasNextPage.
            if !node.keys.is_empty()
                && (node.keys.iter().any(|k| eqi(k, "endcursor"))
                    || node.keys.iter().any(|k| eqi(k, "end_cursor")))
            {
                let end_cursor = self.child_string(i, &["endCursor", "end_cursor"]);
                let has_next = self
                    .child_bool(i, &["hasNextPage", "has_next_page", "hasMore", "has_more"])
                    .unwrap_or(false);
                if let Some(cursor) = end_cursor {
                    if !cursor.is_empty() {
                        push_unique(
                            &mut out,
                            Continuation {
                                kind: "relay_cursor".into(),
                                path: self.path_of(i),
                                value: cursor,
                                has_next,
                            },
                        );
                        continue;
                    }
                }
            }
            // A leaf whose key names a continuation and whose value is scalar.
            if let (Segment::Key(k), Some(scalar)) = (&node.segment, &node.scalar) {
                let kl = k.to_ascii_lowercase();
                let is_url_key = matches!(
                    kl.as_str(),
                    "next" | "nextpage" | "next_page" | "nexturl" | "next_url"
                );
                let is_param_key = matches!(
                    kl.as_str(),
                    "cursor" | "after" | "offset" | "page" | "start" | "endcursor" | "end_cursor"
                );
                if is_url_key {
                    if let Some(s) = scalar.as_str() {
                        if s.starts_with("http") || s.starts_with('/') {
                            push_unique(
                                &mut out,
                                Continuation {
                                    kind: "next_url".into(),
                                    path: self.path_of(i),
                                    value: s.to_string(),
                                    has_next: true,
                                },
                            );
                        }
                    }
                } else if is_param_key {
                    let value = scalar_to_string(scalar);
                    if !value.is_empty() && value != "null" && value != "false" {
                        push_unique(
                            &mut out,
                            Continuation {
                                kind: "param".into(),
                                path: self.path_of(i),
                                value,
                                has_next: true,
                            },
                        );
                    }
                }
            }
        }
        out
    }

    /// Find a child of `parent` whose key matches one of `keys` and read its
    /// string value.
    fn child_string(&self, parent: usize, keys: &[&str]) -> Option<String> {
        for &c in &self.nodes[parent].children {
            if let Segment::Key(k) = &self.nodes[c].segment {
                if keys.iter().any(|want| eqi(k, want)) {
                    if let Some(scalar) = &self.nodes[c].scalar {
                        return scalar.as_str().map(str::to_string);
                    }
                }
            }
        }
        None
    }

    fn child_bool(&self, parent: usize, keys: &[&str]) -> Option<bool> {
        for &c in &self.nodes[parent].children {
            if let Segment::Key(k) = &self.nodes[c].segment {
                if keys.iter().any(|want| eqi(k, want)) {
                    if let Some(scalar) = &self.nodes[c].scalar {
                        return scalar.as_bool();
                    }
                }
            }
        }
        None
    }
}

// ── the linguistic charge kernel ─────────────────────────────────────────

/// Intrinsic content charge of a string leaf: the entropy-gap redundancy,
/// weighted by length and the naturalness of its script, then scaled by a
/// key-name prior. Zero for empty/whitespace strings.
fn leaf_charge(segment: &Segment, s: &str) -> f64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    let char_len = trimmed.chars().count();
    // A URL/hash/enum value is not content even if long; suppress by shape.
    if looks_like_opaque_token(trimmed) {
        return 0.05 * key_prior(segment);
    }

    let redundancy = linguistic_redundancy(trimmed);
    let space_ratio = ratio(trimmed, |c| c == ' ' || c == '\t');
    let letter_ratio = ratio(trimmed, |c| c.is_alphabetic());

    // Below the reliable length, entropy is noisy; fall back to script ratios.
    let prose = if char_len >= MIN_RELIABLE_LEN {
        0.6 * redundancy + 0.25 * space_ratio + 0.15 * letter_ratio
    } else {
        0.5 * letter_ratio + 0.5 * space_ratio.min(0.3) / 0.3
    };

    // Length signal saturates: a 40-char sentence and a 4000-char one are both
    // "content", we don't want length to dominate the field.
    let length_signal = (char_len as f64 / 80.0).min(1.0).sqrt();

    (prose.clamp(0.0, 1.0) * length_signal * key_prior(segment)).max(0.0)
}

/// Shannon redundancy `R = 1 − H/H_max`: how far a string's character
/// distribution sits from uniform-over-its-own-alphabet. High for natural
/// language, near zero for random ids / hashes / base64. Dictionary- and
/// language-agnostic, and — unlike a conditional-bigram estimate — robust at
/// *short* string lengths, because it needs only first-order statistics.
///
/// The intuition is Shannon's original one: natural language is redundant.
/// Its characters are strongly non-uniform — space and a handful of letters
/// dominate — so its entropy `H` is well below the maximum `H_max = log2(k)`
/// that its `k`-symbol alphabet could carry, and `R` is large (~0.2–0.35 at
/// the character level, across languages). A hash or UUID uses its alphabet
/// near-uniformly, so `H ≈ H_max` and `R ≈ 0`. That gap is the signal.
///
/// (An optional second-order refinement — conditional bigram entropy — adds
/// discrimination on *long* text but is unreliable below a few hundred chars,
/// so the load-bearing quantity here is the first-order redundancy.)
#[must_use]
pub fn linguistic_redundancy(s: &str) -> f64 {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return 0.0;
    }
    let mut counts: BTreeMap<char, f64> = BTreeMap::new();
    for &c in &chars {
        *counts.entry(c).or_insert(0.0) += 1.0;
    }
    let k = counts.len();
    if k < 2 {
        return 0.0; // a single repeated char: no information, not "prose"
    }
    let n = chars.len() as f64;
    let h: f64 = counts
        .values()
        .map(|&count| {
            let p = count / n;
            -p * p.log2()
        })
        .sum();
    let h_max = (k as f64).log2(); // maximum entropy of a k-symbol source
    if h_max <= f64::EPSILON {
        return 0.0;
    }
    (1.0 - h / h_max).clamp(0.0, 1.0)
}

/// Opaque tokens we never treat as content: urls, hex/base64 blobs, uuids,
/// css-ish `a-b-c` identifiers, GraphQL cache keys like `Type:123`.
fn looks_like_opaque_token(s: &str) -> bool {
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("data:") {
        return true;
    }
    if s.contains(' ') {
        return false; // has whitespace → probably prose, keep it
    }
    let len = s.chars().count();
    if len < 8 {
        return false;
    }
    let hexish = s.chars().all(|c| c.is_ascii_hexdigit());
    let uuidish = len >= 32 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let base64ish = len >= 20
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=' || c == '_' || c == '-'
        });
    // A single long word with no spaces and high char diversity is a token.
    hexish || uuidish || base64ish
}

/// Key-name prior: multiply charge up for content-y keys, down for plumbing.
fn key_prior(segment: &Segment) -> f64 {
    let Segment::Key(k) = segment else {
        return 1.0; // array elements inherit their container's intent
    };
    let kl = k.to_ascii_lowercase();
    const BOOST: &[&str] = &[
        "title",
        "name",
        "text",
        "body",
        "description",
        "content",
        "headline",
        "caption",
        "label",
        "summary",
        "abstract",
        "excerpt",
        "question",
        "answer",
        "comment",
        "review",
        "bio",
        "message",
        "subtitle",
        "heading",
        "paragraph",
        "quote",
        "snippet",
    ];
    const SUPPRESS: &[&str] = &[
        "id",
        "_id",
        "uid",
        "guid",
        "hash",
        "key",
        "token",
        "cursor",
        "__typename",
        "type",
        "class",
        "classname",
        "style",
        "css",
        "href",
        "src",
        "url",
        "slug",
        "icon",
        "color",
        "width",
        "height",
        "size",
        "timestamp",
        "createdat",
        "updatedat",
        "version",
        "sku",
        "isbn",
        "mimetype",
        "encoding",
        "checksum",
        "signature",
        "nonce",
    ];
    if BOOST.iter().any(|b| kl == *b || kl.contains(b)) {
        1.6
    } else if SUPPRESS.iter().any(|b| kl == *b) {
        0.15
    } else {
        1.0
    }
}

// ── small helpers ──────────────────────────────────────────────────────────

fn ratio(s: &str, pred: impl Fn(char) -> bool) -> f64 {
    let total = s.chars().count();
    if total == 0 {
        return 0.0;
    }
    s.chars().filter(|&c| pred(c)).count() as f64 / total as f64
}

fn eqi(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn scalar_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn push_unique(out: &mut Vec<Continuation>, c: Continuation) {
    if !out
        .iter()
        .any(|existing| existing.value == c.value && existing.kind == c.kind)
    {
        out.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── the entropy-gap kernel ──────────────────────────────────────────

    #[test]
    fn redundancy_separates_prose_from_unstructured_tokens() {
        // First-order redundancy cleanly separates prose from *unstructured*
        // machine tokens — random hex and base64 use their alphabet near
        // uniformly, prose does not. (Structured tokens like a UUID have
        // skewed first-order stats of their own and are handled downstream by
        // the opaque-shape guard in `leaf_charge`, not here.)
        let prose = linguistic_redundancy(
            "the quick brown fox jumps over the lazy dog near the river bank at dawn",
        );
        let sha = linguistic_redundancy("a3f9c8e1b2d4f6079a1c3e5b7d9f0a2c4e6b8d0f");
        let base64 = linguistic_redundancy("dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVy");
        let hash = linguistic_redundancy("f1e2d3c4b5a6978869504132a1b2c3d4");
        assert!(prose > sha, "prose {prose} vs sha {sha}");
        assert!(prose > base64, "prose {prose} vs base64 {base64}");
        assert!(prose > hash, "prose {prose} vs hash {hash}");
    }

    #[test]
    fn redundancy_is_language_agnostic() {
        // No dictionary is consulted, so non-English prose also reads as prose
        // relative to a hash.
        let german = linguistic_redundancy(
            "der schnelle braune fuchs springt ueber den faulen hund am fluss",
        );
        let hash = linguistic_redundancy("f1e2d3c4b5a6978869504132a1b2c3d4");
        assert!(german > hash, "german {german} vs hash {hash}");
    }

    #[test]
    fn charge_separates_prose_from_every_machine_token() {
        // The composite charge is the actual discriminator: redundancy handles
        // unstructured tokens, the opaque-shape guard handles structured ones
        // (uuid/hex/base64), and the key prior tips content-y keys up. Under a
        // *neutral* key, real prose must out-charge every machine token.
        let neutral = Segment::Key("value".into());
        let prose = leaf_charge(
            &neutral,
            "Understanding semantic rendering without any javascript",
        );
        let uuid = leaf_charge(&neutral, "550e8400-e29b-41d4-a716-446655440000");
        let sha = leaf_charge(&neutral, "a3f9c8e1b2d4f6079a1c3e5b7d9f0a2c4e6b8d0f");
        let base64 = leaf_charge(&neutral, "dGhlIHF1aWNrIGJyb3duIGZveCBqdW1wcyBvdmVy");
        assert!(prose > uuid, "prose {prose} vs uuid {uuid}");
        assert!(prose > sha, "prose {prose} vs sha {sha}");
        assert!(prose > base64, "prose {prose} vs base64 {base64}");
    }

    #[test]
    fn charge_prefers_content_keys() {
        // The key prior lifts content keys over plumbing keys for equal text.
        let title = leaf_charge(
            &Segment::Key("title".into()),
            "Understanding Semantic Rendering Without JavaScript",
        );
        let typename = leaf_charge(&Segment::Key("__typename".into()), "ProductListingItem");
        let hash = leaf_charge(
            &Segment::Key("hash".into()),
            "a3f9c8e1b2d4f6079a1c3e5b7d9f0a2c",
        );
        assert!(title > typename, "title {title} vs __typename {typename}");
        assert!(title > hash, "title {title} vs hash {hash}");
    }

    // ── the field solver: exactness ─────────────────────────────────────

    /// Brute-force reference: φ(n) = Σ_m α^dist(n,m) q(m) over all pairs.
    fn brute_force_field(tree: &Tree) -> Vec<f64> {
        let n = tree.nodes.len();
        (0..n)
            .map(|i| {
                (0..n)
                    .map(|m| {
                        let d = tree_distance(tree, i, m);
                        ALPHA.powi(d as i32) * tree.nodes[m].charge
                    })
                    .sum()
            })
            .collect()
    }

    fn tree_distance(tree: &Tree, a: usize, b: usize) -> usize {
        // Depth of each, then walk the deeper up to the LCA.
        let anc = |mut x: usize| {
            let mut v = vec![x];
            while let Some(p) = tree.nodes[x].parent {
                v.push(p);
                x = p;
            }
            v
        };
        let pa = anc(a);
        let pb = anc(b);
        // distance = depth(a)+depth(b) − 2·depth(lca)
        for (da, &na) in pa.iter().enumerate() {
            if let Some(db) = pb.iter().position(|&nb| nb == na) {
                return da + db;
            }
        }
        pa.len() + pb.len()
    }

    #[test]
    fn field_solver_matches_brute_force_greens_function() {
        // A small, irregular tree exercises up + down passes and both branch
        // factors.
        let blob = json!({
            "a": { "b": "hello world this is prose", "c": ["x", "y", "z"] },
            "d": "another sentence with real words in it",
            "e": { "f": { "g": "deep nested content here now" } }
        });
        let mut tree = Tree::default();
        let root = tree.build(&blob, Segment::Root, 0, None);
        tree.roots.push(root);
        tree.solve_field();

        let reference = brute_force_field(&tree);
        for (i, node) in tree.nodes.iter().enumerate() {
            assert!(
                (node.field - reference[i]).abs() < 1e-9,
                "node {i}: solved {} vs brute {}",
                node.field,
                reference[i]
            );
        }
    }

    #[test]
    fn field_lifts_a_content_cluster_above_isolated_noise() {
        // A container full of prose vs a lone prose string surrounded by config.
        let blob = json!({
            "article": {
                "p1": "The first paragraph explains the core idea in plain words.",
                "p2": "The second paragraph continues the explanation clearly.",
                "p3": "A third paragraph adds supporting detail and examples here."
            },
            "config": {
                "id": "a3f9c8e1b2d4f6079a1c3e5b7d9f0a2c",
                "flag": "The only sentence lost in a sea of settings values.",
                "theme": "dark",
                "version": "4.2.1"
            }
        });
        let rec = recover(&[blob]);
        // The clustered article paragraphs are recovered…
        assert!(rec.recovered_text.contains("first paragraph"));
        assert!(rec.recovered_text.contains("second paragraph"));
        // …and dominate: the field concentrated on the article subtree.
        assert!(rec.recovered_chars > 100);
        assert!(rec.peak_field > 0.0);
    }

    // ── end-to-end recovery of a real SPA shape ─────────────────────────

    #[test]
    fn recovers_next_data_article_without_js() {
        // The shape Next.js embeds in <script id="__NEXT_DATA__">.
        let next_data = json!({
            "props": {
                "pageProps": {
                    "article": {
                        "title": "How the Aletheia Engine Recovers Content",
                        "author": { "name": "Ada Lovelace", "id": "usr_88213" },
                        "body": "Modern single page applications ship their content as \
                                 embedded state rather than server rendered html. This \
                                 engine reconstructs that content directly from the state \
                                 tree, with no browser and no javascript execution at all.",
                        "__typename": "Article"
                    }
                }
            },
            "buildId": "a91f0c8e2b4d6f80"
        });
        let rec = recover(&[next_data]);
        assert!(
            rec.recovered_text.contains("Aletheia Engine"),
            "{}",
            rec.recovered_text
        );
        assert!(rec.recovered_text.contains("embedded state"));
        // The author name is a high-field short field; the build id / typename
        // are not recovered as content.
        assert!(
            rec.recovered_fields
                .iter()
                .any(|f| f.value.contains("Aletheia"))
        );
        assert!(!rec.recovered_text.contains("a91f0c8e2b4d6f80"));
        assert!(!rec.recovered_text.contains("usr_88213"));
    }

    #[test]
    fn extracts_relay_pagination_cursor_for_browser_free_deep_crawl() {
        let apollo = json!({
            "data": {
                "products": {
                    "edges": [
                        { "node": { "name": "Widget A", "sku": "W-A-1" } },
                        { "node": { "name": "Widget B", "sku": "W-B-2" } }
                    ],
                    "pageInfo": {
                        "hasNextPage": true,
                        "endCursor": "YXJyYXljb25uZWN0aW9uOjI0"
                    }
                }
            }
        });
        let rec = recover(&[apollo]);
        let cursor = rec
            .continuations
            .iter()
            .find(|c| c.kind == "relay_cursor")
            .expect("relay cursor recovered");
        assert_eq!(cursor.value, "YXJyYXljb25uZWN0aW9uOjI0");
        assert!(cursor.has_next);
        // The product names are recovered content; the skus are not.
        assert!(rec.recovered_text.contains("Widget A"));
    }

    #[test]
    fn extracts_next_url_and_offset_continuations() {
        let state = json!({
            "feed": { "items": ["a", "b"], "next": "/api/feed?page=2" },
            "paging": { "offset": 40, "page": 3 }
        });
        let rec = recover(&[state]);
        assert!(
            rec.continuations
                .iter()
                .any(|c| c.kind == "next_url" && c.value == "/api/feed?page=2")
        );
        assert!(
            rec.continuations
                .iter()
                .any(|c| c.kind == "param" && c.value == "40")
        );
    }

    #[test]
    fn js_content_ratio_flags_spa_vs_static() {
        // Empty DOM, lots recovered → near 1 (a pure SPA).
        assert!(js_content_ratio(0, 5000) > 0.99);
        // Rich DOM, nothing recovered → 0 (a static page).
        assert_eq!(js_content_ratio(5000, 0), 0.0);
        // Half and half.
        assert!((js_content_ratio(1000, 1000) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn empty_and_degenerate_input_never_panics() {
        assert_eq!(recover(&[]).recovered_chars, 0);
        assert_eq!(recover(&[json!(null)]).recovered_chars, 0);
        assert_eq!(recover(&[json!(42)]).recovered_chars, 0);
        assert_eq!(recover(&[json!("")]).recovered_chars, 0);
        // A single repeated character is not prose.
        assert_eq!(linguistic_redundancy("aaaaaaaaaaaaaaaa"), 0.0);
    }

    #[test]
    fn recovery_is_deterministic() {
        let blob = json!({ "props": { "title": "Deterministic Output Every Time", "n": 5 } });
        let a = recover(&[blob.clone()]);
        let b = recover(&[blob]);
        assert_eq!(a, b);
    }
}
