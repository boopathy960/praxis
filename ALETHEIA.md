# Aletheia — rendering JavaScript pages without JavaScript

*ἀλήθεια — "unconcealment." Recovering the content a page hides behind its
JavaScript, by reading the state the framework would have rendered from.*

## The problem

The semantic render engine used to strip `<script>` and read the DOM. That
works for server-rendered HTML and fails completely on the modern web: a
Next.js / Nuxt / SvelteKit / Apollo / Redux single-page app ships an **empty
DOM body** plus a large JSON **hydration state** blob, and the browser's
JavaScript builds the page from that state on the client. The article, the
product list, the "next page" cursor — all of it is present in the HTML the
crawler already downloaded, sitting in a `<script>` tag. The DOM read sees a
blank mount point; the content is right there, unread.

The industry answer is to run a headless browser (Puppeteer/Playwright) to
execute the JavaScript. That is heavy (a whole Chromium per page), slow,
fragile, and easy to detect and block. Aletheia takes the other path:
**recover the rendered meaning directly from the state, executing no
JavaScript at all.** Two new mathematical mechanisms make that possible.

---

## Mechanism 1 — Linguistic charge: Shannon redundancy as a prose detector

A hydration blob is mostly framework plumbing — component ids, GraphQL cache
keys, `__typename`, CSS classes, base64 assets, UUIDs, timestamps — with the
actual content scattered through it. To recover content we must first tell
**prose from machine noise**, with no dictionary and no ML model (this runs in
a Rust crawler, over every language on earth).

The discriminator is Shannon's own **redundancy**: how far a string's
character distribution sits from uniform over its own alphabet. For a string
with unigram entropy `H` and alphabet size `k`:

```
    R = 1 − H / H_max ,       H_max = log₂ k
```

Natural language is *redundant* — space and a handful of letters dominate its
character distribution — so `H ≪ H_max` and `R` is large (≈ 0.2–0.35 at the
character level, in any language). A random hash / base64 token uses its
alphabet near-uniformly, so `H ≈ H_max` and `R ≈ 0`. Because this is a
**first-order** statistic it needs very little data, so it stays reliable on
the short strings a state tree is full of — where a conditional-bigram
estimate would be pure noise.

Redundancy alone can't separate prose from a *structured* token like a UUID
(whose repeated digits and dashes give it skewed first-order statistics too),
so the per-node **content charge** `q(n)` folds `R` together with:

- an **opaque-token shape guard** — URLs, all-hex/all-base64 words, and
  `xxxx-xxxx-…` UUID shapes are suppressed regardless of `R`;
- a **key-name prior** — `title/body/description/headline/…` boost, while
  `id/hash/cursor/__typename/url/style/…` suppress;
- **script ratios** (space fraction, letter fraction) and a saturating
  length term.

The result: `q(n) > 0` on real content, `q(n) ≈ 0` on plumbing.

---

## Mechanism 2 — The Semantic Potential Field: a Green's function on the tree

Per-node scoring still misfires. A lone prose-looking string can drift in a
sea of config; an *empty container node* can be exactly the one that holds the
article. Content has **structure**: an article's paragraphs sit together under
one subtree; a nav menu's short labels cluster too.

So Aletheia treats the charge `q` as **electric charge on the JSON tree** and
computes a field in which content regions reinforce and isolated noise damps.
Define the field at every node as the distance-discounted sum of *all* charge:

```
    φ(n) = Σ_m  α^dist(n,m) · q(m) ,        α ∈ (0,1)
```

where `dist` is the number of tree edges between `n` and `m`. This is a
**discrete Green's function** — the resolvent of a screened diffusion
(`(I − αP)⁻¹`) applied to the charge. In general that is an `O(N³)` matrix
inverse. **On a tree it factorizes exactly through the lowest common
ancestor**, because between any two nodes there is exactly one path, so the
walk-sum collapses and `φ` solves in **two linear passes**:

```
    up-pass   (post-order):   U(n) = q(n) + α · Σ_{c ∈ children} U(c)
    down-pass (pre-order):    Ctx(child) = α · φ(parent) − α² · U(child)
                              φ(n) = U(n) + Ctx(n) ,    Ctx(root) = 0
```

`U(n)` is the discounted charge in `n`'s own subtree; the down-pass carries the
**leave-one-out** context from everything *outside* it — `α·φ(parent)` minus
the `α²·U(child)` the child itself contributed, so nothing is double-counted.
Total cost is `O(N)` for `N` state nodes. (The engine's tests verify the
two-pass result equals the brute-force `Σ_m α^dist q(m)` to machine precision.)

Content is then the set of leaves whose field exceeds an adaptive threshold
(a fraction of the page's peak field), **linearized in document order** — the
JSON preserves array/object order, and the tree is built pre-order, so node
index *is* reading order — to reconstruct the text the SPA would have shown.

---

## The deep-crawl payoff — continuation flux

The same state tree that hides the content also hides the way to the *next*
page. A **continuation-flux** scan lifts pagination structure straight out of
the state:

- **Relay `pageInfo`** — objects carrying `endCursor` + `hasNextPage`
  (GraphQL's universal pagination shape) → a `relay_cursor` continuation;
- **`next` / `nextPage` / `nextUrl`** string values → a `next_url`
  continuation the crawler fetches directly;
- **`cursor` / `after` / `offset` / `page`** scalars → `param` continuations.

This lets the deep-crawl engine walk an infinite feed or a paginated listing
**with no browser** — the exact case that previously required executing the
site's JavaScript. URL-shaped continuations are seeded to the front of the
crawl frontier, ahead of DOM pagination and outbound links.

`js_content_ratio = recovered / (dom_text + recovered) ∈ [0,1]` reports how
much of a page's meaning lived only in JavaScript state — ≈ 0 for a static
page, ≈ 1 for a pure SPA — so the crawler knows when it has just rendered a
JS-only page without a browser.

---

## Where it lives

| piece | file |
|---|---|
| the engine (charge kernel, field solver, continuation flux) | `backend/crates/core/src/aletheia.rs` |
| state-blob extraction + report wiring | `backend/crates/core/src/semantic_render.rs` (`extract_state_blobs`, `RecoveredContent`) |
| thin-DOM SPA fallback + continuation seeding | `backend/crates/core/src/deep_crawl.rs` |

Everything is deterministic and unit-tested: the entropy discriminator, the
exact-vs-brute-force field solve, end-to-end recovery of a Next.js
`__NEXT_DATA__` article, Relay-cursor extraction, and the SPA render path
through the full engine — all with **no JavaScript engine anywhere in the
stack**.
