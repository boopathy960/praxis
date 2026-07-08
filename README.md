# PRAXIS — a self-hostable, auditable personal AI agent backend

praxis is a security-focused personal-AI backend in Rust. You point it at your own LLM
(any OpenAI-compatible endpoint — local Ollama or a cloud key), and it gives agents **real,
supervised hands on your machine and the web**, executes work through a **verified agentic
loop** (every mutating action must prove a machine-checkable postcondition), and accumulates
results in a **proof economy** — an adversarial, staked ledger where a claim is minted into
shared memory only after it survives attack and arrives with a proof anyone can re-run cheaply.

Everything runs on your infrastructure, with your keys, and every action is supervised and
logged. The differentiator over a hosted agent is not raw intelligence — it is **control,
privacy, and the ability to verify what the agent actually did.**

> **Honest scope.** This is real, compiling, tested infrastructure (190+ tests). It is *not*
> a frontier autonomous solver and makes no physical-world measurements. See
> [Capabilities & boundaries](#capabilities--honest-boundaries) before relying on it.

## Workspace layout

```
backend/
  Cargo.toml            # cargo workspace (edition 2024)
  crates/
    core/               # astra-core: all services, safety gates, persistence (SQLite)
    server/             # astra-server: actix-web HTTP API (binary: astra-server)
    supervisor/         # astra-supervisor: binary promotion/rollback supervisor
  scripts/              # supervised startup script
  ARCHITECTURE.md       # system map: domains, data flows, the verified-knowledge stack
  ASC2.md               # ASC-II controller notes
  NEXUS_BACKEND.md      # Nexus module notes
```

There is no local "brain" crate: the offline cognitive engine was removed — reasoning now runs
on a remote/self-hosted LLM (or a no-op stub when none is configured). One small piece,
`turboquant` (vector quantization), was kept and now lives in `astra-core`.

## Build, test, run

```sh
cd backend
cargo build --release          # all workspace crates
cargo test                     # full suite (core + server + supervisor)
cargo run -p astra-server      # serves http://127.0.0.1:8080
```

Smoke test:

```sh
curl http://127.0.0.1:8080/api/v1/health
curl http://127.0.0.1:8080/api/v1/ready
```

## Point it at a model (required for any real reasoning)

Astra talks to any OpenAI-compatible `/chat/completions` endpoint. The default config points at
NVIDIA's hosted API running **Nemotron 3 Ultra** (a reasoning model). Get a free key at
[build.nvidia.com](https://build.nvidia.com) — keys look like `nvapi-...`:

```sh
export ASTRA_ASC2_REMOTE_ENDPOINT=https://integrate.api.nvidia.com/v1
export ASTRA_ASC2_REMOTE_API_KEY=nvapi-...
export ASTRA_ASC2_REMOTE_MODEL=nvidia/nemotron-3-ultra-550b-a55b
# Ultra (550B) is the flagship; it can be slow to first token. For snappier runs:
#   nvidia/nemotron-3-super-120b-a12b  or  nvidia/nemotron-3-nano-30b-a3b
```

Nemotron streams a hidden chain-of-thought before its answer, so the client streams responses and
keeps a generous token budget — tune with `ASTRA_ASC2_REMOTE_MAX_TOKENS`,
`ASTRA_ASC2_REMOTE_TIMEOUT_SECS`, and `ASTRA_ASC2_REMOTE_STREAM`.

Prefer local and private? Any OpenAI-compatible server works, e.g. Ollama:

```sh
ollama serve                                   # exposes http://localhost:11434/v1
export ASTRA_ASC2_REMOTE_ENDPOINT=http://localhost:11434/v1
export ASTRA_ASC2_REMOTE_MODEL=qwen2.5:7b-instruct-q4_K_M   # tool-use-capable model recommended
# ASTRA_ASC2_REMOTE_API_KEY left empty for Ollama
```

Without an endpoint, reasoning falls back to a stub that returns honestly: "no model
configured." Tool execution, crawling, and the proof economy's checks still work — only the
*reasoning* needs a model.

## Configuration (environment variables)

**Server**

| Variable | Default | Purpose |
|---|---|---|
| `ASTRA_HOST` / `ASTRA_PORT` | `127.0.0.1` / `8080` | Bind address / port (TLS, if set, binds port+1) |
| `ASTRA_ENV` | `development` | `development`/`dev`/`test` relax auth; anything else = production |
| `ASTRA_ADMIN_TOKEN` | — | **Required outside development**; sent as `x-astra-admin-token`, constant-time compared |
| `ASTRA_DATA_DIR` | `./data` | SQLite stores + artifacts (per-service DB files) |
| `ASTRA_CORS_ALLOWED_ORIGINS` | `http://127.0.0.1:8080` | Comma-separated allowlist |
| `ASTRA_TLS_CERT_PATH` / `ASTRA_TLS_KEY_PATH` | — | PEM cert/key; set together |

**Reasoning (LLM)**

| Variable | Default | Purpose |
|---|---|---|
| `ASTRA_ASC2_MODE` | `shadow` | `disabled` / `shadow` (no side effects) / `active` |
| `ASTRA_ASC2_REMOTE_ENDPOINT` | — | OpenAI-compatible base URL (`/chat/completions` is appended) |
| `ASTRA_ASC2_REMOTE_API_KEY` | — | Bearer token (omit for keyless local servers) |
| `ASTRA_ASC2_REMOTE_MODEL` | `nvidia/nemotron-3-ultra-550b-a55b` | Model id (e.g. `nvidia/nemotron-3-nano-30b-a3b`, `qwen2.5:7b-instruct-q4_K_M`) |
| `ASTRA_ASC2_REMOTE_MAX_TOKENS` | `2048` | Generated-token cap (reasoning models need headroom for thinking + answer) |
| `ASTRA_ASC2_REMOTE_TIMEOUT_SECS` | `300` | Per-request timeout (large reasoning models can be slow to first token) |
| `ASTRA_ASC2_REMOTE_STREAM` | `true` | Stream the completion (SSE); set `false` to buffer |

**Device layer (agent hands on the host)**

| Variable | Default | Purpose |
|---|---|---|
| `ASTRA_DEVICE_MODE` | permissive | Set `confined` to jail file ops to the agent-workspace root |

**Web discovery / crawl**

| Variable | Default | Purpose |
|---|---|---|
| `ASTRA_SEARXNG_URL` | `http://localhost:8888` | Self-hosted SearXNG (JSON output must be enabled in its `settings.yml`) |
| `ASTRA_SEARXNG_TIMEOUT_MS` | `12000` | Search request timeout |

**Self-driving proof economy**

| Variable | Default | Purpose |
|---|---|---|
| `ASTRA_PROOF_AUTONOMY` | off | Set `on` to run the proposer/refuter conductor on a timer (needs an LLM) |
| `ASTRA_PROOF_TOPIC` | host facts | What the proposer makes claims about |
| `ASTRA_PROOF_INTERVAL_SECS` | `300` | Round cadence (min 30) |

**Telegram BYOK bot** — `ASTRA_TELEGRAM_BOT_TOKEN`, `ASTRA_TELEGRAM_ALLOWED_CHAT_IDS`.
**Sandbox tuning** — see `SandboxRuntimeConfig::from_env` in `crates/core/src/sandbox/mod.rs`
(`ASTRA_SANDBOX_*`). **ASC-II** — see `backend/ASC2.md` (`ASTRA_ASC2_*`).

## API surface (`/api/v1`)

| Route group | Purpose |
|---|---|
| `GET /health`, `GET /ready` | Liveness / readiness |
| `POST /agent/run` | **Verified agentic loop** — plan→act→observe→verify→replan with postconditions |
| `POST/GET /proof/*` | **Proof economy** — propose/attack/verify/mint claims, ledger, accounts, `round` |
| `POST/GET /device/*` | **Supervised device layer** — `status`, `live`, `audit`, `execute`, `crawl`, `search` |
| `POST/GET /assistant/commands` | Natural-language commands → safety-classified action plans |
| `POST/GET /asc2/*` | ASC-II missions (remote-LLM), benchmarks, self-modification proposals |
| `POST/GET /os-guardian/*` | OS event monitoring + DLP analysis |
| `POST/GET /httpa/*` | HTTPA sessions, intents, receipts |
| `POST/GET /sandbox/*` | Reference-monitor action evaluation/execution + audit |
| `POST/GET /search/intelligence*`, `/research/*` | Governed deep research + crawl |
| `POST/GET /nexus/*`, `/weave/*`, `/chronicle/*` | Nexus, intent substrate, continuity memory |
| `POST/GET /runtime/*`, `/artifacts/*`, `/connectors/*`, `/render/*` | Agent runtime, artifacts, connectors, semantic render |

All responses use the envelope `{"ok": bool, "data": ..., "error": ...}` with an `x-request-id`.

## Capabilities & honest boundaries

**Solves today**
- Supervised automation of your own machine (shell/fs/process) with a watchdog, kill-switch, and audit log — and, via the agentic loop, **verified** outcomes (an action is only "done" once its postcondition is proven).
- Web → **structured datasets**: crawl + extract JSON-LD entities, tables, and pagination into typed records (not prose).
- Grounded, cited Q&A over the live web (Telegram bot, your key).
- A compounding, machine-checkable knowledge ledger (the proof economy), including proven impossibility ("X cannot be done here").

**Needs setup** — web discovery needs a self-hosted SearXNG; the agentic loop's reliability tracks the LLM you choose (a tool-use-capable model matters); the self-driving conductor needs an LLM + the autonomy flag.

**Cannot do** — physical-world experiments (no labs/sensors; "experiment" = a software check); JS-heavy / bot-protected sites (no headless browser yet); authenticated transactions (no login/purchase); deep semantic understanding (recall is lexical); it is not a frontier hosted agent.

## Security model

- **Block by default.** The sandbox reference monitor denies side-effecting actions unless policy gates pass; writes/sends/payments require approval.
- **The device layer is watched, not unbounded.** Even in permissive mode, host commands run under a watchdog with a timeout + process-tree kill, and a hard blocklist refuses machine-destroying commands (`rm -rf /`, `format`, fork bombs) regardless of mode.
- **Admin auth.** Production write routes require `x-astra-admin-token` (constant-time compared).
- **Secrets never round-trip.** The OS guardian redacts secret-bearing metadata to `redacted:sha3:…` digests.
- **Audit is plain logging.** The previous hash-chained "tamper-evident" ledgers were removed; the sandbox/device/HTTPA records are now plain logs (no `*/verify` endpoints).

## Architecture

See **[ARCHITECTURE.md](backend/ARCHITECTURE.md)** for the domain map, the key data flows, and the
verified-knowledge stack (`verification` → `agentic_loop` → `proof_economy` → `proof_round`).

## Implementation notes

- Service-layer code is synchronous; route handlers that perform live network/host work are dispatched to the actix blocking pool via `web::block`.
- Blocking HTTP (`reqwest::blocking`) and host execution run on dedicated OS threads via `astra_core::common::run_blocking_io` — never create a blocking reqwest client on an async worker (it panics the runtime).
- Persistence is SQLite (WAL) under `ASTRA_DATA_DIR`; each service owns its DB file. Back up that directory.
