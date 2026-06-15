# Astra architecture

A map of the system so it stays navigable. Everything lives in one cargo workspace; `astra-core`
holds the services, `astra-server` exposes them over HTTP, `astra-supervisor` handles binary
promotion. `AppState` (`crates/core/src/lib.rs`) wires every service together and is shared into
every route handler.

## Layers

```
            ┌─────────────────────────────────────────────────────────────┐
  ingress   │  HTTP /api/v1 (astra-server)   ·   Telegram bot   ·   timers │
            └───────────────┬─────────────────────────────────────────────┘
                            │  AppState (one wired graph of services)
        ┌───────────────────┼───────────────────────────────────────────┐
 reason │  asc2 (remote LLM) · agentic_loop · assistant · agent_runtime  │
        ├───────────────────┼───────────────────────────────────────────┤
 act    │  device_agent (host: shell/fs/proc)  ·  deep_crawl + web_search │
        │  semantic_render (HTML→records)      ·  live_context · connectors│
        ├───────────────────┼───────────────────────────────────────────┤
 verify │  verification (the one Check type)                              │
 & know │  proof_economy (staked claims) · proof_round (self-driving)     │
        │  chronicle (episodic memory) · research · nexus · turboquant     │
        ├───────────────────┼───────────────────────────────────────────┤
 govern │  sandbox (block-by-default reference monitor) · httpa · guardian │
        └─────────────────────────────────────────────────────────────────┘
```

The arbiter of truth at the bottom of the reasoning stack is **not the LLM** — it is a
deterministic `Check` executed through the device layer. The LLM proposes and acts; reality (an
executed check) decides.

## Modules by domain (`crates/core/src`)

**Foundation**
| Module | Purpose |
|---|---|
| `common` | `AppConfig`, `AppError`, `ApiResponse`, ids/time/hashing, `run_blocking_io` |
| `error` | error → HTTP mapping |

**Runtime & orchestration**
| Module | Purpose |
|---|---|
| `agent_runtime` (`+swarm.rs`) | generated-agent executions, cooperative swarm scheduler, shared `execute_tool` tool dispatch |
| `autonomy` | background job queue + worker (tick loop) |
| `weave` | intent substrate: NL intent → composed/materialized agent run |

**Reasoning & agents**
| Module | Purpose |
|---|---|
| `asc2` | remote-LLM mission controller; `ReasoningExecutor` trait (`execute` + `complete`), `RemoteReasoningExecutor`, `StubLocalExecutor` |
| `agentic_loop` | **Verified Agentic Loop** — plan→act→observe→verify→replan; mints/recalls via the economy |
| `assistant` | NL command → safety-classified action plan |
| `agent_quality` | quality scoring helpers |

**Act — device & web (the agent's hands & eyes)**
| Module | Purpose |
|---|---|
| `device_agent` | supervised host tools (system_info/process_list/disk_usage/fs_*/open_path/shell_exec) + `DeviceSupervisor` (live watch, kill-switch, audit) + hard blocklist |
| `deep_crawl` | safe_fetch → semantic render → link-follow BFS; surfaces a structured `dataset`; auto-seeds from SearXNG |
| `web_search` | `SearxngClient` — query → result URLs (discovery layer) |
| `search_intelligence` | governed deep-research engine: relevance/credibility/freshness scoring, persisted index (quantized embeddings) |
| `semantic_render` | HTML → structured records: title/headings/links + JSON-LD entities + tables + pagination (the structured-data harvester) |
| `connectors` | connector registry |

> Note: `live_context.rs` exists but is currently an **orphan** (no `mod` declaration — not compiled). It's a cleanup/wire-in candidate.

**Verify & knowledge**
| Module | Purpose |
|---|---|
| `verification` | the **one shared `Check`** type (file/command/output/impossibility/trivial) + `run(check, device, ctx)` + reversibility/cost |
| `proof_economy` | adversarial, staked, machine-verified claim ledger (propose/attack/mint/verify, reputation, VOI `next_experiment`, `find_minted_semantic`) |
| `proof_round` | **self-driving conductor** — LLM proposer + refuter populations run rounds against the economy (+ env-gated timer) |
| `chronicle` | episodic continuity memory (decay/archive/insights/brief) |
| `research`, `nexus` | research jobs; Nexus business module |
| `turboquant`, `text_match` | vector quantization; deterministic lexical similarity (semantic recall) |

**Govern & safety**
| Module | Purpose |
|---|---|
| `sandbox` | block-by-default reference monitor: capability/taint/risk/approval gates, Docker exec path, plain audit log |
| `httpa` | sessions + intents + plain receipts |
| `os_guardian` | OS event monitoring + DLP secret redaction |
| `artifacts` | artifact safety |

## Key data flows

**Telegram web-agent** (a question → a grounded answer)
```
message → telegram.handle_mission
  → gather_web_evidence: device.deep_crawl (SearXNG discover → fetch → semantic render → rank)
  → augment objective with evidence
  → asc2.execute_mission_with_executor (user's BYOK LLM) → synthesized answer + sources
```

**Verified agentic loop** (`POST /agent/run`)
```
loop: LLM.complete(state) → NextAction{tool,input,postcondition,done}
  → allowlist check → device.execute(tool)            (supervised: watchdog/kill/audit)
  → verification.run(postcondition)  → verified | unverified | failed
  → feed observation back → replan … until done/budget
on completion: propose the verified result as a proof-economy claim   (supply)
at start: find_minted_semantic(objective) → recall a proven result, skip work  (memory)
```

**Proof economy round** (`POST /proof/round`, or the timer)
```
propose: LLM → {statement, check} → economy.propose → device runs the check (arbiter)
attack (VOI-ordered): economy.next_experiment → LLM refuter → economy.attack
  survive K attacks + check still passes → MINTED (permanent, re-verifiable, staked)
  check fails → REFUTED (proposer's stake → attacker)
```

## The verified-knowledge stack (the novel core)

```
verification.Check          one machine-checkable, device-run predicate
        │  used by
        ├── agentic_loop     proves each mutating action; mints/recalls results
        └── proof_economy    the staked ledger; Check is the impartial arbiter
                  │  driven by
                  └── proof_round   LLM proposer/refuter populations, self-driving
```
This is what turns "an action you must trust" into "a claim with a proof anyone can cheaply
re-run," and lets verified work compound instead of being re-litigated.

## AppState wiring (construction order matters)

`device` → `proof_economy` (needs device) → reasoner (from `ASTRA_ASC2_REMOTE_ENDPOINT`) →
`agentic_loop` (`.with_economy`) + `proof_conductor` → the rest. The reasoner is shared (clone)
by the loop and the conductor. Background workers (`spawn_autonomy_worker`,
`spawn_telegram_worker`, `spawn_proof_conductor`) are launched from `astra-server`'s `main`.

## Honest boundaries

- **Truth is software-grounded, not physical.** A `Check` runs a command/file/crawl — there is no lab, sensor, or robot. This is the trust/verification substrate, not a scientific solver.
- **Reasoning quality = the LLM you configure.** Small local models are unreliable at the JSON tool-calling the loop and conductor need.
- **No JS rendering** (SPA/bot-protected sites yield little) and **no authenticated automation** yet.
- **Recall is lexical** (`text_match`), not embedding-based; a real model can drop in behind the same `similarity` signature.
- **Audit is plain logging** — the former hash-chained ledgers were removed.
