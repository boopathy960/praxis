# ASC-II Runtime

ASC-II is the compiled server's recursive autonomous-agent controller. It defaults to `shadow`
mode: decisions, formula metrics, proof packets, trust history, and dynamic sub-agent manifests
are persisted, but ASC-II does not authorize external side effects.

## Configuration

- `ASTRA_ASC2_MODE`: `disabled`, `shadow`, or `active`
- `ASTRA_ASC2_REMOTE_ENDPOINT`: OpenAI-compatible API base URL or chat-completions URL
  (default config: `https://integrate.api.nvidia.com/v1`)
- `ASTRA_ASC2_REMOTE_API_KEY`: optional bearer token (NVIDIA keys look like `nvapi-...`)
- `ASTRA_ASC2_REMOTE_MODEL`: remote model name (default `nvidia/nemotron-3-ultra-550b-a55b`)
- `ASTRA_ASC2_REMOTE_MAX_TOKENS`: generated-token cap (default `2048`). Reasoning models
  (e.g. Nemotron) spend tokens thinking before answering, so keep this generous.
- `ASTRA_ASC2_REMOTE_TIMEOUT_SECS`: per-request timeout (default `300`). Large reasoning
  models can be slow to first token.
- `ASTRA_ASC2_REMOTE_STREAM`: stream the completion as SSE (default `true`). The client
  concatenates `content` deltas and falls back to `reasoning_content` if `content` is empty.
- `ASTRA_ASC2_OPENCLAW_BASE_URL`: HTTP endpoint used for shared-task baseline comparisons
- `ASTRA_ASC2_AUTO_PROMOTE`: promote validated candidate binaries when `true`

Sensitive missions always use the local deterministic executor. Active-mode side effects require
owner authorization and a passing proof-carrying action certificate.

## Binary self-modification

`POST /api/v1/asc2/self-modifications` stages a candidate self-modification. Body:
`{ "workspace_path": "...", "changed_paths": ["asc2/prompts/..."], "current_binary": "..." }`.
It is **admin-gated** (production requires the `x-astra-admin-token` header) and passes a defense-in-depth gate stack before anything is staged:

1. **Declared-path allowlist** — every declared path must be under `asc2/{strategies,policies,manifests,prompts}/` (no traversal / absolute escape).
2. **Real-diff enforcement** — the *actual* `git status` of the workspace must contain no change outside that boundary (the declared list is not trusted; a non-git or unverifiable workspace fails closed).
3. **Promotable benchmark** — the latest ASC-II benchmark must be promotable.
4. **Build gate** — `cargo fmt --check` + `cargo test --workspace` + `cargo build --release` must all pass.
5. **Canary** — the freshly built binary is booted in isolation (ephemeral port, throw-away data dir) and must serve `/api/v1/ready` before it is staged.

Only after all five does it stage the binary (and promote it — writing `active_version.json` with an N-deep `previous_binaries` rollback history — when `ASTRA_ASC2_AUTO_PROMOTE=true`). `GET /api/v1/asc2/self-modifications` lists prior records.

### Autonomous self-modification

`POST /api/v1/asc2/self-modifications/autonomous` (admin-gated). Body:
`{ "goal": "...", "workspace_path": "...", "current_binary": "..." }`. The configured model proposes
ONE bounded change (a single file inside the boundary); it is applied to the workspace and driven
through the full gate stack above. **If any gate rejects it, the proposed change is reverted**, so a
rejected proposal never lingers. Promotion happens only when every gate passes and auto-promote is on.

> Note: the boundary paths (`asc2/*`) are generated *data*, not compiled Rust source — so an
> autonomous change only alters runtime behavior to the extent the server reads those files. A
> change to compiled source is possible but, by design, must be operator-supplied and survive the
> same gates; the loop will not author arbitrary code into the binary.

Tuning: `ASTRA_ASC2_CANARY_PORT` (default `8899`), `ASTRA_ASC2_CANARY_TIMEOUT_SECS` (default `60`).

## Supervisor

Run `scripts/start_astra_supervised.ps1` to build and launch the rollback supervisor. The
supervisor watches `ASTRA_DATA_DIR/asc2/active_version.json` (UTF-8, with or without BOM), verifies
the candidate binary hash, checks `/api/v1/ready`, and restores the previous version after failed
promotion or repeated health-check failures.

Candidate Rust changes are restricted to generated strategy, policy, manifest, and prompt paths.
Promotion requires formatting, workspace tests, the reasoning benchmark, a release build, and a
promotable ASC-II benchmark report.
