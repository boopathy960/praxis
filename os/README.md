# Praxis OS — the proof-scheduled operating system, from scratch

A freestanding Rust kernel (see `../PRAXIS.md` for the full thesis). Three crates.

**What runs, verified live on (emulated) hardware:** boots on bare-metal
x86_64; discovers every CPU core via **ACPI/MADT** and **starts the application
cores for real** (INIT-SIPI-SIPI → a from-scratch 16→32→64-bit trampoline →
each core reports in: `3/3 application cores online`) — and each stays
genuinely, independently running afterward: `cpus` reads live per-core tick
counters that visibly advance between calls, proof of real ongoing parallel
execution, not a one-time check-in; drops to **ring 3 and
runs a real ELF program loaded from its own VFS**, servicing `syscall` traps;
keeps files **durable across power cycles with a crash-safe journal**; talks to
the real internet over a from-scratch **e1000 + TCP/IP stack with
retransmission, flow control, Reno congestion control, and TIME-WAIT**, both
fetching pages and **serving them** (host `curl` → kernel httpd); carries a
complete, RFC-vector-verified **crypto suite** (SHA-256, HMAC, HKDF,
ChaCha20-Poly1305, X25519 — the TLS 1.3 primitive set); and adds **virtio-blk**,
an **ATA disk driver**, and **demand paging** — the same `no_std`,
zero-dependency nucleus running on x86_64 **and** AArch64.

Three crates:

| crate | what it is |
|---|---|
| `nucleus/` | The kernel core. `no_std`, **zero dependencies** — its own spinlock, its own free-list heap allocator, its own cooperative future executor, its own shell. Everything below is implemented here. |
| `boot/` | The bare-metal image: `x86_64-unknown-none`, `bootloader_api` handoff, from-scratch 16550 UART driver, **IDT + 8259 PIC + PIT timer + PS/2 keyboard** (all built from scratch as naked-function ISRs), the physical frame allocator wired to the bootloader memory map, `hlt` idle loop, praxsh on serial. No host OS beneath it. |
| `host/` | The same nucleus driven from a std console, so you can run the kernel core on any machine today. The platform surface is identical: a clock and a console. |

## The mechanisms (what Linux doesn't have)

1. **Trust ledger** (`nucleus/src/proof.rs`) — execution privilege is live,
   revocable state computed from claims (memory safety, capability bounds,
   resource bounds), not a uid. Drift demotes a task *while it runs*; every
   privilege move is tick-stamped in the transition ledger.
2. **Proof-gated cooperative scheduling** (`nucleus/src/sched.rs`) —
   preemption machinery exists because kernels can't trust code to yield.
   Proven tasks never pay that tax; unproven tasks are throttled to every
   4th round — the distrust tax, priced explicitly instead of levied on all.
3. **Epistemic scheduler** (same file) — the scheduler keeps an integer-EWMA
   predictive model of each task's burst and measures its own *surprise*
   (prediction error) per poll. Attention flows toward surprise; a Proven task
   that chronically defies its model is flagged (`suspects`) as a drift
   signal for re-verification. Scheduling as active inference.
4. **Tier-gated zero-copy IPC** (`nucleus/src/ipc.rs`) — a message is an
   ownership handoff of the allocation itself: no copy, no trap, no context
   switch, and Rust's move semantics prove the sender can't touch it after
   send. The fast path is a proof privilege; unproven senders are denied.
5. **Transactional intent syscalls** (`nucleus/src/intent.rs`) — a syscall is
   a goal: an op batch plus a machine-checkable postcondition. The kernel
   fuses the batch (dead-store elimination, append coalescing — legal because
   it knows the goal), applies it to a shadow, **proves the postcondition
   before commit**, and rolls back wholesale on failure. No syscall ever
   half-happens.
6. **The Kairos index** (`nucleus/src/kairos.rs`, full derivation in
   **[KAIROS.md](KAIROS.md)**) — risk-adjusted generalized-cµ scheduling in
   pure integer math: `K = (V·A + Q·1000)·1000 / (b̂ + û + 1000)`. Backlog in
   the numerator inherits max-weight throughput optimality; the predicted
   burst in the denominator inherits the cµ flow-time collapse (**measured
   4.86× / 79.4% mean-latency reduction** on the heterogeneous benchmark);
   and the model's own prediction error `û` in the denominator is the new
   part — predictability itself becomes CPU currency, so a task drifting
   from its proven model pays in its own scheduling index before the
   verification layer even reacts. praxsh: `kairos` / `kairos push <id> <units>`.

## The SPU executive (`nucleus/src/spu/`)

The kernel also carries an executive co-designed for the **State Processing
Unit** — the memory-centric reasoning chip ("SPU chip.pdf": persistent agent
contexts, three-tier substrate, FeMAC + p-bit fabric, Janus tiles,
Metacognition/Consolidation engines, State-Graft). The chip is
simulator-first; the OS is built the same way, against a device model, driven
by six new formulas derived in **[SPU-OS.md](SPU-OS.md)**:

1. thermodynamic context placement (value density `ρ = H·ΔW/S`)
2. the Wear-Pacing Invariant (`writes ≤ N_w·t/L + slack` — endurance as quota)
3. the metacognitive stopping rule (`n* = z²·p̃(1−p̃)/θ²` — answer / continue / escalate)
4. the graft threshold (`n₀` where COW state-graft beats quadratic re-prefill)
5. the Economic Switch Quantity (`B* = √(2λ·t_sw/c_d)` — think/explore batching)
6. consolidation utility (`U = Σ novelty·success`, wear-gated permanence)

praxsh commands: `spu · ctx · graft · reason · modes · consolidate`.

## The storage & memory stack (`nucleus/src/{mem,block,fs}.rs`)

A real OS needs to remember things and manage physical memory:

- **Physical frame allocator** (`mem.rs`) — a bitmap allocator over the
  bootloader's memory map: `allocate`/`allocate_contiguous`/`deallocate`,
  exact accounting, reserved-region overlap handling. On bare metal it is fed
  the real map; `frames` reports it.
- **Block layer** (`block.rs`) — a `BlockDevice` trait (the boundary a real
  NVMe/virtio driver would implement) with a `RamDisk`, modeling the SPU's
  non-volatile persistent tier.
- **ATA disk driver** (`ata.rs`) — a REAL disk now sits behind that boundary:
  classic ATA PIO on the legacy primary channel (IDENTIFY DEVICE, READ/WRITE
  SECTORS with 28-bit LBA, CACHE FLUSH after every write). Portable core
  behind an `AtaPorts` trait, host-tested against a software drive model
  (same pattern as the e1000); the boot crate supplies four `in`/`out`
  instructions (`boot/src/disk.rs`). At boot the kernel probes the
  primary-slave data disk, **restores the filesystem from the last power
  cycle**, and re-targets `sync` at the real disk. Verified live in QEMU
  across two full power-offs: boot #1 found a blank disk and recorded
  itself; boot #2 printed `fs: restored from disk — 5 nodes survived the
  power cycle` and `persistence: boot #2 recorded`. Files on Praxis are
  durable.
- **Virtual filesystem** (`fs.rs`) — an inode VFS: a hierarchical namespace,
  path resolution, files and directories, file descriptors with byte offsets,
  `read/write/seek/mkdir/readdir/unlink/stat`. `snapshot`/`restore` serialize
  the whole tree to a block device and back, so the filesystem **survives
  power loss** exactly as it would on the NV tier.

praxsh commands: `frames · ls · stat · mkdir · rm · cat · write · sync · fsck`.

## Processes, paging & isolation (`nucleus/src/{vmem,elf,process}.rs`)

The jump from "kernel with subsystems" to "OS that runs programs":

- **Virtual memory** (`vmem.rs`) — faithful x86_64 4-level page tables
  (PML4→PDPT→PD→PT) over the frame allocator: `map`/`unmap`/`translate` with
  writable/user/no-execute permission bits enforced per access. Each address
  space has its own root frame — the value that goes into `CR3`. Two spaces
  mapping the same virtual address to different frames **cannot see each
  other's memory**.
- **ELF64 loader** (`elf.rs`) — parses and validates real `x86_64` ELF
  executables, extracting `PT_LOAD` segments (with `.bss` zero-fill) and the
  entry point.
- **Processes** (`process.rs`) — each process loads an ELF into its own
  isolated address space and gets a **capability set derived from its proof
  tier**: a *proven* process runs uncaged with device + spawn rights; an
  *unproven* one runs but is caged (read-only, no spawn, no raw hardware).
  Authority is a function of proof, not of a uid — the ambient-authority fix
  the whole project argues for, now at the process level. A tier-aware
  scheduler picks who runs next (unproven work pays the same distrust tax).
- **Context switch** (`boot/src/switch.rs`) — the naked-asm register
  save/restore and `CR3` load that *enact* a scheduling decision on real
  hardware.

praxsh commands: `exec · proc · pmap · sched · kill`. In the demo, two programs
loaded at the same virtual address get different `CR3` roots and tier-split
capabilities — isolation and confinement, live.

### Ring 3 for real (`boot/src/usermode.rs`)

The privilege boundary is not just modelled — the kernel **drops to CPL 3 and
runs actual user code on the metal**. `usermode::launch` maps a hand-assembled
user program and its stack straight into the live page tables (setting the
`USER` bit at every level of the walk), `iretq`s into ring 3, and services the
`syscall` traps the program makes: a `WRITE` copies a user buffer to the
console, an `EXIT` `longjmp`s back into the kernel so boot continues. The
syscall path switches to a dedicated kernel stack on entry and returns via
`iretq` through the correctly-ordered GDT descriptors. Verified live under
QEMU — the serial shows `[ring3] hello from userland: this code runs at CPL 3`
printed by ring-3 code through a syscall, then a clean return to ring 0.
(Getting here surfaced a real bug: `syscall` clears the interrupt flag and the
`longjmp` back to the kernel didn't restore it, so the idle loop would later
freeze at `hlt` — fixed by re-enabling interrupts after the excursion.)

## The syscall boundary — proof as enforced authority (`nucleus/src/syscall.rs`)

The door between a process and the kernel, and the place the whole thesis is
*enforced*. Every other OS answers "may you?" with a uid or a label — ambient
authority a process carries because of **who ran it**. Praxis asks **what has
been proven about this code?**: each syscall checks the capability its caller's
proof tier granted at load. In the demo, a proven process and an unproven one
issue the *same* `write` and `spawn` calls — the proven one succeeds, the
unproven one is refused with `PermissionDenied(FsWrite)` / `(Spawn)`, and both
may still `read`. Authority is a function of proof, not identity.

On bare metal this rides real hardware: `boot/src/gdt.rs` builds the GDT (ring-0
kernel + ring-3 user segments), a TSS, and the `syscall`/`sysret` fast path
(the `STAR`/`LSTAR`/`FMASK`/`EFER.SCE` MSRs) with a naked entry stub; and
`vmem::share_higher_half` maps the kernel into every process address space so a
trap taken in user space lands in real kernel code.

praxsh command: `sys <pid> <call> …`.

## Preemptive multitasking — the time-slice is a function of proof (`nucleus/src/preempt.rs`)

Preemption exists because you can't trust code to yield: a timer interrupt
takes the CPU back so one runaway can't freeze the machine. Every OS levies
that as a *flat* tax — the same quantum for everyone. Praxis prices it by proof:

- **Proven** code carried a termination / resource-bound proof, so it *proved*
  it won't hog the CPU → it earns a long slice (10 ticks) and runs nearly
  uninterrupted.
- **Partial** → medium (5). **Unproven** → short (2), preempted aggressively.

So a runaway `while true {}` in an **unproven** process *cannot hang the
system* — it is forced off the CPU every couple of ticks — while a proven
compute kernel runs almost straight through. The demo (`exec` three tiers,
then `quantum 400`, then `top`) shows the proven thread taking ~60% of the CPU
and the unproven runaway ~12%, each preempted the same number of turns: trust
buys uninterrupted CPU, distrust is contained. It's *both* faster (proven work
takes fewer context switches) and safer than a flat quantum.

On bare metal the PIT timer drives `preempt.on_tick()` each interrupt; a
returned `Preempted { from, to }` is where `switch::switch_context` fires.

praxsh commands: `top` (proof-weighted scheduler view) · `quantum [n]` (advance
the RT scheduler live).

## SMP — starting every core for real (`nucleus/src/{acpi,lapic}.rs`, `boot/src/smp.rs`)

Discovering cores (ACPI/MADT) is only half of SMP; the other half is making a
*second* core execute your code. Praxis does both:

- **`nucleus/src/acpi.rs`** walks RSDP → RSDT/XSDT → MADT and enumerates every
  Local APIC entry — one per logical CPU, with its APIC id and enabled flag —
  behind a `PhysMem` trait, so the walk is unit-tested on the host against a
  synthetic firmware image.
- **`nucleus/src/lapic.rs`** encodes the INIT and STARTUP inter-processor
  interrupts (the Interrupt Command Register fields, per the SDM) behind a
  `LapicMmio` trait, also host-tested against a recording mock.
- **`boot/src/smp.rs`** is the bare-metal glue: a **from-scratch AP
  trampoline** — a page of hand-written `.code16` → `.code32` → `.code64`
  assembly, position-fixed at physical `0x8000` — that an application core
  executes after INIT-SIPI-SIPI. It walks itself through protected mode into
  64-bit long mode on the *same page tables* the boot core built (after the
  boot core identity-maps the trampoline page so it survives enabling paging),
  switches to a private stack, and jumps into Rust.

Bring-up is **best-effort and bounded**: each core gets a deadline to check
in; a core that never answers simply isn't counted, so the boot core can never
hang on hardware that didn't respond.

Once online, an AP does not just check in once and park — it spins
incrementing its own tick counter **forever**. `cpus` prints those counters
live; running it twice and watching every online core's number advance
(verified live under `-smp 4`: three independent counters climbing by
millions between two calls, seconds apart) is the actual proof of SMP —
genuinely, continuously independent execution, not a one-shot handshake.

praxsh command: `cpus` (topology + live per-core tick counters).

## Operator primitives — command lines no Unix has (`nucleus/src/ops.rs`)

A shell exists to shrink the gap between what a person wants and what the
machine is. Unix shells make that gap huge and self-inflicted: commands are
imperative and half-succeed, irreversible, opaque about what they'll do,
un-queryable about why things are, and forgetful (you re-run the same check
forever). Praxis's transactional + causal substrate lets the shell *delete*
those problems, not paper over them. Six commands that reduce real problems,
complexity, and time:

| command | the terminal problem it kills |
|---|---|
| `ensure <key> <value>` | imperative scripts that half-succeed. Declarative, transactional (commits only once its postcondition is **proven**), and **idempotent** — re-running when it already holds is an instant no-op. |
| `undo` / `redo` | irreversibility — the scariest property of every shell. Every mutation journals its pre-image; step the whole terminal backward and forward in time. |
| `dry <key> <value>` | not knowing what a command *will* do. Previews the change with a **zero-touch guarantee** — the function has no write path, so it *cannot* alter the world. |
| `why <kv \| task \| proc>` | "why is it like this?" = log archaeology. Answered from the kernel's causal ledgers, recorded at the moment of change. |
| `prove <pred>` / `recall <pred>` | recomputing the same check forever. Proof-memory: a checked fact is cached with the tick it was proven and recalled instantly. |
| `when <pred> do <command>` | poll-sleep loops. A reactive trigger fires a command the instant a predicate becomes true — no polling. |

Predicate grammar (shared by `prove`/`recall`/`when`): `kv:<k>=<v>` ·
`kv:<k>~<text>` · `file:<path>` · `file:<path>~<text>`.

## Networking — a real stack, egress gated by proof (`nucleus/src/net.rs`)

A real OS talks to the world. Praxis ships a freestanding, zero-dependency
Ethernet / ARP / IPv4 / ICMP / UDP / **TCP** stack: a `NetDevice` boundary (the
line a NIC driver sits behind) with a loopback NIC, frame parsing,
an ARP resolver + cache, IPv4 with the RFC 1071 ones-complement checksum, ICMP
echo (ping), UDP datagrams demultiplexed to bound sockets, and a real **TCP**
transport — the three-way handshake, in-order data with cumulative ACKs, and the
FIN close (`nucleus/src/tcp.rs`, a pure state machine the stack drives).

The transport is **loss-hardened**: every SYN, SYN-ACK, data, and FIN segment
sits in a per-connection retransmission queue until the ACK that covers it
arrives; a clock-driven `tick` (pumped by the idle loop, like everything else)
re-emits overdue segments verbatim with an exponentially backing-off RTO
(50 ticks base, doubling per retry) and honestly kills the connection after 6
fruitless retries rather than leaving a zombie. The receive side answers
duplicate data, retransmitted SYN-ACKs, and retransmitted FINs with fresh
resync ACKs — so a lost ACK in either direction never wedges a good
connection. Tested by deterministically eating frames off the wire (the pump
harness makes packet loss a one-liner) and watching the transfer complete
anyway.

It also does **real flow control**: each connection has a bounded receive
buffer, and every ACK advertises the *true* free space left in it — a window
that shrinks as unread data piles up, reaches zero when the buffer is full
(telling the peer to stop), and reopens with a window-update ACK when the
application drains it. The sender tracks the peer's advertised window and
refuses to transmit into a zero window. A misbehaving peer that ignores the
window is clamped at the buffer bound rather than allowed to allocate without
limit.

And a **real NIC driver now sits behind that boundary**: an Intel e1000
(82540EM) driver (`nucleus/src/e1000.rs`) — software reset, MAC from RAL/RAH
or the EEPROM via EERD, legacy 16-byte RX/TX descriptor rings in one
physically-contiguous DMA arena carved from the frame allocator, polled
operation with interrupts masked. The driver core is portable and fully
unit-tested on the host against a software model of the chip (the same
pattern as the PCI enumerator); the boot crate supplies only the thin real
bus — BAR0 MMIO through the bootloader's physical-memory mapping, plus the
PCI command-register write that grants bus mastering (`boot/src/nic.rs`).
Verified live under QEMU (`-netdev user -device e1000`): the kernel takes
10.0.2.15, ARP-resolves the slirp gateway, and gets real ICMP echo replies
from 10.0.2.2 across the DMA rings.

On top of the real wire sit the last two pieces of a working internet host:

- **Routing** — the stack knows its netmask and default gateway; off-subnet
  traffic is L2-addressed to the gateway's MAC while the IP header still
  names the true destination (the per-packet routing decision every real
  stack makes, previously missing because loopback never needed it).
- **DNS** (`nucleus/src/dns.rs`) — a from-scratch RFC 1035 codec: A-record
  query builder and response parser, including name compression with a
  pointer-hop budget so a hostile resolver cannot spin the kernel, plus
  fuzz-style truncation tests. The `dns` command resolves over real UDP/53
  (slirp's stub resolver at 10.0.2.3); `fetch <host> [path]` composes the
  whole tower — DNS resolve → TCP handshake → HTTP GET → body streamed back.

Verified end to end on bare metal: `fetch example.com /` returned
`HTTP/1.1 200 OK` — 827 bytes served by Cloudflare, carried over the
from-scratch driver, stack, TCP, and resolver. The kernel is an internet
host. (That fetch also found and killed a real allocator bug: the free-list
heap never coalesced freed neighbors, so `Vec` doubling during body receive
shattered the arena and OOM'd — the heap is now address-ordered and merges
on free, with regression tests for exactly that workload.)

And the other direction — **the kernel serves the web** (`nucleus/src/httpd.rs`):
a from-scratch HTTP/1.0 daemon over the kernel's own TCP listener, serving
files straight out of the kernel's own VFS. No sockets API, no userspace —
a polled kernel service pumped by the idle loop (`Nucleus::pump_services`),
started from praxsh with `httpd start 80`. It buffers split requests, refuses
traversal and non-GET methods, chunks responses under the NIC's DMA buffer
size, and answers a half-closing client from `CloseWait` (the `Tcp::send`
state check was widened for exactly that). `GET /` serves `/index.html` when
present, else a generated status page. Verified live: with
`hostfwd=tcp::18080-:80`, a real host-side `curl http://127.0.0.1:18080/etc/motd`
got `proof buys speed` back from the kernel, plus the front page with live
request stats and a correct `404` with headers for a missing path.

The Praxis twist — the thing Linux does not do — is that **network egress is a
function of proof, not identity**. Every send and every outbound connection
carries the caller's proof tier; an `Unproven` principal may reach loopback but
is **denied the wire**, exactly like the process-capability and syscall layers. A
compromised, unproven program simply cannot exfiltrate.

praxsh commands: `net · ping · arp · udp {bind|send|recv} · tcp {listen|connect|accept|send|recv|close} · firewall <tier> <ip>`.

```text
praxsh> ping 10.0.0.1 2
reply from 10.0.0.1 icmp_seq=1
praxsh> tcp listen 80
praxsh> tcp connect 10.0.0.1 80          # full 3-way handshake, over loopback
conn 0 → 10.0.0.1:80 — ESTABLISHED
praxsh> tcp accept 80
accepted conn 1 on port 80 — ESTABLISHED
praxsh> tcp send 0 GET / HTTP/1.0
praxsh> tcp recv 1
conn 1: GET / HTTP/1.0                  # the server side received it
praxsh> firewall unproven 8.8.8.8
unproven → 8.8.8.8: DENIED — unproven code may not reach the wire
```

## Run it

```sh
cargo run -p praxis-host              # interactive praxsh (any machine)
cargo run -p praxis-host -- --demo    # scripted tour of every mechanism
cargo test -p praxis-nucleus          # 87 kernel-core tests
```

## Bare metal

```sh
rustup target add x86_64-unknown-none
cargo build -p praxis-boot --target x86_64-unknown-none --release
```

To boot it, wrap the ELF in a bootable disk image. The `image/` crate does this
with the `bootloader` builder (BIOS + UEFI). Its build script needs a **nightly**
toolchain, so it is kept out of the stable workspace and run explicitly:

```sh
rustup toolchain install nightly --component rust-src --component llvm-tools-preview
rustup target add x86_64-unknown-none --toolchain nightly
cargo +nightly build -p praxis-boot --target x86_64-unknown-none --release
cd os/image && cargo +nightly run          # writes os/dist/praxis-os-{bios,uefi}.img
qemu-system-x86_64 -drive format=raw,file=os/dist/praxis-os-bios.img -serial stdio
```

and praxsh answers on the serial console.

**This boots for real.** Verified end-to-end under QEMU (BIOS): the bootloader
hands off, the kernel brings up the GDT/TSS + syscalls, the IDT/PIC/PIT/keyboard,
the physical frame allocator, paging (a real isolated `init` process), the
proof-gated scheduler, the VFS, and a **loopback TCP request that completes a
full 3-way handshake and carries data on bare metal** — then drops to `praxsh>`:

```text
Praxis OS — proof-scheduled kernel, bare metal x86_64
gdt/tss + syscall fast path: online (ring 0/3 ready)
idt/pic/timer/keyboard: online (PIT @ 100 Hz)
physical frames: mapped from bootloader memory map
spawned pid 1 'init' … loaded, isolated, scheduled
pci bus: 7 device(s) discovered
00:03.0  8086:100e  02.00  Network controller (Intel)          ← real config-space read
00:04.0  1af4:1005  00.ff  Unclassified (Red Hat (virtio)) [virtio]
listening on tcp port 80
conn 0 → 10.0.0.1:80 — ESTABLISHED
conn 1: GET /motd HTTP/1.0
praxsh>
```

The kernel enumerates the real PCI bus over the 0xCF8/0xCFC config ports
(`nucleus/src/pci.rs`, portable + host-tested; the boot layer supplies the real
`in/out` port I/O) — the discovery step every real driver starts from.

(Headless capture: add `-serial file:out.txt -display none -no-reboot`; for a
fault trace add `-d int,guest_errors -D qemu.txt`.)

## Raspberry Pi / ARM64 (`pi/`)

Praxis is **not x86-only**. The nucleus is architecture-independent `no_std` Rust,
so porting to another machine is just rewriting the thin arch layer at the
bottom. The `pi/` crate is the **AArch64 (ARM64)** port for the Raspberry Pi: a
new entry stub, a PL011 UART console, and the ARM generic-timer clock — and the
*entire* nucleus (scheduler, proof economy, IPC, intents, SPU, VFS, and the
UDP/ICMP/TCP stack) reused verbatim. No segmentation, no port I/O, no BIOS.

```sh
rustup target add aarch64-unknown-none --toolchain nightly
cargo +nightly build -p praxis-pi --target aarch64-unknown-none --release
rust-objcopy -O binary target/aarch64-unknown-none/release/praxis-pi dist/kernel8.img
qemu-system-aarch64 -M raspi3b -kernel dist/kernel8.img -serial stdio
```

**Verified booting on QEMU's Raspberry Pi 3** — the same OS, a new ISA:

```text
Praxis OS — AArch64 (Raspberry Pi), bare metal
cpu: exception level EL2, generic-timer clock online
spawned task 1 'demo-proven-1' … / task 2 'demo-unproven-2' …
conn 0 → 10.0.0.1:80 — ESTABLISHED          ← TCP 3-way handshake, on ARM64
conn 1: GET /motd HTTP/1.0
praxsh>
```

Retargeting is one constant: the PL011 base (`0x3F20_1000` on Pi 3 / QEMU
`raspi3b`, `0xFE20_1000` on Pi 4, `0x0900_0000` on QEMU `-M virt`). Running it on
a *physical* Pi is a spare-machine exercise — it still needs drivers for that
board's specific peripherals — but the kernel already boots the ARM64 the Pi is.

## Relationship to the Astra backend

`backend/crates/core/src/praxis/` is **Phase 0** — the proof-scheduled runtime
on a host OS, wired to the real proof economy, Sentinel, Forge and the claw
CLI at `/api/v1/praxis/*`. This directory is the **Phase 2 seed** — the same
tier discipline compiled into a kernel that owns the machine. The bridge is
deliberate: the userspace economy mints and attacks claims; the nucleus
consumes their verdicts the way Linux consumes page tables.
