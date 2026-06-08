# ASC-II Runtime

ASC-II is the compiled server's recursive autonomous-agent controller. It defaults to `shadow`
mode: decisions, formula metrics, proof packets, trust history, and dynamic sub-agent manifests
are persisted, but ASC-II does not authorize external side effects.

## Configuration

- `ASTRA_ASC2_MODE`: `disabled`, `shadow`, or `active`
- `ASTRA_ASC2_REMOTE_ENDPOINT`: OpenAI-compatible API base URL or chat-completions URL
- `ASTRA_ASC2_REMOTE_API_KEY`: optional bearer token
- `ASTRA_ASC2_REMOTE_MODEL`: remote model name
- `ASTRA_ASC2_OPENCLAW_BASE_URL`: HTTP endpoint used for shared-task baseline comparisons
- `ASTRA_ASC2_AUTO_PROMOTE`: promote validated candidate binaries when `true`

Sensitive missions always use the local deterministic executor. Active-mode side effects require
owner authorization and a passing proof-carrying action certificate.

## Supervisor

Run `scripts/start_astra_supervised.ps1` to build and launch the rollback supervisor. The
supervisor watches `ASTRA_DATA_DIR/asc2/active_version.json`, verifies the candidate binary hash,
checks `/api/v1/ready`, and restores the previous version after failed promotion or repeated
health-check failures.

Candidate Rust changes are restricted to generated strategy, policy, manifest, and prompt paths.
Promotion requires formatting, workspace tests, the reasoning benchmark, a release build, and a
promotable ASC-II benchmark report.
