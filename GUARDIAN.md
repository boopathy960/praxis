# The CEO Guardian — continuous, autonomous OS-security

The CEO is the single autonomous agent that runs this OS. The **Guardian** is the
organ that makes it *continuously watch the machine it runs on* — finding
loopholes, malware, and data leaks, and driving every fix through the same
governed authority the CEO already holds.

It is **owned by the CEO** (`ceo.guardian`), not a second agent. One mind, one
loop:

```
   observe ──▶ assess ──▶ (triage) ──▶ remediate ──▶ record
   device      OS Guardian   model       │            store + chronicle + ledger
   eyes        threat + DLP  reasoning    │
                                          ├─ Observed                 informational, logged
                                          ├─ AutoHealed               safe, non-destructive containment
                                          ├─ ApprovalRequired         destructive host fix → Policy proposal
                                          ├─ SelfModificationProposed harden the OS's own code/config
                                          └─ CapabilityProposed       forge a detector / propose a sub-agent
```

## What it does each patrol

1. **Observe** — turns the project's own supervised [device layer](backend/crates/core/src/device_agent.rs)
   eyes on the host:
   - `process_list` → every running process image.
   - a bounded, depth-limited **recursive** walk (`fs_list` + `fs_read`) over the
     configured scan roots → sensitive files (`.env`, `id_rsa`, `*.pem`,
     `credentials`, …) anywhere in the tree, read under the device read-cap.
     Heavy build/VCS/cache dirs are skipped; `.ssh`/`.aws`-style secret dirs are
     not. Caps: depth 3, 300 dirs, 24 file reads per patrol.
   - `net_connections` (a new first-class device primitive: `netstat -ano` /
     `ss -tunap`) → exposed listeners bound to all interfaces, each scored
     through the OS Guardian's Network path. Every listener's PID is correlated
     to its **owning process** (from the same-patrol process snapshot): a socket
     owned by a malware-like process is escalated to a `Malware` finding.
     **Outbound** connections from a malware-like process to a routable public
     host are flagged as `Malware` (possible exfiltration / command-and-control).
   - `system_info` → the OS's own security posture.
2. **Assess** — scores each observation through the existing
   [OS Guardian](backend/crates/core/src/os_guardian.rs):
   - Processes → the threat evaluator (malware-like execution patterns).
   - File contents → `scan_outbound_text` + the DLP analyzer (secrets at rest).
   - Posture → structural loopholes (e.g. the device layer in permissive mode).
2b. **Learn the machine (baseline drift).** The Guardian persists a per-machine
   baseline of the host's surfaces (network listeners, sensitive-file
   fingerprints). Once a first patrol has seeded it, a listener that **newly
   appears** — even on loopback — or a credential file whose contents **changed**
   becomes an `Anomaly` finding. Anomaly-by-change is a far stronger, lower-noise
   signal than any static pattern; the first patrol only seeds (it never flags),
   and surfaces unseen for 30 days are pruned so a returning one is flagged again.
3. **Triage** *(model-backed, bounded to one call/patrol)* — the reasoner judges
   the single worst finding: genuine threat or false positive, and the
   least-invasive correct fix. Advisory only — it enriches the finding, it never
   authorizes an action.
4. **Remediate** — the safe half of every fix is applied immediately
   (containment manifests, rotation/quarantine plans written **only** to the
   Guardian's own data dir); everything host-altering opens an approval-gated
   path:
   - destructive remediation → a `Policy` proposal to the governance desk;
   - a systemic loophole in the OS's own code → a real, bounded `SelfModification`
     proposal (drafted via ASC-II, exactly like `Ceo::self_modify`);
   - a missing detector capability → a Forge-authored, proof-minted tool.
5. **Record** — every finding and patrol is persisted, remembered in the
   chronicle, and receipted in the OS Guardian ledger.
6. **Execute (closing the loop).** A separate executor pass carries out the
   remediations the court has **approved**. It is doubly gated: governance
   approval is required, and the one destructive action — terminating a
   malware-classified process — *additionally* needs `ASTRA_GUARDIAN_EXECUTE=on`.
   Without that flag an approved destructive fix is recorded as `planned` (the
   exact command it would run) and left for a human; non-destructive ones are
   recorded as `advisory`. This is the only place the Guardian alters the host.

The Guardian also aggregates recent findings into a **posture** — a
severity-weighted risk score, an A–F health grade, and the open-remediation
count — and folds that one-line summary into the CEO's executive deliberation, so
the machine's live security state actively drives self-thinking and self-evolution.

## Safety by construction

The Guardian **never** performs a destructive host action on its own. It mirrors
the OS Guardian policy (`destructive_actions_enabled = false`,
`remediation_requires_approval = true`): killing a process, deleting a file, or
blocking a socket is only ever *proposed* to the Supreme Court (governance), and
waits for the court's 51%. Auto-heal writes only to the Guardian's own data
directory. Secrets are never persisted raw — evidence is redacted pattern matches
and fingerprints. Every patrol is bounded (process/file/finding/proposal caps) so
an incident storm can never hammer the model or the host.

## Running it

Off by default. Enable the continuous patrol:

| Env | Default | Meaning |
|---|---|---|
| `ASTRA_CEO_GUARDIAN` | `off` | turn on the continuous patrol worker |
| `ASTRA_CEO_GUARDIAN_INTERVAL_SECS` | `240` | cadence (min 30) |
| `ASTRA_GUARDIAN_SCAN_ROOTS` | project workspace | `;`/`,`-separated dirs to inspect |
| `ASTRA_GUARDIAN_SELF_MOD` | on if a model is configured | allow drafting hardening self-mods |
| `ASTRA_GUARDIAN_FORGE` | on if a model is configured | allow forging detector tools |
| `ASTRA_GUARDIAN_EXECUTE` | `off` | allow the executor to run **approved** destructive remediations (process termination) |

Self-modification and tool-forging additionally require a configured remote model
(`ASC-II` remote endpoint); without one the Guardian still runs fully as a
deterministic detector + safe-containment loop.

## API

| Route | Method | |
|---|---|---|
| `/api/v1/ceo/guardian/status` | GET | model backing, scan roots, counts, baseline size, peak severity |
| `/api/v1/ceo/guardian/posture` | GET | severity-weighted risk, health grade, open remediations |
| `/api/v1/ceo/guardian/findings` | GET | recent security findings, newest first |
| `/api/v1/ceo/guardian/patrols` | GET | recent patrol reports |
| `/api/v1/ceo/guardian/patrol` | POST | trigger one patrol now (admin-gated) |
| `/api/v1/ceo/guardian/remediations` | GET | executor records (executed / planned / advisory) |
| `/api/v1/ceo/guardian/remediate` | POST | run the remediation executor now (admin-gated) |
| `/guardian` | GET | the live security dashboard (self-contained HTML) |

Implementation: [`backend/crates/core/src/ceo_guardian.rs`](backend/crates/core/src/ceo_guardian.rs).
Owned by the CEO ([`backend/crates/core/src/ceo.rs`](backend/crates/core/src/ceo.rs));
ticked by `spawn_ceo_guardian_worker` in
[`backend/crates/server/src/main.rs`](backend/crates/server/src/main.rs).
