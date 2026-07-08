# Axiom — the proof-scheduled operating system

**The idea:** an operating system where **isolation is a tax you pay only when proof is
absent.** Linux charges every program the distrust tax forever — traps, context switches,
TLB flushes, copies, Spectre mitigations — because it can never know what code will do, so
it must guard it at runtime, on every operation, until the end of time. Axiom inverts this:
software arrives carrying machine-checkable claims in the proof economy, and the kernel's
dispatcher answers one question — *what has been minted about this code?* — to decide how
expensively it must be caged. Fully proven code runs uncaged: single address space, kernel
context, IPC as a function call, zero copies, no traps. **Trust, established by proof once,
buys speed forever — and drifts away the moment the proof breaks.**

This is where "faster than Linux" stops being marketing and becomes an engineering claim
with a mechanism: not by beating Linux at scheduling C code, but by *deleting the costs
Linux exists to impose*, for exactly the code that has proven it doesn't need them.

---

## 1. Why this is honest physics, not hype

The performance ceiling of a conventional OS is set by hardware-enforced distrust:

- A **syscall** is a privilege trap — hundreds of cycles before any work happens, worse
  since Meltdown/Spectre mitigations (KPTI) added page-table switches to every crossing.
- A **context switch** between processes costs microseconds — register state, address-space
  switch, TLB shootdown, cache pollution.
- **IPC** between two processes costs two traps + two switches + data copies across address
  spaces. This is why microservice-style software on Linux burns double-digit percentages
  of CPU on boundary crossings.
- Every **I/O buffer** is copied between user and kernel space because neither trusts the
  other's memory.

Every fast path in modern Linux is an *escape hatch from Linux*: kernel-bypass networking
(DPDK), `io_uring` (batch to avoid traps), eBPF (verified bytecode allowed to run *inside*
the kernel at native speed precisely because a verifier proved it safe). eBPF is the
smoking gun, deployed on a billion machines: **when safety is proven statically, the
runtime cage is removed, and verified code runs where unverified code never could.**

The research lineage proved the general form and then stalled:

- **Microsoft Singularity** (2003–2010): Software-Isolated Processes — everything in ring 0,
  one address space, isolation enforced by type-safety proofs instead of the MMU. IPC
  became a pointer handoff: orders of magnitude cheaper than process IPC. It worked. It
  died as a research artifact.
- **seL4**: the formally verified microkernel is *also the fastest* microkernel — proof and
  performance turned out to be allies, because verified invariants let you optimize
  fearlessly.
- **Theseus, Redox** (Rust OSes): memory-safe kernels, active but academic/hobbyist.

Why did the proofs-for-performance OS never ship? **Proof supply.** Singularity required
every component rewritten in Sing# and trusted its own compiler as the sole proof
authority. One vendor, one language, one verifier — proof production could not scale to an
ecosystem. That is the exact problem this project has already solved in another domain.

## 2. The new part: the proof economy is the missing half of Singularity

Astra contributes the piece no OS project has ever had — **an industrial process for
producing, attacking, pricing, and continuously re-verifying proofs about software,
performed by a market instead of a vendor**:

| Singularity's unsolved problem | Astra's existing organ |
|---|---|
| Who produces proofs for arbitrary software? | Agora/Noosphere: a paid, permissionless proof market (lemma markets, bounties) |
| Who checks the proofs aren't wrong? | `proof_economy`: adversarial staked attack before mint; refuters earn by breaking claims |
| What if a proof stops being true? | `sentinel`: continuous re-verification, drift ledger — to the second |
| Who pays for all this? | claims are assets: royalties up `depends_on` lineage (`genome::pricing`) |
| How does the OS trust a foreign proof? | local re-execution — checks are a constrained DSL re-run on *your* machine (PACT) |
| What about code that misbehaves later? | `governance` revocation + stake slashing + **tier demotion** (below) |

So the genuinely new composition — existing nowhere, not in Singularity, seL4, eBPF, or
any Rust OS — is:

> **Execution privilege as a live market price.** How fast your code runs is a function of
> what is currently proven about it, where proofs are minted adversarially, watched
> continuously, and revoked automatically. The kernel consumes the proof economy the way
> Linux consumes page tables.

## 3. Architecture: the three tiers

```
 ┌────────────────────────────────────────────────────────────────────┐
 │  TIER 0 — PROVEN            single address space, kernel context   │
 │  claims: memory-safe, capability-bounded, termination/resource-    │
 │  bounded. Cost model: syscall = function call; IPC = pointer       │
 │  handoff (zero-copy); no traps, no TLB flush, no KPTI.             │
 │  (Singularity SIPs + eBPF, generalized; Rust/wasm toolchains       │
 │   emit the claims; the mesh mints them.)                           │
 ├────────────────────────────────────────────────────────────────────┤
 │  TIER 1 — PARTIALLY PROVEN   lightweight hardware domains          │
 │  claims cover memory safety but not resource bounds (or vice       │
 │  versa). Cost model: MPK/CHERI-style domain switches — cheaper     │
 │  than address-space switches, dearer than calls.                   │
 ├────────────────────────────────────────────────────────────────────┤
 │  TIER 2 — UNPROVEN / LEGACY   the full Linux tax                   │
 │  classic process isolation; POSIX/Linux personality (starnix-      │
 │  style) or a compat VM. Runs everything, fast as Linux, never      │
 │  faster. The point: the old world still boots — it just doesn't    │
 │  get the discount.                                                 │
 └────────────────────────────────────────────────────────────────────┘
        dispatcher = proof-economy lookup   ·   demotion = sentinel drift
```

**Promotion** is earned: ship your component with claims (memory safety via Rust/wasm
verification toolchains, capability bounds as impossibility claims — "this module performs
no syscall outside {read, write on cap C}"), survive the refuter market, get minted, run in
Tier 0. **Demotion is automatic and continuous:** the sentinel re-attacks standing claims;
the moment one drifts — a CVE-class behavior appears in an update, a refuter finally breaks
an impossibility — the scheduler demotes the component to a slower tier *while it keeps
running*. Nobody has built this: **performance that degrades gracefully with trust instead
of security that fails catastrophically with speed.**

### The performance claims, stated checkably

For Tier-0-resident workloads (the honest scope of "better than Linux"):

- IPC-heavy composition (services, drivers, pipelines): boundary crossings drop from
  μs-scale process IPC to ns-scale calls — the Singularity result, now with a proof supply
  chain. This is the dominant win; modern software is mostly boundaries.
- Syscall-bound work: traps become calls; post-KPTI trap overhead disappears entirely.
- I/O: zero-copy end-to-end where buffer ownership is proven (linear-type handoff — the
  claim *is* the copy-eliminator).
- Intent batching: the syscall interface becomes goal-level intents with postconditions
  (`weave`/HTTPA pattern, `io_uring` generalized) — the kernel fuses and reorders
  operations because it knows the goal, and the postcondition check proves the fusion safe.
- Scheduling: `active_inference` as the scheduler's predictor — placement by predicted
  contention, profiling effort spent only where surprise is high. Real but modest; the tier
  mechanism is where the order-of-magnitude lives.

And the boundary, stated plainly: unproven code runs **no faster than Linux, ever**. The
system's aggregate speed is proportional to its proof coverage — which is precisely why
attaching the OS to a proof *market* matters: coverage compounds. The Noosphere flywheel
(bounties on the hottest unproven components, royalties to whoever proves them) is the
mechanism by which the OS gets faster *over time without new hardware*.

## 4. The massive OS problems this solves (beyond speed)

1. **The distrust tax** — the perpetual runtime cost of assuming all code is hostile.
   Solved by making trust provable, priced, and revocable. (This is the performance story.)
2. **Supply-chain blindness** — you cannot know what installed software does. In Axiom the
   *package format is the claim bundle*: behavioral guarantees and impossibilities, minted
   adversarially, watched for drift (`continuum` domain 2 as the package manager). "What
   does this update change about what the software can do?" becomes a queryable diff.
3. **Ambient authority** — the 1970s model where every process wields its user's full
   power; the model AI agents are about to detonate. Tier membership *is* capability
   confinement; agents are just Tier-1 processes whose claims say what they cannot do.
4. **Crash-and-pray reliability** — driver dies, kernel panics, human reads logs. Axiom
   routes failures through `crucible` (competing root-cause hypotheses, discriminating
   test) and `forge` (regenerate the component), gated by the ASC-II five-gate promotion
   pipeline with supervisor rollback — which is, notice, *already a working prototype of
   verified OS component hot-swap* in this repo.
5. **Configuration drift** — the system's intended state is a set of sentinel-watched
   checks; NixOS declares state, Axiom *proves* it continuously.
6. **Ossified evolution** — kernels can barely change (eBPF exists because changing Linux
   is too dangerous). An OS whose components carry proofs can accept aggressive changes
   from *anyone* — including its own Forge — because the gates, not the author's
   reputation, decide. The OS becomes self-improving under the same constitution as the
   rest of Astra (CEO 49% / governance 51% / kill switch).

## 5. Build path (each phase ships value alone)

- **Phase 0 — the Axiom runtime on Linux (months).** A userspace host: proven components
  (wasm/Rust, claims minted via the economy) co-scheduled in one process with zero-copy
  channels; unproven components in ordinary processes. Side-by-side benchmark vs. the same
  pipeline as Linux processes — *publish the boundary-crossing numbers*. This is a product
  by itself (a verified, self-healing service runtime) and it de-risks the physics claim.
- **Phase 1 — Axiom as guest OS (year).** Unikernel-style Tier-0 image under a VMM
  (crosvm-class); Linux runs beside it for everything legacy. Deploy target: dataplane
  services, agent sandboxes — the workloads that are all boundaries.
- **Phase 2 — bare metal (years, eyes open).** Rust microkernel (or seL4 as the verified
  nucleus) with the proof-tier dispatcher native; Linux personality layer (starnix-style)
  for compatibility; native drivers earn Tier 0 one by one via driver-claim bounties on
  the mesh. Drivers are the moat Linux keeps — this phase is honest about being a decade
  game, and Phases 0–1 don't need it to win.
- **Phase 3 — the mesh-native distro.** Package manager = Agora client; installing
  software = buying its claims; the refuter market is your permanent, paid red team; the
  Agon ranks components and vendors by survived proofs. The first OS whose security posture
  and performance profile are *live market data*.

## 6. Honest boundaries

- "Better than Linux" is **scoped to proof coverage**: Tier-0-heavy systems (service
  meshes, dataplanes, agent runtimes, embedded/appliance builds) see the order-of-magnitude
  boundary-crossing wins; a desktop full of unproven legacy code sees Linux-parity, plus
  the trust dividend. Axiom never makes unproven code faster.
- Proof production is the whole game. Memory safety comes cheap from toolchains
  (Rust/wasm); capability and resource bounds need the market to mature. If the mesh fails
  as an economy, Axiom degrades into "a nice verified runtime" — real floor, smaller story.
- Spectre-class side channels within a single address space are the serious open problem
  for software isolation (it's why the industry retreated to hardware after 2018). Tier-0
  admission must include speculation-safety claims (compiled-in mitigation evidence,
  constant-time properties for secret-handling code), and secret-holding components may
  *choose* Tier 1 — the tier system prices this honestly instead of pretending it away.
- The claims in §3 are stated so Phase 0 can falsify them cheaply. That is deliberate; it
  is how this project treats every claim.

## 7. Phase 0 — SHIPPED (`core/src/axiom/`)

The proof-scheduled runtime is now real code, wired at `/api/v1/axiom/*`:

- **`AxiomKernel`** (`axiom/mod.rs`) — the proof-tier dispatcher. A component
  registers with `claims` (economy claim ids classed as `memory_safety` /
  `capability_bound` / `resource_bound` / `speculation_safety`); every claim is
  mirrored as a Sentinel watch. `dispatch` is the proof-scheduled syscall:
  Tier 0 = uncaged in-process call (native intents + live Forge tools), Tier 1 =
  guarded domain (capability envelope + audit per call), Tier 2 = a real
  supervised OS process. Unproven in-process code is **refused**, not merely
  caged — in-process execution is a privilege earned in the economy.
- **Live demotion/promotion** — `POST /axiom/resync` re-reads the economy and
  re-samples every mirrored watch; tier moves land in the durable
  `axiom_transitions` ledger. A claim an adversary *refuted* quarantines the
  component until it re-proves itself.
- **The falsifiable numbers** — `POST /axiom/bench` runs the same no-op through
  all three mechanisms and reports ns/op plus the measured `cage_tax_ratio`
  (§3's claim, checkable on any machine).
- **`axsh`** (`axiom/shell.rs`) — the kernel's built-in command line
  (`POST /axiom/shell`): `uname · ps · tiers · claims · dispatch · register ·
  resync · drift · ledger · bench`, plus two honestly-priced escape hatches —
  `sh <cmd>` (raw Tier-2 caged process) and `claw <args>` (the Claw Code CLI as
  the agentic userland command, `ASTRA_CLAW_BIN`): the AI agent is just another
  Tier-2 program until its claims say otherwise.
- Kernel built-ins (`axiom.echo/time/sum/checksum`) are the explicit TCB —
  Tier 0 by construction; everything else earns its tier from minted claims.

## 8. Phase 2 seed — SHIPPED (`os/`, the from-scratch kernel)

A freestanding Rust kernel now lives at `os/` — `no_std`, zero-dependency
nucleus (own spinlock, own free-list allocator, own cooperative executor, own
shell) that compiles for bare-metal `x86_64-unknown-none` (`os/boot`, with a
from-scratch 16550 UART driver and TSC clock) and runs identically on a hosted
console (`os/host`, `cargo run -p axiom-host -- --demo`). It carries mechanisms
no mainstream kernel has:

1. **In-kernel trust ledger** — privilege from claims, demotion on drift while
   the task runs, tick-stamped transition log.
2. **Proof-gated cooperative scheduling** — proven tasks never pay the
   preemption tax; unproven tasks are throttled to every 4th round.
3. **Epistemic scheduler** — integer-EWMA burst prediction per task, attention
   steered by surprise; chronically surprising Proven tasks are flagged as
   drift suspects for re-verification. Scheduling as active inference.
4. **Tier-gated zero-copy IPC** — ownership-handoff messaging (measured ~10×
   cheaper than the copying boundary, before counting traps).
5. **Transactional intent syscalls** — op batches fused (dead-store
   elimination) and committed only after their postcondition is *proven* on a
   shadow; failed goals leave the world untouched.

See `os/README.md`. Phase 0 (§7) and this kernel meet in the middle: the
economy mints verdicts, the nucleus consumes them.

The kernel has since grown the subsystems that make it a real OS rather than a
scheduler demo: a **physical frame allocator** over the bootloader memory map
(`nucleus/src/mem.rs`), a **block layer** (`block.rs`, the SPU's persistent
tier), a **virtual filesystem** with path resolution, files/directories, file
descriptors, and snapshot/restore that survives power loss (`fs.rs`), and on
bare metal a from-scratch **interrupt stack** — IDT, 8259 PIC, PIT timer
(the hardware clock and preemption substrate), and PS/2 keyboard, all built as
naked-function ISRs (`boot/src/interrupts.rs`).

It then grew the layers that make it run *programs*: **4-level x86_64 paging
and per-process address spaces** (`vmem.rs`, with W/U/NX permission bits and
hardware-enforced isolation), an **ELF64 loader** (`elf.rs`), a **process
model whose capabilities are derived from its proof tier** (`process.rs` — a
proven process runs uncaged, an unproven one runs caged; authority from proof,
not uid), and the arch **context-switch + `CR3` load** that enacts scheduling
on metal (`boot/src/switch.rs`).

Most recently it grew the **userspace boundary** — the place the proof→authority
thesis is *enforced*: a **capability-checked syscall layer** (`syscall.rs`)
where each call is gated by the caller's proof-tier capabilities (an unproven
process is refused the very `write`/`spawn` a proven one is granted), the
higher-half **kernel sharing** every address space needs for a trap to land in
kernel code (`vmem::share_higher_half`), and the bare-metal **ring-3 machinery**
— GDT + TSS + the `syscall`/`sysret` MSRs and entry stub (`boot/src/gdt.rs`).

And a layer that exists nowhere else: **operator primitives** (`ops.rs`) — six
axsh commands that exploit the transactional + causal substrate to delete
classes of terminal problem Unix cannot. `ensure` (declarative, idempotent,
proven, atomic convergence), `undo`/`redo` (a reversible terminal — every
mutation journals its pre-image), `dry` (preview with a structural zero-touch
guarantee), `why` (causal history from the ledgers, not log archaeology),
`prove`/`recall` (proof-memory — a checked fact is never re-checked), and
`when <pred> do <cmd>` (reactive triggers instead of poll loops).

And **preemptive multitasking where the quantum is a function of proof**
(`preempt.rs`): proven code earns a long time-slice (it proved it won't hog the
CPU), unproven code a short one and is preempted aggressively — so a runaway in
an unproven process *cannot* freeze the machine, while proven work runs nearly
uninterrupted. Faster (fewer context switches for proven work) and safer
(unproven work contained) than every OS's flat quantum. The PIT timer drives it
on bare metal; `top`/`quantum` expose it. 72 kernel-core tests; the bare-metal
image builds warning-free for `x86_64-unknown-none`.

## 9. One sentence

Linux spends hardware forever guarding code it can never trust; **Axiom lets code buy its
way out of the cage with proofs** — minted by a market, watched by the sentinel, revoked on
drift — making the operating system's speed, for the first time, a compounding function of
how much of its software has been proven true.
