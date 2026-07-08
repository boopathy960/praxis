# Agora — the proof-settled market for machine labor

> **Layer 2 exists:** [NOOSPHERE.md](NOOSPHERE.md) — the collective-intelligence engine
> built on this settlement substrate (lemma markets, proof-grounded agent ranking,
> prediction layer, reasoning duels, self-play flywheel).

**The innovation:** federate Astra's proof economy across trust domains, so that two agents
owned by strangers can transact work — and **payment settles if and only if the buyer's own
machine re-runs the seller's `Check` and it passes.** Not reputation, not attestations, not
validators, not a platform's word: settlement by local deterministic re-execution of a
machine-checkable proof.

This document is (1) a deep read of what Astra is, (2) the structural gap that read exposes,
(3) the billion-dollar problem that gap maps onto, and (4) the design of the missing layer.

---

## 1. What Astra actually is (the deep read)

Strip away the module names and Astra is one idea applied recursively:

> **A statement about the world is worthless until a deterministic, re-runnable `Check`
> executed against reality confirms it — and the LLM is never the arbiter.**

Every organ is that idea pointed at a different target:

| Organ | The idea pointed at… |
|---|---|
| `verification::Check` | the primitive itself — one serializable, deterministic predicate |
| `agentic_loop` | actions ("done" = postcondition proven) |
| `proof_economy` | knowledge (claims staked, attacked, minted) |
| `proof_round` | knowledge production (self-driving proposer/refuter) |
| `sentinel` | time (standing properties, drift ledger) |
| `active_inference` | attention (predict checks, look where surprise is likely) |
| `noesis` | generalization (predict checks never run; imagination pays rent in proof) |
| `crucible` | causality (competing explanations, discriminating experiment) |
| `forge` / `architect` | the system's own tools and agents (capabilities must prove themselves) |
| `genome` | company operations (certified contracts: envelope + conformal residual) |
| `continuum` | six external verticals (reproducibility, supply chain, compliance, spec mining, impossibility registry, guardrails) |
| `ceo` / `governance` | authority itself (49% executive under a 51% constitutional court) |

This is a complete **trust machine**. And it has exactly one structural property that no
amount of new organs will fix:

**Everything lives inside one trust domain. One machine, one owner, one ledger.**

## 2. The gap: an economy of one

`proof_economy` is named as an economy — stake, attack, mint, reputation, even
value-of-information pricing for the next experiment. But an economy with one participant is
not an economy; it is a ledger. A currency only you accept is not money.

Grep the codebase: there is no federation, no peer identity, no claim exchange, no cross-node
anything. Every minted claim — every surviving proof, every proven impossibility, every
sentinel-watched guarantee — is worth something only to the machine that minted it.

Meanwhile every hard sub-problem of *selling* proofs to strangers is already built, as a
single-node organ, and stops exactly one layer short of the market:

- A **portable proof format** — `Check` is a serde-serializable, constrained enum. It is a
  *DSL, not code*: re-running a foreign check means interpreting a small JSON AST through your
  own supervised device layer, never executing a stranger's script. This is the safety
  property every "run my verification script" scheme lacks, and Astra has it by construction.
- A **safe execution substrate for foreign work** — the block-by-default sandbox reference
  monitor, Docker exec path, watchdog, kill-switch, hard blocklist.
- **Settlement semantics** — propose/stake/attack/mint/slash already implement escrow logic;
  they just clear against a local reputation float instead of a counterparty.
- A **warranty layer** — the sentinel already turns "verified once" into "continuously
  re-verified, with the exact timestamp it broke."
- An **underwriting layer** — `genome::underwriting` already prices correlated risk with
  concentration limits.
- **Receipts** — `httpa` already issues session/intent receipts.
- An **enforcement arm** — governance's army/police already revoke misbehaving actors inside
  the boundary.
- An **autonomous market participant** — the CEO can already propose, budget, and act under
  constitutional control. Give it counterparties and it can *earn*.

The whole codebase has been converging on a market and has never met a second node.

## 3. The problem, and why it is worth more than a billion dollars

### The lemons market for machine labor

Agent-to-agent commerce is arriving at scale: Juniper projects agentic-commerce transaction
value growing from ~$8B (2026) to **~$1.5T by 2030**; McKinsey's global framing reaches
**$3–5T**. Yet Cisco's enterprise survey found **85% of enterprises run agent pilots and only
5% ship them** — and the top named barrier is trust and risk, not capability.

The reason is structural, and it is Akerlof's lemons market replayed at machine speed: when a
buyer cannot verify quality without redoing the work, rational buyers discount every offer to
the value of the worst one, good sellers exit, and the market collapses to slop. Human labor
markets escaped this with courts, escrow, audits, and reputation accumulated over years.
Agents have none of these, and the human versions don't scale to counterparties that are
spawned in milliseconds, transact in seconds, and cost nothing to abandon.

Every serious attempt at "agent trust" attacks the wrong layer:

- **Identity & KYA** (Skyfire, Lyrie's Agent Trust Protocol, AgentFacts, Web Bot Auth) —
  proves *who the agent is*. An authenticated agent can still deliver garbage.
- **Payment rails** (Coinbase's x402, Payman, Stripe/Visa/Mastercard agentic stacks) — proves
  *the agent can pay*. Money movement is solved; whether the paid-for work is correct is not.
- **Intent audit trails** (Mastercard Verifiable Intent) — proves *what was asked and that a
  transaction matches the mandate*. A tamper-evident record of the agreement, silent on
  whether the delivered work satisfies it.
- **Validator/jury marketplaces** (VAGACHAIN and kin) — the closest neighbor: signed execution
  receipts, SLA disputes judged by validators or juries. But the trust root is *other
  parties' attestations and votes* — social consensus, sybil-vulnerable, and expensive
  precisely when it matters. A receipt says "the seller's machine says it ran."
- **zkML / TEE attestation** — proves *this model executed this inference*. It says nothing
  about whether the output satisfies the buyer's postcondition on the buyer's substrate.

Reputation is gameable at machine speed. Attestations are hearsay. Validators are a jury you
must trust. **The only trust mechanism that scales to machine speed is verification that is
cheaper than the work itself, executed by the party who bears the risk.** That is — exactly
and only — what a `Check` is.

### The prize

Trust layers of new economies are reliably the most valuable position in them: payments trust
built Stripe and Visa; signature trust built DocuSign; audit trust is a $200B+ industry. A
neutral settlement layer taking a 0.5–2% clearing fee on even a low single-digit percentage
of the projected agentic flow clears $1B+ in annual revenue — before warranties,
underwriting, or the refuter market. The beachhead (software-checkable B2B work: code, data,
infra, compliance — a $500B+ outsourcing market today) is precisely the slice where `Check`
already speaks the language.

## 4. The idea: Agora, and the PACT protocol

**Agora** is the federation layer: Astra nodes form a mesh where work products travel with
their proofs and money travels only behind passed checks. The transaction grammar is **PACT —
Proof-carrying Agent Contract Transactions**:

```
   Buyer node                                Seller node
      │  1. INTENT: objective + acceptance Check(s)   │
      │     + escrowed payment + verification         │
      │     environment manifest                      │
      │──────────────────────────────────────────────▶│
      │                                               │ 2. Seller's verified agentic
      │                                               │    loop does the work; every
      │                                               │    step already postcondition-
      │                                               │    proven locally
      │  3. DELIVERABLE BUNDLE: artifacts +           │
      │     claim + Check + evidence + minted         │
      │     lineage (depends_on) + signature          │
      │◀──────────────────────────────────────────────│
      │  4. Buyer SANDBOX materializes the bundle     │
      │     and RE-RUNS the Check locally             │
      │     (foreign check = JSON AST interpreted     │
      │     through buyer's own device layer)         │
      │                                               │
      │  5a. PASS → escrow releases, signed twin      │
      │      receipts, claim minted on BOTH ledgers   │
      │  5b. FAIL → escrow returns, seller's bonded   │
      │      stake slashed, refutation recorded       │
      │  5c. DISPUTE → adversarial attack round       │
      │      rented from the mesh (proof_round        │
      │      across nodes), stake decides             │
```

The trust root is the buyer's own hardware. No oracle, no jury, no platform vouching. The
seller is not trusted; the seller is *checked*.

### What federation makes newly possible (none of this exists anywhere)

**Settlement-by-re-execution.** The acceptance test is agreed *before* the work, as a
machine-checkable object, and payment is mechanically contingent on it passing on the buyer's
substrate. This inverts every existing marketplace: quality disputes stop being adjudicated
and start being *computed*.

**Reputation as capital, not stars.** A node's reputation is its stake history — earned only
by claims that survived checks and adversarial attack, burned on refutation. Sybils get
nothing from new identities: a fresh keypair has zero survived stake, and stake cannot be
transferred, only earned. This is the first sybil-resistance mechanism that costs attackers
work rather than money.

**The refuter-for-hire market.** `proof_round`'s attacker population, federated: any node can
earn by *breaking* others' claims before mint. Verification itself becomes a paid profession
of the mesh — a decentralized, incentive-aligned red team. Nothing like a market where
third parties earn by falsifying strangers' work products exists today.

**The impossibility exchange.** Astra already mints proven negatives ("X cannot be done under
Y"). Federated, this becomes the first market where *dead ends have a price*: pay once for a
proven impossibility instead of burning compute rediscovering it. Globally deduplicated
negative knowledge — no research economy has ever had this.

**Warranties as a product.** A sentinel subscription across nodes: "this claim is re-verified
hourly for 90 days; you are alerted the instant it drifts, and the bond pays out if it
breaks." One-time badges (SOC2, code review, benchmark claims) become continuously-enforced,
financially-backed guarantees. `genome::underwriting` prices the correlated-risk book.

**The compounding commons.** Every settled PACT mints a claim into both ledgers. The mesh
accumulates the largest corpus of machine-verified, adversarially-tested facts about software
reality in existence — the exact training substrate `noesis` needs to predict unrun checks.
The market's exhaust is a world model no one else can build, and it appreciates with volume.

## 5. Why this is one layer of new code, not a rebuild

| Market function | Already-built organ | The missing federation shim |
|---|---|---|
| Proof format | `verification::Check` (serde DSL) | canonical serialization + signature envelope |
| Safe foreign re-execution | sandbox monitor + Docker path | bundle → workspace materialization step |
| Escrow / slashing | `proof_economy` stake semantics | counterparty accounts + payment-rail adapter |
| Acceptance contract | `agentic_loop` postconditions | pre-agreed `Check` in the intent handshake |
| Receipts | `httpa` receipts | dual-signed settlement receipt |
| Disputes | `proof_round` attack rounds | cross-node attack RPC + bonded refuters |
| Warranty | `sentinel` drift ledger | subscription + payout binding |
| Underwriting | `genome::underwriting` | mesh-wide risk book |
| Fraud enforcement | governance army/police | peer ban-lists, slash propagation |
| Autonomous participant | `ceo` under governance | budget-bounded market wallet |

New surface, concretely:

1. **`crates/core/src/mesh/`** — node identity (Ed25519 keypair; the pubkey *is* the account),
   signed envelopes for claims/intents/receipts, peer registry with capability adverts.
2. **PACT handshake API** — `/mesh/v1/{offer,intent,deliver,verify,settle,dispute}` on the
   existing actix server; the same `ApiResponse` envelope.
3. **Verification Environment Manifest** — pinned container image digest, resource limits,
   network policy, materialized workspace layout. This is the honest answer to "deterministic
   on whose machine?": a `Check` is only tradable when its manifest makes both substrates
   equivalent. Checks that can't be manifest-pinned are not tradable — fail closed.
4. **Deliverable bundle format** — content-addressed tarball (artifacts) + claim JSON + check
   + evidence + lineage; hash-referenced by both receipts.
5. **Escrow adapter trait** — credits ledger first (no money-transmitter exposure), then
   stablecoin (x402-compatible — *ride* the payment rails everyone else built; PACT is the
   condition layer above them) and PSP adapters. Rail-agnostic by design.
6. **Bonded stake** — real collateral behind seller claims and refuter attacks; slash on
   refutation, propagated as a signed, re-verifiable slash record.

## 6. Honest boundaries (in this project's own voice)

- **Only checkable work is tradable.** PACT settles what reduces to a `Check`: code, data,
  infra state, crawls, compliance, reproducibility. It cannot settle taste ("write me a
  beautiful essay") — those markets keep human judges or conformal-bounded residuals
  (`genome` shows the pattern). This bounds the beachhead; it does not shrink it below
  hundreds of billions.
- **Determinism is scoped, not assumed.** No manifest, no trade. Flaky checks (network-
  dependent, time-dependent) must pin or quorum (N-of-M re-runs) — the manifest declares
  which.
- **A check proves the postcondition, not the absence of malice in artifacts.** Delivered
  code passing tests can still be hostile elsewhere. Buyer-side materialization is
  sandbox-jailed; supply-chain checks from `continuum` domain 2 (no-outbound-call
  impossibilities) become the standard hygiene add-on every buyer attaches. State this
  loudly in every receipt.
- **Adversarial checks are inert by construction** — a malicious *check* is a JSON AST run
  through the buyer's own gated device layer; the reference monitor treats it like any
  untrusted action. A malicious *manifest* (hostile container image) is refused unless
  image digests are on the buyer's allowlist.
- **Legal reality.** Escrowed value = money-transmission territory. Phase order exists for
  this reason: credits → licensed PSP/stablecoin adapters. The protocol never holds funds;
  adapters do.
- **Cold start.** Two-sided markets die empty. The seed liquidity is Astra itself: `continuum`
  verticals (reproducibility certificates, supply-chain impossibilities, compliance drift
  watches) are sellable on day one, and every self-hosted node is both producer and consumer.

## 7. Build sequence

- **Phase 0 — mesh + barter (weeks).** `mesh` crate, identities, signed claim exchange,
  credits ledger. Two nodes trade proofs with no money. The demo: node A sells node B a
  minted supply-chain impossibility; B re-runs the check locally and its ledger mints it.
- **Phase 1 — PACT settlement (months).** Intent handshake with pre-agreed checks, deliverable
  bundles, manifest-pinned re-execution, dual receipts, bonded stake + slashing.
- **Phase 2 — money + warranties.** x402/stablecoin escrow adapter; sentinel warranty
  subscriptions with bonded payout; refuter-for-hire attack rounds.
- **Phase 3 — the exchange.** Impossibility market, underwritten claim books, private meshes
  for consortia (supply-chain and compliance verticals sell themselves here), and the open
  PACT spec — publish the protocol, keep the reference node, clearing, and underwriting as
  the business. The Stripe playbook, one economy earlier.

## 8. The one-sentence pitch

Everyone else is teaching agents to *pay* each other; nothing on earth lets a stranger's
agent **prove its work before the money moves** — Astra is one federation layer away from
being that settlement standard, because it is the only system whose native unit of output is
already a portable, re-runnable proof.

---

*Grounding: Juniper Research agentic-commerce forecast ($8B 2026 → $1.5T 2030); McKinsey $3–5T
global agentic-commerce opportunity; Cisco enterprise survey (85% pilots / 5% production,
trust as top barrier); nearest-neighbor landscape: Skyfire (KYA identity), Coinbase x402
(HTTP-native stablecoin payments), Mastercard Verifiable Intent (intent audit trails),
Lyrie ATP (agent identity), VAGACHAIN (validator-judged SLA receipts) — none settle on
buyer-side re-execution of a machine-checkable proof of the work itself.*
