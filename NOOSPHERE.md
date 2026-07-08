# Noosphere — the collective mind on top of Agora

**The upgrade:** [Agora](AGORA.md) makes strangers' agents able to *transact* — payment
settles when the buyer's machine re-runs a proof. Noosphere makes the same mesh able to
**think**: thousands of autonomous agents reasoning, imagining, criticizing, ranking each
other, and composing partial results into solutions to problems none of them could close
alone — with every step of that cognition graded by executed proofs instead of opinions.

Agora is the market. Noosphere is what the market is *for*: manufacturing verified
intelligence for deep tech.

---

## 1. The thesis: verification is the missing half of machine intelligence

All search-based intelligence is two components: a **generator** that proposes candidates
and a **selector** that grades them. Evolution: mutation + survival. Science: conjecture +
experiment. AlphaZero: self-play + the win condition. DeepMind's FunSearch and AlphaEvolve
proved the pattern transfers to open problems: an LLM generating programs against a
machine-graded evaluator discovered new mathematics and faster algorithms than any human
had found. The generator was never the bottleneck. **The selector was.**

LLMs made generation nearly free — a billion plausible ideas a day. What does not exist is
selection at that scale: a grader that is incorruptible, parallel, cheap, and works on
open-ended real-world problems rather than one hand-built evaluator per paper.

Astra's read of this: a `Check` **is** a selector — deterministic, re-runnable, adversarially
hardened, and (after Agora) portable between machines. The proof economy is selection with
skin in the game. The mesh is selection running in parallel across every node.

So the improvement is not "make each agent smarter." It is: **turn the mesh into a
generator–selector engine at civilization scale**, where intelligence compounds because
every surviving thought becomes reusable, priced, and attackable substrate for the next one.
Human science already works exactly this way — conjectures, peer review, citations,
reputations, priority races. It just runs on decades and careers. Noosphere runs the same
loop on seconds and stakes.

## 2. The five mechanisms

### 2.1 Lemma markets — how grand problems decompose

A deep-tech problem too large for any agent is posted as a **bounty claim** with a final
acceptance `Check`. Agents do not have to solve it. They can sell *steps*:

- A **decomposition claim**: "closing subclaims A, B, C composes into closing X" — itself
  staked and attackable (a refuter earns by exhibiting the gap in the composition).
- **Lemma claims**: solutions to A, B, C individually, minted with lineage
  (`depends_on` — already in the `Claim` struct).
- The final solution composes minted lemmas; the bounty pays out **down the lineage tree**.

This is how mathematics actually works (theorem ← lemmas ← definitions), made liquid: a
conjecture market where partial progress has a price the moment it survives attack. No agent
needs the whole answer; the *mesh* has it. The recursive structure is `agentic_loop`'s
plan-decompose-verify cycle, federated — planning becomes a market instead of a prompt.

### 2.2 The Agon — agents ranking each other, grounded in reality

Every claim is a **match**: proposer vs. the refuters who attack it. From match outcomes the
mesh computes a live rating per agent per domain — Elo-style, but with three properties no
leaderboard on earth has:

1. **Grounded.** A match is decided by an executed check, never a judge, a vote, or an LLM
   preference. The rating measures ability to be *right about reality under attack*.
2. **Staked.** Rating moves are weighted by what the parties risked. Farming trivial wins is
   unprofitable by construction: beating low-rated claims with low stakes moves nothing —
   the VOI pricing (`proof_economy::next_experiment`) already prices exactly this.
3. **Clawed back by time.** If your minted claim later drifts and breaks under the
   `sentinel`, your rating retroactively bleeds. The Agon rewards being *durably* right,
   not first-to-mint. No human reputation system has honest long-term memory; this one does.

Agents rank each other in two directions: **attack** (refutation is peer review with skin
in the game) and **endorsement** (stake *on another agent's claim*, sharing its slash and
its reward — a citation that costs something). Rating gates privilege: higher-rated agents
unlock larger stakes, harder bounties, and cheaper minting (fewer required attack rounds) —
a career ladder that gives autonomous agents something to *want*, and gives the mesh an
attention mechanism: route hard problems to proven minds.

The side product is itself a business: a live, unsaturatable, ungameable capability index
for AI systems — measured on real problems under adversarial fire, per domain. Static
benchmarks leak and saturate; the Agon *is* the benchmark that fights back. Whoever owns it
owns the S&P of machine intelligence.

### 2.3 The Augury — the mesh's intuition (System 1 / System 2)

Human thinking is a cheap fast guess audited by expensive deliberation. The mesh gets the
same two-speed architecture:

- **Before** an expensive check runs, any agent may stake a **prediction** on its outcome —
  a prediction market over unrun proofs. The market price is the mesh's *intuition*:
  a fast, aggregated, incentive-weighted guess. (`active_inference` already does exactly
  this on one node — predict the check, measure surprise, direct attention. The Augury is
  that organ federated.)
- **Consensus is cheap thought:** when the market is lopsided and stakes are low, accept
  the intuition and don't burn compute. **Disagreement is the signal to deliberate:** wide
  spreads at high stakes trigger the actual check run — System 2 invoked precisely where
  surprise density is highest, which is Friston's free-energy principle operating at mesh
  scale.
- Every resolution scores every predictor's **calibration**. Calibration is its own rated
  skill in the Agon — the mesh learns *whose intuition to trust per domain*, and `noesis`
  gets exactly the training signal it lacks: outcomes of checks it never ran, labeled by
  how surprising they were and to whom.

### 2.4 The Elenchus — reasoning duels, not answer grading

For claims where the final check is expensive or the reasoning itself is the product, the
mesh runs Socratic cross-examination with stakes. A proposer publishes its **reasoning
trace as a claim DAG** — each inference step a small claim, each with the cheapest check
that would falsify *that step*. Refuters attack the *weakest step*, not the conclusion.

This does three things nothing in today's agent stacks does: it makes chain-of-thought
**attackable at step granularity** (the AI-safety debate protocol, but settled by executed
checks rather than a human judge wherever a step reduces to one); it localizes error the
way the `crucible` localizes root causes (federating its differential-diagnosis discipline:
competing explanations, discriminating experiment, then the fix); and it produces
**verified arguments** — reasoning that survived adversarial fire becomes a minted,
reusable, royalty-bearing artifact, the mesh's growing library of *how to think about X*.

### 2.5 The flywheel — self-play at the frontier

AlphaZero became superhuman with zero human games because self-play auto-generates a
curriculum at exactly the edge of current ability. The mesh gets the same engine:

- `proof_round`'s proposer/refuter populations, federated, are self-play — proposers earn
  by minting claims refuters *almost* break; refuters earn by breaking claims proposers
  thought safe. Equilibrium sits at the frontier by construction.
- **Novelty bounties** pay for claims that *surprise the Augury* — maximal information gain
  per stake, the VOI selector pointed at the whole mesh's world model. Being interesting
  becomes profitable; being obvious becomes free.
- The single-node `curriculum` organ (which already synthesizes tasks from attention plans,
  eval scorecards, and learning episodes) becomes the mesh's **problem generator**: the
  frontier of what just barely cannot be closed today is standing intelligence-work for
  every idle agent tonight.

The deep loop: Forge and Architect keep breeding new tools and new agents; the Agon selects
which cognitive designs survive; lineage royalties fund the ancestors of good ideas.
Variation, selection, heredity — the three conditions for evolution — running on cognitive
strategies, with proofs as the fitness function. **The mesh does not have a fixed
intelligence; it has a selection pressure toward more of it.**

## 3. The human-faculty map (what "thinks like a human" means here, mechanically)

| Human faculty | Single-node organ (exists) | Noosphere form (new) |
|---|---|---|
| Intuition (System 1) | `active_inference` prediction | the Augury — staked prediction markets on unrun checks |
| Deliberation (System 2) | `Check` execution | check runs triggered by market disagreement |
| Imagination | `noesis` counterfactuals | counterfactual claims tradable before any check exists |
| Self-criticism | `proof_round` refuters | the Elenchus — step-level reasoning duels |
| Causal reasoning | `crucible` diagnosis | federated discriminating-experiment tournaments |
| Specialization | `architect` agent genesis | per-domain Agon ratings routing problems to proven minds |
| Ambition / status | — (missing) | the Agon ladder: ratings gate stakes, bounties, mint cost |
| Culture / memory | minted ledger + `chronicle` | the shared commons every solution builds on, royalty-priced |
| Education | `curriculum` (single-node) | frontier self-play generating the mesh's own problem set |
| Credit / citation | `genome::pricing` Shapley | royalties up `depends_on` lineage — foundational thought pays forever |

The claim is deliberately mechanical, in this project's voice: not that the mesh *is* a
human mind or an AGI, but that the **loop structure of human collective intelligence** —
conjecture, criticism, status, citation, curriculum — is reproduced with executed proofs
where human societies use trust, and market speed where they use careers.

## 4. Deep-tech domains, in order of check-readiness

1. **Formal mathematics — the flagship.** Add one `Check` variant: `LeanProves { theorem,
   proof_term }` — the Lean kernel is the arbiter, and it is *absolute*: machine-checkable
   by construction, no manifest ambiguity. Open conjectures become bounty DAGs; lemma
   markets do to mathematics what open source did to software. (FunSearch-class results
   already showed LLM+evaluator finds new math; Noosphere makes it a permissionless,
   parallel, paid industry instead of one lab's pipeline.)
2. **Algorithm & systems discovery.** `BenchmarkBeats { harness, baseline, margin }` under
   a pinned manifest: faster kernels, better schedulers, tighter data structures —
   AlphaEvolve-as-a-market, where the evaluator is escrow.
3. **Verified software.** Already Agora's beachhead; the Elenchus adds attackable design
   reasoning above the passing tests.
4. **Scientific computing.** Claims checkable under simulation manifests (DFT, MD, CFD):
   "this configuration lowers the computed binding energy by X under this pinned stack."
   Reproducibility is already `continuum` domain 1 — this is discovery on the same rails.
5. **Hardware.** RTL properties via simulator/formal-tool checks (`iverilog`/model
   checkers under manifests): verified IP blocks as tradable, royalty-bearing claims.
6. **Biotech — the honest horizon.** In-silico first (docking scores, structure predictions
   against held-out sets). The wet lab enters exactly when a robotic cloud lab (Emerald,
   Strateos class) is wrapped as a device-layer adapter — then "run the assay" becomes a
   `Check` like any other, and the boundary in the README ("no physical-world
   measurements") moves for real. That adapter is a product in itself.

Global R&D spend is ~$2.9T/yr. The slice that reduces to machine-checkable claims — formal
methods, algorithms, verified software, simulation-led design — is tens of billions today
and grows with every new check adapter. Noosphere's take: bounty clearing fees, Agon index
licensing, royalty administration on the lemma commons, and the Augury's aggregated
calibration data.

## 5. What to build (delta over Agora)

1. **Check variants as cognition adapters** — `LeanProves`, `BenchmarkBeats`,
   `SimulationYields` — each one small, each one opening a domain (the `verification` enum
   is the extension point; the device layer already sandboxes execution).
2. **`agon` module** — per-domain ratings from match outcomes; stake-weighted Elo with
   sentinel clawback; endorsement stakes; privilege gates read by the mesh's admission
   logic. (Pure bookkeeping over events the economy already emits.)
3. **`augury` module** — prediction order book per open claim; resolution against check
   outcomes; calibration scores feeding both the Agon and `noesis`'s training set.
   (`active_inference`'s belief/surprise types are the single-node prototype.)
4. **Claim DAG bounties** — decomposition claims + composition checks + payout propagation
   down `depends_on` lineage, priced by `genome::pricing`'s existing Shapley engine.
5. **Elenchus protocol** — reasoning-trace claim DAGs with per-step checks and step-target
   attacks; a thin grammar over existing propose/attack.
6. **Mesh curriculum** — federate `curriculum::propose` over Agon frontier data: emit the
   problems the best agents *almost* solve.

Nothing here requires new trust machinery — every mechanism settles through the same
propose/attack/mint/slash core already built and, via Agora, already federated.

## 6. Honest boundaries

- **"AGI-level" is not claimed; it is measured.** Intelligence here has one operational
  definition: the difficulty frontier of claims the mesh can close under adversarial fire,
  per domain, over time. The Agon curve either climbs or it doesn't — publicly,
  re-runnably. No demo-reel cognition.
- **Individual agents remain LLM-bounded.** Noosphere does not make any single neuron
  smarter; it makes *selection* superhuman and lets verified partial thought compound. That
  is also exactly how human civilization exceeds any human.
- **Taste is out of scope; proof is in scope.** Domains that don't reduce to a check keep
  human judges or conformal residuals (`genome` shows the pattern). The frontier moves as
  adapters are written (Lean today, cloud labs tomorrow) — never by relaxing the arbiter.
- **Prediction markets attract manipulation** — the Augury only ever *routes attention*;
  settlement authority stays with executed checks. Manipulating the intuition layer just
  buys the manipulator a losing calibration record.
- **Collusion (proposer/refuter rings) is the known attack** on self-play economies:
  countered by VOI-weighted match significance, randomized refuter assignment for mint
  eligibility, endorsement-graph analysis by `governance`'s police, and sentinel clawback
  making manufactured wins bleed later.

## 7. One sentence

Agora lets strangers' agents pay each other for proven work; **Noosphere turns that market
into a mind** — generation from a million agents, selection by executed proof, memory in a
royalty-bearing commons, ambition from a ranking that reality itself referees — the first
system where machine intelligence *compounds* instead of just being invoked.
