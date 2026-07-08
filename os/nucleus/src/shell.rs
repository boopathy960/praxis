//! praxsh-nucleus — the kernel's own command line.
//!
//! Transport-agnostic: one line in, formatted text out over any
//! `core::fmt::Write` (the bare-metal UART, the hosted stdout, anything).
//! The commands are the kernel's actual primitives — the trust ledger, the
//! epistemic scheduler, zero-copy channels, transactional intents — not a
//! userland pretending to be one.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use crate::intent::{submit, Intent, Op, Post};
use crate::ipc::Channel;
use crate::proof::{ClaimClass, Tier};
use crate::sched::yield_now;
use crate::Nucleus;

const HELP: &str = "\
praxsh — the Praxis nucleus shell (freestanding kernel core)
  help                        this text
  uname                       kernel identity + the mechanisms it ships
  ps                          tasks: tier, polls, predicted burst, surprise
  tiers                       tier census + the cost model
  mem                         nucleus heap usage (bare metal) or host note
  ticks                       current clock tick
  spawn <proven|partial|unproven> [bursts]   admit a demo task at a tier
  run [polls]                 run the scheduler for a slice (default 64)
  claims                      every claim in the trust ledger
  drift <claim-id>            break a claim (the sentinel's verdict, in kernel)
  restore <claim-id>          re-prove a drifted claim
  resync                      recompute tiers live; log transitions
  transitions                 the tier ledger (tick-stamped privilege moves)
  suspects                    proven tasks whose behavior defies their model
  kairos [push <id> <units>]  the scheduling index, decomposed per task —
                              backlog x service-rate / model-uncertainty;
                              push declares pending work units (an arrival)
  ipcbench [n]                zero-copy handoff vs copying transfer, in ticks
  intent-demo                 fusion + transactional rollback, live
  kv put <k> <v> | get <k> | del <k> | list   intent-backed object store

SPU — the State Processing Unit executive (os/SPU-OS.md)
  spu                         device status: wear pacing, grafts, mode fabric
  ctx new <name> <kb> <tokens> | list | touch <id> | place | suspend <id>
                              agent contexts + thermodynamic placement
  graft <src> <dst> <tokens>  State-Graft vs re-prefill (the n0 crossover)
  reason <deadline> [conf]    anytime reasoning w/ metacognitive stopping
  modes [demand <det|stoch> <n> | service <k>]   ESQ-batched mode switching
  consolidate [record <skill> <ok|fail> <novelty>]  experience -> skill

storage & memory
  frames                      physical frame allocator: used/free/total
  ls [path] | stat <path> | mkdir <path> | rm <path>
  cat <path> | write <path> <text...>          file IO over the VFS
  sync                        snapshot the filesystem to the persistent tier
  fsck                        reload the filesystem from the persistent tier

processes & virtual memory
  exec <proven|partial|unproven> [name]   load a built-in demo ELF as a process
  proc                        the process table: pid, tier, caps, pages, CR3
  pmap <pid>                  a process's virtual memory + capability map
  sched                       run the tier-aware process scheduler once
  kill <pid>                  terminate a process, reclaiming its frames
  top                         preemptive scheduler: proof-weighted quanta,
                              CPU share, preemptions (a runaway can't hang it)
  quantum [n]                 advance the RT scheduler n timer ticks, live
  sys <pid> <call> ...        make a syscall AS a process — authority is
                              checked against its proof-derived capabilities
                              calls: getpid | log <msg> | read <path>
                                     write <path> <text...> | spawn <tier> <name>

networking — a real stack, with egress gated by proof (not identity)
  net                         interface: ip/mac, arp cache, sockets, counters
  ping <ip> [count]           ICMP echo over the stack's own eth/ip/icmp
  arp                         the ARP cache (ip -> mac), learned live
  udp bind <port> | send <ip> <port> <text...> | recv <port>
                              datagrams demultiplexed to bound sockets
  tcp                         the connection table (id, state, remote)
  tcp listen <port> | connect <ip> <port> | accept <port>
  tcp send <id> <text...> | recv <id> | close <id>
                              a real TCP: 3-way handshake, data, FIN close
  dns <name> [server]         resolve a hostname (A records) over real UDP;
                              default server 10.0.2.3 (QEMU slirp resolver)
  fetch <host> [path]         resolve + TCP connect + HTTP GET, end to end —
                              the kernel fetching a page off the internet
  httpd start [port] | status the kernel HTTP daemon: serves the VFS over the
                              kernel's own TCP stack (default port 80)
  pci                         the PCI devices the kernel found on the bus
  cpus                        CPU topology: cores found (ACPI) + started (SMP)
  crypto <sha256|hmac|hkdf|x25519> <args>   the from-scratch TLS crypto suite
  virtio selftest             round-trip a sector through the virtio-blk driver
  firewall <proven|partial|unproven> <ip>   would this tier reach that host?
                              (the wire needs proof; loopback never does)

operator primitives — commands no Unix has (os problems Praxis deletes)
  ensure <key> <value>        declare the goal; idempotent + proven + atomic
                              (re-running when it holds is an instant no-op)
  undo | redo                 step the whole terminal backward/forward in time
  dry <key> <value>           preview a change with a zero-touch guarantee
  why <kv <key> | task <id> | proc <pid>>   causal history, not log archaeology
  prove <pred> [recheck]      check a fact and remember it (never re-check)
  recall <pred>               the cached verdict, instantly
  when <pred> do <command>    fire a command when a predicate becomes true
                              predicates: kv:<k>=<v> | kv:<k>~<text>
                                          file:<path> | file:<path>~<text>";

/// Execute one shell line, then run the reactive pass so any `when` triggers
/// whose predicate the command just made true fire as a bounded cascade. This
/// is what makes `when <predicate> do <command>` work without a poll loop.
pub fn exec(nucleus: &mut Nucleus, line: &str, out: &mut dyn Write) {
    dispatch(nucleus, line, out);
    if !nucleus.ops.reacting {
        nucleus.ops.reacting = true;
        // Bounded cascade: a fired command may satisfy another trigger, but we
        // cap the chain so a self-perpetuating pair cannot loop forever.
        for _ in 0..8 {
            let fired = nucleus.ops.take_fired(&nucleus.store, &nucleus.fs);
            if fired.is_empty() {
                break;
            }
            for command in fired {
                let _ = writeln!(out, "[when] → {command}");
                dispatch(nucleus, &command, out);
            }
        }
        nucleus.ops.reacting = false;
    }
}

/// Execute one shell line against the nucleus. Command failures are printed,
/// never returned — a shell that dies on a typo is not a shell.
fn dispatch(nucleus: &mut Nucleus, line: &str, out: &mut dyn Write) {
    let line = line.trim();
    let mut parts = line.split_whitespace();
    let head = parts.next().unwrap_or("");
    let args: Vec<&str> = parts.collect();
    let result = match head {
        "" | "help" => writeln!(out, "{HELP}"),
        "uname" => uname(nucleus, out),
        "ps" => ps(nucleus, out),
        "tiers" => tiers(nucleus, out),
        "mem" => mem(nucleus, out),
        "ticks" => writeln!(out, "tick {}", nucleus.sched.now()),
        "spawn" => spawn(nucleus, &args, out),
        "run" => run(nucleus, &args, out),
        "claims" => claims(nucleus, out),
        "drift" => drift(nucleus, &args, true, out),
        "restore" => drift(nucleus, &args, false, out),
        "resync" => resync(nucleus, out),
        "transitions" => transitions(nucleus, out),
        "suspects" => suspects(nucleus, out),
        "kairos" => kairos_cmd(nucleus, &args, out),
        "ipcbench" => ipcbench(nucleus, &args, out),
        "intent-demo" => intent_demo(nucleus, out),
        "kv" => kv(nucleus, &args, out),
        "spu" => spu_status(nucleus, out),
        "ctx" => ctx_cmd(nucleus, &args, out),
        "graft" => graft_cmd(nucleus, &args, out),
        "reason" => reason_cmd(nucleus, &args, out),
        "modes" => modes_cmd(nucleus, &args, out),
        "consolidate" => consolidate_cmd(nucleus, &args, out),
        "frames" => frames_cmd(nucleus, out),
        "ls" => ls_cmd(nucleus, &args, out),
        "stat" => stat_cmd(nucleus, &args, out),
        "mkdir" => mkdir_cmd(nucleus, &args, out),
        "rm" => rm_cmd(nucleus, &args, out),
        "cat" => cat_cmd(nucleus, &args, out),
        "write" => write_cmd(nucleus, &args, out),
        "sync" => sync_cmd(nucleus, out),
        "fsck" => fsck_cmd(nucleus, out),
        "exec" => exec_cmd(nucleus, &args, out),
        "proc" => proc_cmd(nucleus, out),
        "pmap" => pmap_cmd(nucleus, &args, out),
        "sched" => sched_cmd(nucleus, out),
        "kill" => kill_cmd(nucleus, &args, out),
        "top" => top_cmd(nucleus, out),
        "quantum" => quantum_cmd(nucleus, &args, out),
        "net" => net_cmd(nucleus, out),
        "ping" => ping_cmd(nucleus, &args, out),
        "arp" => arp_cmd(nucleus, out),
        "udp" => udp_cmd(nucleus, &args, out),
        "tcp" => tcp_cmd(nucleus, &args, out),
        "dns" => dns_cmd(nucleus, &args, out),
        "fetch" => fetch_cmd(nucleus, &args, out),
        "httpd" => httpd_cmd(nucleus, &args, out),
        "pci" => pci_cmd(nucleus, out),
        "cpus" => cpus_cmd(nucleus, out),
        "crypto" => crypto_cmd(&args, out),
        "virtio" => virtio_cmd(&args, out),
        "firewall" => firewall_cmd(nucleus, &args, out),
        "sys" => sys_cmd(nucleus, &args, out),
        "ensure" => ensure_cmd(nucleus, &args, out),
        "undo" => undo_cmd(nucleus, out),
        "redo" => redo_cmd(nucleus, out),
        "dry" => dry_cmd(nucleus, &args, out),
        "why" => why_cmd(nucleus, &args, out),
        "prove" => prove_cmd(nucleus, &args, out),
        "recall" => recall_cmd(nucleus, &args, out),
        "when" => when_cmd(nucleus, &args, out),
        other => writeln!(out, "unknown command '{other}' — try `help`"),
    };
    let _ = result;
}

fn uname(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    writeln!(
        out,
        "Praxis Nucleus {} — freestanding Rust kernel core (no_std, zero deps)\n\
         mechanisms: proof-gated cooperative scheduling / epistemic (surprise-\n\
         driven) scheduler / kairos index (risk-adjusted generalized-cµ:\n\
         backlog x rate / uncertainty) / tier-gated zero-copy IPC /\n\
         transactional intent syscalls with postcondition proof\n\
         tasks={} runnable={} round={}",
        env!("CARGO_PKG_VERSION"),
        nucleus.sched.tasks().len(),
        nucleus.sched.runnable(),
        nucleus.sched.round(),
    )
}

fn ps(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    writeln!(
        out,
        "{:<4} {:<14} {:<9} {:>7} {:>12} {:>9}  STATE",
        "ID", "NAME", "TIER", "POLLS", "PRED_mTICKS", "SURPRISE"
    )?;
    for task in nucleus.sched.tasks() {
        writeln!(
            out,
            "{:<4} {:<14} {:<9} {:>7} {:>12} {:>8}‰  {}",
            task.id,
            task.name,
            task.tier.name(),
            task.stats.polls,
            task.stats.predicted_milliticks,
            task.stats.surprise_milli,
            if task.done { "done" } else { "runnable" }
        )?;
    }
    Ok(())
}

fn tiers(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let mut census = [0usize; 3];
    for task in nucleus.sched.tasks() {
        census[task.tier.rank() as usize] += 1;
    }
    writeln!(
        out,
        "tier 0 proven    {:>3}  cooperative, unthrottled — preemption is a distrust tax it never pays\n\
         tier 1 partial   {:>3}  guard checks per boundary crossing\n\
         tier 2 unproven  {:>3}  throttled to every {}th round — the tax, priced explicitly",
        census[0],
        census[1],
        census[2],
        crate::sched::UNPROVEN_STRIDE
    )
}

fn mem(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    match nucleus.heap {
        Some(heap) => writeln!(
            out,
            "nucleus heap: {} / {} bytes used (free-list allocator)",
            heap.used(),
            heap.total()
        ),
        None => writeln!(out, "hosted run: the platform allocator backs the nucleus"),
    }
}

fn spawn(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let tier = args.first().copied().unwrap_or("unproven");
    let bursts: u64 = args
        .get(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(64);
    let id = nucleus.sched.peek_next_id();
    match tier {
        "proven" => {
            nucleus.ledger.grant(id, ClaimClass::MemorySafety);
            nucleus.ledger.grant(id, ClaimClass::CapabilityBound);
            nucleus.ledger.grant(id, ClaimClass::ResourceBound);
        }
        "partial" => {
            nucleus.ledger.grant(id, ClaimClass::MemorySafety);
        }
        "unproven" => {}
        other => return writeln!(out, "unknown tier '{other}' (proven|partial|unproven)"),
    }
    let name = alloc::format!("demo-{tier}-{id}");
    let spawned = nucleus
        .sched
        .spawn(name.clone(), &nucleus.ledger, demo_task(bursts));
    writeln!(
        out,
        "spawned task {spawned} '{name}' at tier {} ({bursts} bursts)",
        nucleus.ledger.tier_of(spawned).name()
    )
}

async fn demo_task(bursts: u64) {
    for i in 0..bursts {
        // A deterministic little burst of work between yields.
        let mut acc = 0u64;
        for j in 0..(200 + (i % 7) * 40) {
            acc = acc.wrapping_add(core::hint::black_box(j));
        }
        core::hint::black_box(acc);
        yield_now().await;
    }
}

fn run(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let polls: usize = args
        .first()
        .and_then(|value| value.parse().ok())
        .unwrap_or(64);
    let done = nucleus.sched.run_slice(polls);
    writeln!(
        out,
        "ran {done} scheduling decisions; {} tasks still runnable",
        nucleus.sched.runnable()
    )
}

fn claims(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let mut any = false;
    for claim in nucleus.ledger.all_claims() {
        any = true;
        writeln!(
            out,
            "claim {} task {} {} holding={}",
            claim.claim_id,
            claim.task,
            claim.class.name(),
            claim.holding
        )?;
    }
    if !any {
        writeln!(out, "the trust ledger is empty")?;
    }
    Ok(())
}

fn drift(
    nucleus: &mut Nucleus,
    args: &[&str],
    break_it: bool,
    out: &mut dyn Write,
) -> core::fmt::Result {
    let Some(claim_id) = args.first().and_then(|value| value.parse().ok()) else {
        return writeln!(out, "usage: drift|restore <claim-id>");
    };
    let claim = if break_it {
        nucleus.ledger.drift(claim_id)
    } else {
        nucleus.ledger.restore(claim_id)
    };
    match claim {
        Some(claim) => {
            let (task, class, holding) = (claim.task, claim.class, claim.holding);
            writeln!(
                out,
                "claim {claim_id} ({}) on task {task} holding={holding} — run `resync` to apply",
                class.name()
            )
        }
        None => writeln!(out, "no claim {claim_id}"),
    }
}

fn resync(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let moved = nucleus.sched.resync(&mut nucleus.ledger);
    writeln!(out, "resync: {moved} task(s) changed tier while running")
}

fn transitions(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let transitions = nucleus.ledger.transitions();
    if transitions.is_empty() {
        return writeln!(out, "no tier transitions recorded");
    }
    for t in transitions {
        writeln!(
            out,
            "tick {} task {}: {} -> {} ({})",
            t.at_tick,
            t.task,
            t.from.name(),
            t.to.name(),
            t.reason
        )?;
    }
    Ok(())
}

fn suspects(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let suspects = nucleus.sched.suspects();
    if suspects.is_empty() {
        return writeln!(
            out,
            "no drift suspects — every proven task matches its model"
        );
    }
    for task in suspects {
        writeln!(
            out,
            "task {} '{}' surprise={}‰ — behavior defies its model; re-verify its claims",
            task.id, task.name, task.stats.surprise_milli
        )?;
    }
    Ok(())
}

/// The Kairos index, decomposed — what the scheduler is actually maximizing
/// and why. `kairos push <id> <units>` declares an arrival of pending work.
fn kairos_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    if let ["push", id, units] = args {
        return match (id.parse::<u64>(), units.parse::<u64>()) {
            (Ok(id), Ok(units)) => {
                if nucleus.sched.push_backlog(id, units) {
                    writeln!(out, "task {id}: +{units} pending units declared")
                } else {
                    writeln!(out, "no runnable task with id {id}")
                }
            }
            _ => writeln!(out, "usage: kairos push <task-id> <units>"),
        };
    }
    writeln!(
        out,
        "K = (V*A + Q*1000) * 1000 / (b + u + 1000)   — serve max K\n\
         Q backlog (max-weight: throughput-optimal) / b predicted burst\n\
         (generalized-cµ: flow time collapses) / u model uncertainty (the\n\
         epistemic discount: predictability is CPU currency)\n"
    )?;
    writeln!(
        out,
        "{:<4} {:<14} {:<9} {:>7} {:>9} {:>9} {:>7} {:>10}",
        "ID", "NAME", "TIER", "Q", "b_mTICK", "u_mILLI", "A", "K"
    )?;
    for task in nucleus.sched.tasks() {
        if task.done {
            continue;
        }
        let attention = crate::sched::Scheduler::attention(task.tier, task.stats.surprise_milli);
        let k = crate::kairos::index(&crate::kairos::Inputs {
            attention,
            backlog: task.stats.backlog,
            predicted_milliticks: task.stats.predicted_milliticks,
            uncertainty_milli: task.stats.uncertainty_milli,
        });
        writeln!(
            out,
            "{:<4} {:<14} {:<9} {:>7} {:>9} {:>9} {:>7} {:>10}",
            task.id,
            task.name,
            task.tier.name(),
            task.stats.backlog,
            task.stats.predicted_milliticks,
            task.stats.uncertainty_milli,
            attention,
            k
        )?;
    }
    Ok(())
}

fn ipcbench(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let n: u64 = args
        .first()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000);
    type Message = [u8; 1024];
    let channel: Channel<Message> = Channel::new(Tier::Partial);

    // Zero-copy: the allocation itself moves through the channel.
    let mut message = Box::new([0u8; 1024]);
    let start = nucleus.sched.now();
    for _ in 0..n {
        channel.send(message, Tier::Proven).ok();
        message = channel.recv().expect("just sent");
    }
    let handoff_ticks = nucleus.sched.now().saturating_sub(start);

    // The copying boundary every distrust-based OS imposes: byte-for-byte
    // duplication per crossing (the kernel-copy analog, same address space).
    let mut copied = Box::new([0u8; 1024]);
    let start = nucleus.sched.now();
    for _ in 0..n {
        let clone = Box::new(*copied);
        core::hint::black_box(&clone);
        copied = clone;
    }
    let copy_ticks = nucleus.sched.now().saturating_sub(start);

    writeln!(
        out,
        "{n} crossings x 1 KiB:\n\
         zero-copy ownership handoff : {handoff_ticks} ticks ({} /msg)\n\
         copying transfer            : {copy_ticks} ticks ({} /msg)\n\
         (bare metal adds traps + context switches to the copying path;\n\
          proof deletes those too)",
        handoff_ticks / n.max(1),
        copy_ticks / n.max(1),
    )
}

fn intent_demo(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let fused = submit(
        &mut nucleus.store,
        Intent {
            goal: "assemble greeting".into(),
            ops: alloc::vec![
                Op::Put {
                    key: "msg".into(),
                    value: "hello".into()
                },
                Op::Append {
                    key: "msg".into(),
                    value: " praxis".into()
                },
                Op::Put {
                    key: "scratch".into(),
                    value: "tmp".into()
                },
                Op::Delete {
                    key: "scratch".into()
                },
            ],
            post: Post::KeyEquals {
                key: "msg".into(),
                value: "hello praxis".into(),
            },
        },
    );
    writeln!(
        out,
        "intent 1 '{}': {} ops submitted -> {} executed (fused), committed={} — {}",
        fused.goal, fused.ops_submitted, fused.ops_executed, fused.committed, fused.detail
    )?;

    let rolled_back = submit(
        &mut nucleus.store,
        Intent {
            goal: "botched update".into(),
            ops: alloc::vec![
                Op::Put {
                    key: "msg".into(),
                    value: "corrupted".into()
                },
                Op::Put {
                    key: "collateral".into(),
                    value: "damage".into()
                },
            ],
            post: Post::KeyEquals {
                key: "msg".into(),
                value: "something else entirely".into(),
            },
        },
    );
    writeln!(
        out,
        "intent 2 '{}': committed={} — {}",
        rolled_back.goal, rolled_back.committed, rolled_back.detail
    )?;
    writeln!(
        out,
        "store after both: msg={:?} collateral={:?}  (the failed intent left no trace)",
        nucleus.store.get("msg"),
        nucleus.store.get("collateral")
    )
}

fn kv(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    match args {
        ["put", key, rest @ ..] if !rest.is_empty() => {
            let value = rest.join(" ");
            let report = submit(
                &mut nucleus.store,
                Intent {
                    goal: alloc::format!("kv put {key}"),
                    ops: alloc::vec![Op::Put {
                        key: (*key).to_string(),
                        value: value.clone(),
                    }],
                    post: Post::KeyEquals {
                        key: (*key).to_string(),
                        value,
                    },
                },
            );
            writeln!(out, "{} (committed={})", report.detail, report.committed)
        }
        ["del", key] => {
            let report = submit(
                &mut nucleus.store,
                Intent {
                    goal: alloc::format!("kv del {key}"),
                    ops: alloc::vec![Op::Delete {
                        key: (*key).to_string(),
                    }],
                    post: Post::KeyAbsent {
                        key: (*key).to_string(),
                    },
                },
            );
            writeln!(out, "{} (committed={})", report.detail, report.committed)
        }
        ["get", key] => match nucleus.store.get(key) {
            Some(value) => writeln!(out, "{key} = {value}"),
            None => writeln!(out, "{key} is absent"),
        },
        ["list"] => {
            if nucleus.store.is_empty() {
                writeln!(out, "object store is empty")
            } else {
                let keys: Vec<String> = nucleus.store.keys().cloned().collect();
                writeln!(out, "{} key(s): {}", keys.len(), keys.join(", "))
            }
        }
        _ => writeln!(
            out,
            "usage: kv put <k> <v> | kv get <k> | kv del <k> | kv list"
        ),
    }
}

// ── the SPU executive (os/SPU-OS.md) ─────────────────────────────────────

fn spu_status(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let now = nucleus.sched.now();
    let wear = nucleus.spu.wear_status(now);
    let (grafts, saved) = nucleus.spu.graft_stats();
    let (q_det, q_stoch) = nucleus.modes.queued();
    writeln!(
        out,
        "SPU executive @ tick {now}\n\
         wear pacing : pace={} writes/plane={:?} admitted={} denied={} (N_w={} L={})\n\
         state-graft : {} grafts, {} milli-ticks saved (n0={} tokens)\n\
         mode fabric : mode={:?} B*={} switches={} served det/stoch={}/{} queued={}/{}\n\
         consolidator: {} pending traces\n\
         p-bit fabric: {} samples drawn",
        wear.pace_now,
        wear.plane_writes,
        wear.flushes_admitted,
        wear.flushes_denied,
        wear.endurance,
        wear.lifetime_ticks,
        grafts,
        saved,
        crate::spu::SpuDevice::graft_threshold_tokens(),
        nucleus.modes.mode,
        nucleus.modes.batch_size(),
        nucleus.modes.switches,
        nucleus.modes.served_det,
        nucleus.modes.served_stoch,
        q_det,
        q_stoch,
        nucleus.consolidator.pending_traces(),
        nucleus.pbits.samples_drawn,
    )
}

fn ctx_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let now = nucleus.sched.now();
    match args {
        ["new", name, kb, tokens] => {
            let size: u64 = kb.parse().unwrap_or(16);
            let kv: u64 = tokens.parse().unwrap_or(0);
            let id = nucleus.spu.create_context(*name, size, kv);
            writeln!(
                out,
                "context {id} '{name}' born dormant in nv ({size} KB, {kv} tokens)"
            )
        }
        ["list"] => {
            writeln!(
                out,
                "{:<4} {:<14} {:<5} {:>6} {:>10} {:>8} {:>8}  STATE",
                "ID", "NAME", "TIER", "KB", "HEAT_m", "DENSITY", "TOKENS"
            )?;
            for ctx in nucleus.spu.contexts() {
                writeln!(
                    out,
                    "{:<4} {:<14} {:<5} {:>6} {:>10} {:>8} {:>8}  {}",
                    ctx.ctx_id,
                    ctx.name,
                    ctx.tier.name(),
                    ctx.size_kb,
                    ctx.heat_milli,
                    ctx.value_density(),
                    ctx.kv_tokens,
                    if ctx.dormant { "dormant" } else { "active" }
                )?;
            }
            Ok(())
        }
        ["touch", id] => {
            let id: u64 = id.parse().unwrap_or(0);
            match nucleus.spu.touch(id, now) {
                Some(wake) => writeln!(out, "touched ctx {id} (wake tax {wake} ticks)"),
                None => writeln!(out, "no context {id}"),
            }
        }
        ["place"] => {
            let report = nucleus.spu.place(now);
            writeln!(
                out,
                "placement: {} contexts, {} migrations -> sram={} dram={} nv={} (cost rate {} milli/tick)",
                report.contexts,
                report.migrations,
                report.per_tier[0],
                report.per_tier[1],
                report.per_tier[2],
                report.total_cost_rate_milli
            )
        }
        ["suspend", id] => {
            let id: u64 = id.parse().unwrap_or(0);
            match nucleus.spu.suspend(id, now) {
                Some((tier, admitted)) => writeln!(
                    out,
                    "ctx {id} hibernated to {} (fe write {})",
                    tier.name(),
                    if admitted {
                        "admitted"
                    } else {
                        "DENIED by wear pacing — parked volatile"
                    }
                ),
                None => writeln!(out, "no context {id}"),
            }
        }
        _ => writeln!(
            out,
            "usage: ctx new <name> <kb> <tokens> | list | touch <id> | place | suspend <id>"
        ),
    }
}

fn graft_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let (Some(src), Some(dst), Some(tokens)) = (
        args.first().and_then(|v| v.parse::<u64>().ok()),
        args.get(1).and_then(|v| v.parse::<u64>().ok()),
        args.get(2).and_then(|v| v.parse::<u64>().ok()),
    ) else {
        return writeln!(out, "usage: graft <src-ctx> <dst-ctx> <tokens>");
    };
    match nucleus.spu.graft(src, dst, tokens) {
        Some(report) => writeln!(
            out,
            "graft {}: {} tokens (n0={}) prefill={}m graft={}m saved={}m — {}",
            if report.grafted {
                "COMMITTED"
            } else {
                "declined"
            },
            report.tokens,
            report.threshold_tokens,
            report.cost_prefill_milli,
            report.cost_graft_milli,
            report.saved_milli,
            if report.grafted {
                "shared thought, zero re-prefill"
            } else {
                "below crossover: re-speaking is cheaper"
            }
        ),
        None => writeln!(out, "bad contexts (src must exist, src != dst)"),
    }
}

fn reason_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let deadline: u64 = args.first().and_then(|v| v.parse().ok()).unwrap_or(2000);
    let target: u64 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(900);
    // Three candidate thoughts of hidden quality; the fabric finds the best.
    let candidates = [400u64, 550, 850];
    let sched = &nucleus.sched;
    let report = crate::spu::cortex::reason_until(
        &mut nucleus.pbits,
        &candidates,
        deadline,
        target,
        4000,
        &|| sched.now(),
    );
    writeln!(
        out,
        "reason.until({deadline}): best=candidate-{} p^={}‰ confidence={}‰ samples={} elapsed={} verdict={:?}",
        report.best_candidate,
        report.best_p_milli,
        report.confidence_milli,
        report.samples,
        report.elapsed_ticks,
        report.verdict
    )
}

fn modes_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    use crate::spu::cortex::FabricMode;
    match args {
        ["demand", kind, n] => {
            let count: u64 = n.parse().unwrap_or(1);
            let mode = match *kind {
                "det" => FabricMode::Deterministic,
                "stoch" => FabricMode::Stochastic,
                _ => return writeln!(out, "kind is det|stoch"),
            };
            nucleus.modes.demand(mode, count);
            let (d, s) = nucleus.modes.queued();
            writeln!(
                out,
                "queued: det={d} stoch={s} (B*={})",
                nucleus.modes.batch_size()
            )
        }
        ["service", k] => {
            let rounds: u64 = k.parse().unwrap_or(1);
            let mut served = 0;
            for _ in 0..rounds {
                served += nucleus.modes.service();
            }
            let (d, s) = nucleus.modes.queued();
            writeln!(
                out,
                "served {served} in {rounds} quanta; mode={:?} switches={} remaining det={d} stoch={s}",
                nucleus.modes.mode, nucleus.modes.switches
            )
        }
        [] => writeln!(
            out,
            "mode={:?} B*={} switches={} served det/stoch={}/{}",
            nucleus.modes.mode,
            nucleus.modes.batch_size(),
            nucleus.modes.switches,
            nucleus.modes.served_det,
            nucleus.modes.served_stoch
        ),
        _ => writeln!(out, "usage: modes [demand <det|stoch> <n> | service <k>]"),
    }
}

fn consolidate_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    match args {
        ["record", skill, outcome, novelty] => {
            let success = *outcome == "ok";
            let novelty: u64 = novelty.parse().unwrap_or(0);
            nucleus.consolidator.record(*skill, success, novelty);
            writeln!(
                out,
                "trace recorded: {skill} {} novelty={novelty}‰ ({} pending)",
                if success { "ok" } else { "fail" },
                nucleus.consolidator.pending_traces()
            )
        }
        [] => {
            let now = nucleus.sched.now();
            let reports = nucleus.consolidator.consolidate(&mut nucleus.spu, now);
            if reports.is_empty() {
                return writeln!(out, "nothing to consolidate — record traces first");
            }
            for r in reports {
                writeln!(
                    out,
                    "{}: U={}‰ {} (level {}) — {}",
                    r.skill,
                    r.utility_milli,
                    if r.committed { "COMMITTED" } else { "deferred" },
                    r.new_level,
                    r.reason
                )?;
            }
            Ok(())
        }
        _ => writeln!(
            out,
            "usage: consolidate [record <skill> <ok|fail> <novelty-milli>]"
        ),
    }
}

// ── storage & memory ─────────────────────────────────────────────────────

fn frames_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    match &nucleus.frames {
        Some(fa) => {
            let s = fa.stats();
            writeln!(
                out,
                "physical frames: {}/{} used ({} KiB free of {} KiB), {} allocs {} frees",
                s.used_frames,
                s.total_frames,
                s.free_bytes() / 1024,
                s.total_bytes() / 1024,
                s.allocations,
                s.deallocations
            )
        }
        None => writeln!(out, "no memory map installed (hosted run)"),
    }
}

fn ls_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let path = args.first().copied().unwrap_or("/");
    match nucleus.fs.readdir(path) {
        Ok(entries) => {
            if entries.is_empty() {
                writeln!(out, "(empty)")
            } else {
                for name in entries {
                    let full = if path == "/" {
                        alloc::format!("/{name}")
                    } else {
                        alloc::format!("{path}/{name}")
                    };
                    let tag = match nucleus.fs.stat(&full) {
                        Ok(m) if m.kind == crate::fs::NodeKind::Dir => "/",
                        _ => "",
                    };
                    writeln!(out, "{name}{tag}")?;
                }
                Ok(())
            }
        }
        Err(e) => writeln!(out, "ls: {path}: {e:?}"),
    }
}

fn stat_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(path) = args.first() else {
        return writeln!(out, "usage: stat <path>");
    };
    match nucleus.fs.stat(path) {
        Ok(m) => writeln!(
            out,
            "{path}: {} ino={} size={} bytes",
            match m.kind {
                crate::fs::NodeKind::Dir => "directory",
                crate::fs::NodeKind::File => "file",
            },
            m.ino,
            m.size
        ),
        Err(e) => writeln!(out, "stat: {path}: {e:?}"),
    }
}

fn mkdir_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(path) = args.first() else {
        return writeln!(out, "usage: mkdir <path>");
    };
    match nucleus.fs.mkdir(path) {
        Ok(ino) => writeln!(out, "created directory {path} (ino {ino})"),
        Err(e) => writeln!(out, "mkdir: {path}: {e:?}"),
    }
}

fn rm_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(path) = args.first() else {
        return writeln!(out, "usage: rm <path>");
    };
    match nucleus.fs.unlink(path) {
        Ok(()) => writeln!(out, "removed {path}"),
        Err(e) => writeln!(out, "rm: {path}: {e:?}"),
    }
}

fn cat_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(path) = args.first() else {
        return writeln!(out, "usage: cat <path>");
    };
    match nucleus.fs.read_all(path) {
        Ok(bytes) => match core::str::from_utf8(&bytes) {
            Ok(text) => writeln!(out, "{text}"),
            Err(_) => writeln!(out, "{} bytes of binary data", bytes.len()),
        },
        Err(e) => writeln!(out, "cat: {path}: {e:?}"),
    }
}

fn write_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some((path, rest)) = args.split_first() else {
        return writeln!(out, "usage: write <path> <text...>");
    };
    let text = rest.join(" ");
    match nucleus.fs.write_all(path, text.as_bytes()) {
        Ok(()) => writeln!(out, "wrote {} bytes to {path}", text.len()),
        Err(e) => writeln!(out, "write: {path}: {e:?}"),
    }
}

fn sync_cmd(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    match nucleus.fs.snapshot_journaled(&mut nucleus.disk) {
        Ok(bytes) => writeln!(
            out,
            "filesystem snapshotted: {bytes} bytes to the persistent tier ({} nodes)",
            nucleus.fs.node_count()
        ),
        Err(e) => writeln!(out, "sync failed: {e:?}"),
    }
}

fn fsck_cmd(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    match crate::fs::Vfs::restore_journaled(&nucleus.disk) {
        Ok(restored) => {
            let nodes = restored.node_count();
            nucleus.fs = restored;
            writeln!(
                out,
                "filesystem reloaded from the persistent tier ({nodes} nodes)"
            )
        }
        Err(e) => writeln!(out, "fsck failed: {e:?} (nothing synced yet?)"),
    }
}

// ── processes & virtual memory ───────────────────────────────────────────

/// A tiny built-in demo program: `mov eax, 42; ret` (b8 2a 00 00 00 c3), the
/// stand-in for a real ELF since the kernel has no on-disk toolchain.
const DEMO_CODE: [u8; 6] = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
const DEMO_VADDR: u64 = 0x40_0000;

fn exec_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let tier = match args.first().copied().unwrap_or("unproven") {
        "proven" => Tier::Proven,
        "partial" => Tier::Partial,
        "unproven" => Tier::Unproven,
        other => return writeln!(out, "unknown tier '{other}' (proven|partial|unproven)"),
    };
    let name = args.get(1).copied().unwrap_or("demo");
    let Some(frames) = nucleus.frames.as_mut() else {
        return writeln!(
            out,
            "no memory map installed — cannot allocate an address space"
        );
    };
    let elf = crate::elf::synth_executable(DEMO_VADDR, &DEMO_CODE);
    match nucleus.procs.spawn(name, tier, &elf, frames) {
        Ok(pid) => {
            // Admit it to the preemptive scheduler at its proof tier — that is
            // where its time-slice length is set.
            nucleus.preempt.admit(pid, tier);
            let proc = nucleus.procs.get(pid).expect("just spawned");
            writeln!(
                out,
                "spawned pid {pid} '{name}' tier={} entry={:#x} pages={} CR3={:#x} \
                 quantum={} ticks — loaded, isolated, scheduled",
                proc.tier.name(),
                proc.entry,
                proc.mapped_pages(),
                proc.cr3(),
                crate::preempt::quantum_for(tier)
            )
        }
        Err(e) => writeln!(out, "exec failed: {e:?}"),
    }
}

fn proc_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    if nucleus.procs.count() == 0 {
        return writeln!(out, "no processes — try `exec proven`");
    }
    writeln!(
        out,
        "{:<4} {:<12} {:<9} {:>6} {:>7} {:>14}  CAPS",
        "PID", "NAME", "TIER", "PAGES", "TABLES", "CR3"
    )?;
    for p in nucleus.procs.list() {
        let mut caps = Vec::new();
        if p.caps.in_process_exec {
            caps.push("exec");
        }
        if p.caps.raw_device {
            caps.push("device");
        }
        if p.caps.fs_write {
            caps.push("fs_write");
        }
        if p.caps.spawn {
            caps.push("spawn");
        }
        let current = if nucleus.procs.current() == Some(p.pid) {
            "*"
        } else {
            " "
        };
        let caps_text = if caps.is_empty() {
            String::from("caged")
        } else {
            caps.join(",")
        };
        writeln!(
            out,
            "{current}{:<3} {:<12} {:<9} {:>6} {:>7} {:>14}  {}",
            p.pid,
            p.name,
            p.tier.name(),
            p.mapped_pages(),
            p.table_frames(),
            alloc::format!("{:#x}", p.cr3()),
            caps_text
        )?;
    }
    Ok(())
}

fn pmap_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(pid) = args.first().and_then(|v| v.parse::<u64>().ok()) else {
        return writeln!(out, "usage: pmap <pid>");
    };
    let Some(p) = nucleus.procs.get(pid) else {
        return writeln!(out, "no process {pid}");
    };
    writeln!(
        out,
        "pid {pid} '{}' tier={} state={:?}\n\
         address space: {} pages mapped across {} page-table frames, CR3={:#x}\n\
         entry point {:#x}, {} loadable segment(s)",
        p.name,
        p.tier.name(),
        p.state,
        p.mapped_pages(),
        p.table_frames(),
        p.cr3(),
        p.entry,
        p.segments
    )?;
    // Read the first bytes back through the process's own page tables.
    if let Some(bytes) = p.read_virt(p.entry, 6) {
        writeln!(out, "code@entry (via its page tables): {bytes:02x?}")?;
    }
    writeln!(
        out,
        "capabilities: exec={} device={} fs_write={} spawn={} grow={}",
        p.caps.in_process_exec,
        p.caps.raw_device,
        p.caps.fs_write,
        p.caps.spawn,
        p.caps.grow_memory
    )
}

fn sched_cmd(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    match nucleus.procs.schedule() {
        Some(pid) => {
            let name = nucleus
                .procs
                .get(pid)
                .map(|p| p.name.as_str())
                .unwrap_or("?");
            writeln!(
                out,
                "scheduled pid {pid} '{name}' (total context switches: {})",
                nucleus.procs.switches()
            )
        }
        None => writeln!(out, "no runnable processes"),
    }
}

fn kill_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(pid) = args.first().and_then(|v| v.parse::<u64>().ok()) else {
        return writeln!(out, "usage: kill <pid>");
    };
    let Some(frames) = nucleus.frames.as_mut() else {
        return writeln!(out, "no memory map installed");
    };
    if nucleus.procs.exit(pid, frames) {
        nucleus.preempt.remove(pid);
        writeln!(out, "killed pid {pid}, frames reclaimed, descheduled")
    } else {
        writeln!(out, "no process {pid}")
    }
}

fn top_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let rt = &nucleus.preempt;
    writeln!(
        out,
        "preemptive scheduler @ {} ticks, {} switches, {} preemptions — quantum is set by proof",
        rt.ticks(),
        rt.switches(),
        rt.total_preemptions()
    )?;
    if rt.thread_count() == 0 {
        return writeln!(out, "(no scheduled threads — `exec <tier>` to add one)");
    }
    writeln!(
        out,
        "{:<4} {:<12} {:<9} {:>7} {:>7} {:>6} {:>8}  STATE",
        "PID", "NAME", "TIER", "QUANTUM", "CPU", "CPU%", "PREEMPT"
    )?;
    for t in nucleus.preempt.threads() {
        let current = if rt.current() == Some(t.pid) {
            "*"
        } else {
            " "
        };
        let name = nucleus
            .procs
            .get(t.pid)
            .map(|p| p.name.as_str())
            .unwrap_or("-");
        writeln!(
            out,
            "{current}{:<3} {:<12} {:<9} {:>7} {:>7} {:>5}‰ {:>8}  {:?}",
            t.pid,
            name,
            t.tier.name(),
            crate::preempt::quantum_for(t.tier),
            t.cpu_ticks,
            rt.cpu_share_milli(t.pid),
            t.preemptions,
            t.state
        )?;
    }
    Ok(())
}

fn quantum_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let n: u64 = args.first().and_then(|v| v.parse().ok()).unwrap_or(50);
    use crate::preempt::Tick;
    let events = nucleus.preempt.advance(n);
    let preemptions = events
        .iter()
        .filter(|e| matches!(e, Tick::Preempted { .. }))
        .count();
    writeln!(
        out,
        "advanced {n} timer ticks: {preemptions} preemption(s) — proven threads ran on, \
         unproven ones were forced off the CPU"
    )?;
    // Show the first few switch events so the proof-weighting is visible.
    for event in events.iter().take(6) {
        if let Tick::Preempted { from, to } = event {
            let name = |pid: &u64| {
                nucleus
                    .procs
                    .get(*pid)
                    .map(|p| p.tier.name())
                    .unwrap_or("?")
            };
            writeln!(
                out,
                "  preempted pid {from} ({}) → pid {to} ({})",
                name(from),
                name(to)
            )?;
        }
    }
    Ok(())
}

// ── networking ───────────────────────────────────────────────────────────────

fn parse_ipv4(s: &str) -> Option<crate::net::Ipv4Addr> {
    let mut octets = [0u8; 4];
    let mut i = 0;
    for part in s.split('.') {
        if i >= 4 {
            return None;
        }
        octets[i] = part.parse().ok()?;
        i += 1;
    }
    (i == 4).then_some(octets)
}

fn parse_tier(s: &str) -> Option<Tier> {
    match s {
        "proven" => Some(Tier::Proven),
        "partial" => Some(Tier::Partial),
        "unproven" => Some(Tier::Unproven),
        _ => None,
    }
}

fn fmt_ip(ip: crate::net::Ipv4Addr) -> String {
    alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

fn fmt_mac(mac: crate::net::MacAddr) -> String {
    alloc::format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    )
}

fn net_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let n = &nucleus.net;
    writeln!(out, "iface: ip {} mac {}", fmt_ip(n.ip), fmt_mac(n.mac))?;
    writeln!(
        out,
        "arp cache: {} entr(ies)  sockets: {}",
        n.arp_len(),
        n.socket_count()
    )?;
    writeln!(
        out,
        "egress policy: {}",
        if n.egress_requires_proof {
            "proof-gated — unproven code is denied the wire"
        } else {
            "open"
        }
    )?;
    let s = &n.stats;
    writeln!(
        out,
        "tx {} frames ({} udp, {} icmp)  rx {} frames ({} udp, {} icmp)  arp-replies {}  egress-denied {}",
        s.tx_frames, s.tx_udp, s.tx_icmp, s.rx_frames, s.rx_udp, s.rx_icmp, s.arp_replies, s.egress_denied
    )
}

fn ping_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(ip) = args.first().and_then(|s| parse_ipv4(s)) else {
        return writeln!(out, "usage: ping <ip> [count]");
    };
    let count: u16 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(3);
    'seqs: for seq in 1..=count {
        // A real wire has real latency: the first attempt emits an ARP
        // request and reports pending, and replies land whenever the network
        // feels like it. Wait bounded against the hardware clock — on
        // loopback everything still resolves on the first poll.
        let deadline = nucleus.sched.now() + 200;
        let mut sent = false;
        loop {
            match nucleus.net.ping(ip, seq, Tier::Proven) {
                Ok(()) => {
                    sent = true;
                    break;
                }
                Err(crate::net::NetError::ArpPending) => {
                    nucleus.net.poll_until_quiet();
                    if nucleus.sched.now() >= deadline {
                        break;
                    }
                    core::hint::spin_loop();
                }
                Err(e) => {
                    writeln!(out, "seq {seq}: {e:?}")?;
                    continue 'seqs;
                }
            }
        }
        if !sent {
            writeln!(out, "seq {seq}: request timed out (arp unresolved)")?;
            continue;
        }
        let replied = loop {
            nucleus.net.poll_until_quiet();
            if let Some(reply) = nucleus.net.take_ping_reply() {
                break Some(reply);
            }
            if nucleus.sched.now() >= deadline {
                break None;
            }
            core::hint::spin_loop();
        };
        match replied {
            Some((from, s)) => writeln!(out, "reply from {} icmp_seq={s}", fmt_ip(from))?,
            None => writeln!(out, "seq {seq}: request timed out")?,
        }
    }
    Ok(())
}

fn arp_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    if nucleus.net.arp.is_empty() {
        return writeln!(out, "arp cache empty");
    }
    for (ip, mac) in nucleus.net.arp.iter() {
        writeln!(out, "{}  {}", fmt_ip(*ip), fmt_mac(*mac))?;
    }
    Ok(())
}

fn udp_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    match args {
        ["bind", port] => match port.parse::<u16>() {
            Ok(p) => {
                nucleus.net.bind(p);
                writeln!(out, "bound udp socket on port {p}")
            }
            Err(_) => writeln!(out, "bad port"),
        },
        ["send", ip, port, rest @ ..] if !rest.is_empty() => {
            let (Some(dst), Ok(dp)) = (parse_ipv4(ip), port.parse::<u16>()) else {
                return writeln!(out, "usage: udp send <ip> <port> <text...>");
            };
            let text = rest.join(" ");
            let mut r = nucleus
                .net
                .send_udp(dst, dp, 5000, text.as_bytes(), Tier::Proven);
            if let Err(crate::net::NetError::ArpPending) = r {
                nucleus.net.poll_until_quiet();
                r = nucleus
                    .net
                    .send_udp(dst, dp, 5000, text.as_bytes(), Tier::Proven);
            }
            match r {
                Ok(()) => {
                    nucleus.net.poll_until_quiet();
                    writeln!(out, "sent {} bytes to {}:{dp}", text.len(), fmt_ip(dst))
                }
                Err(e) => writeln!(out, "send failed: {e:?}"),
            }
        }
        ["recv", port] => match port.parse::<u16>() {
            Ok(p) => match nucleus.net.recv_udp(p) {
                Ok(Some(dg)) => writeln!(
                    out,
                    "from {}:{}  {}",
                    fmt_ip(dg.src_ip),
                    dg.src_port,
                    String::from_utf8_lossy(&dg.payload)
                ),
                Ok(None) => writeln!(out, "no datagram waiting on port {p}"),
                Err(_) => writeln!(out, "port {p} is not bound"),
            },
            Err(_) => writeln!(out, "bad port"),
        },
        _ => writeln!(
            out,
            "usage: udp bind <port> | send <ip> <port> <text...> | recv <port>"
        ),
    }
}

fn tcp_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let state_name =
        |n: &Nucleus, id: usize| n.net.tcp_state(id).map(|s| s.name()).unwrap_or("CLOSED");
    match args {
        [] => {
            let table = nucleus.net.tcp_table();
            if table.is_empty() {
                return writeln!(out, "no tcp connections");
            }
            writeln!(out, "ID   STATE         REMOTE")?;
            for (id, st, ip, port) in table {
                writeln!(out, "{id:<4} {:<13} {}:{port}", st.name(), fmt_ip(ip))?;
            }
            Ok(())
        }
        ["listen", port] => match port.parse::<u16>() {
            Ok(p) => {
                nucleus.net.tcp_listen(p);
                writeln!(out, "listening on tcp port {p}")
            }
            Err(_) => writeln!(out, "bad port"),
        },
        ["connect", ip, port] => {
            let (Some(dst), Ok(p)) = (parse_ipv4(ip), port.parse::<u16>()) else {
                return writeln!(out, "usage: tcp connect <ip> <port>");
            };
            match nucleus.net.tcp_connect(dst, p, Tier::Proven) {
                Ok(id) => {
                    nucleus.net.poll_until_quiet();
                    writeln!(
                        out,
                        "conn {id} → {}:{p} — {}",
                        fmt_ip(dst),
                        state_name(nucleus, id)
                    )
                }
                Err(e) => writeln!(out, "connect failed: {e:?}"),
            }
        }
        ["accept", port] => match port.parse::<u16>() {
            Ok(p) => match nucleus.net.tcp_accept(p) {
                Some(id) => writeln!(
                    out,
                    "accepted conn {id} on port {p} — {}",
                    state_name(nucleus, id)
                ),
                None => writeln!(out, "no pending connection on port {p}"),
            },
            Err(_) => writeln!(out, "bad port"),
        },
        ["send", id, rest @ ..] if !rest.is_empty() => {
            let Ok(i) = id.parse::<usize>() else {
                return writeln!(out, "bad conn id");
            };
            let text = rest.join(" ");
            match nucleus.net.tcp_send(i, text.as_bytes()) {
                Ok(()) => {
                    nucleus.net.poll_until_quiet();
                    writeln!(out, "sent {} bytes on conn {i}", text.len())
                }
                Err(e) => writeln!(out, "send failed: {e:?}"),
            }
        }
        ["recv", id] => {
            let Ok(i) = id.parse::<usize>() else {
                return writeln!(out, "bad conn id");
            };
            let data = nucleus.net.tcp_recv(i);
            if data.is_empty() {
                writeln!(out, "no data on conn {i}")
            } else {
                writeln!(out, "conn {i}: {}", String::from_utf8_lossy(&data))
            }
        }
        ["close", id] => {
            let Ok(i) = id.parse::<usize>() else {
                return writeln!(out, "bad conn id");
            };
            nucleus.net.tcp_close(i);
            nucleus.net.poll_until_quiet();
            writeln!(out, "conn {i} — {}", state_name(nucleus, i))
        }
        _ => writeln!(
            out,
            "usage: tcp | listen <port> | connect <ip> <port> | accept <port> | \
             send <id> <text...> | recv <id> | close <id>"
        ),
    }
}

/// The ephemeral UDP port the resolver client uses.
const DNS_CLIENT_PORT: u16 = 33053;
/// QEMU slirp's built-in stub resolver.
const DEFAULT_DNS: crate::net::Ipv4Addr = [10, 0, 2, 3];

/// Resolve `name` to its A records over real UDP, waiting out real-wire
/// latency against the hardware clock (bounded).
fn resolve_a(
    nucleus: &mut Nucleus,
    name: &str,
    server: crate::net::Ipv4Addr,
) -> Result<alloc::vec::Vec<crate::dns::Answer>, String> {
    let id = (nucleus.sched.now() as u16) | 1;
    let query = crate::dns::build_query(id, name).map_err(|e| alloc::format!("bad name: {e:?}"))?;
    nucleus.net.bind(DNS_CLIENT_PORT);
    // Drain anything stale from an earlier query on the same port.
    while let Ok(Some(_)) = nucleus.net.recv_udp(DNS_CLIENT_PORT) {}

    let deadline = nucleus.sched.now() + 300;
    // The wire needs ARP first; keep offering the datagram until it goes out.
    loop {
        match nucleus
            .net
            .send_udp(server, 53, DNS_CLIENT_PORT, &query, Tier::Proven)
        {
            Ok(()) => break,
            Err(crate::net::NetError::ArpPending) => {
                nucleus.net.poll_until_quiet();
                if nucleus.sched.now() >= deadline {
                    return Err(String::from("arp: no route to the resolver"));
                }
                core::hint::spin_loop();
            }
            Err(e) => return Err(alloc::format!("send failed: {e:?}")),
        }
    }
    loop {
        nucleus.net.poll_until_quiet();
        if let Ok(Some(datagram)) = nucleus.net.recv_udp(DNS_CLIENT_PORT) {
            return crate::dns::parse_response(&datagram.payload, id)
                .map_err(|e| alloc::format!("bad response: {e:?}"));
        }
        if nucleus.sched.now() >= deadline {
            return Err(String::from("query timed out"));
        }
        core::hint::spin_loop();
    }
}

fn dns_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(name) = args.first() else {
        return writeln!(out, "usage: dns <name> [server]");
    };
    let server = args
        .get(1)
        .and_then(|s| parse_ipv4(s))
        .unwrap_or(DEFAULT_DNS);
    match resolve_a(nucleus, name, server) {
        Ok(answers) if answers.is_empty() => writeln!(out, "{name}: no records"),
        Ok(answers) => {
            for answer in &answers {
                match answer {
                    crate::dns::Answer::A(ip) => writeln!(out, "{name} A {}", fmt_ip(*ip))?,
                    crate::dns::Answer::Cname(target) => writeln!(out, "{name} CNAME {target}")?,
                }
            }
            Ok(())
        }
        Err(e) => writeln!(out, "dns {name}: {e}"),
    }
}

/// Resolve + connect + GET + read: the whole stack, end to end, against the
/// actual internet (through whatever gateway the NIC is on).
/// The kernel HTTP daemon: start it on a port, or report what it has served.
/// The daemon itself is pumped from the platform idle loop
/// ([`crate::Nucleus::pump_services`]); the shell only manages its lifecycle.
fn httpd_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    match args {
        ["start"] | ["start", _] => {
            let port: u16 = args.get(1).and_then(|v| v.parse().ok()).unwrap_or(80);
            if let Some(existing) = &nucleus.httpd {
                return writeln!(
                    out,
                    "httpd already running on port {} — one daemon per kernel",
                    existing.port()
                );
            }
            nucleus.httpd = Some(crate::httpd::Httpd::start(port, &mut nucleus.net));
            writeln!(
                out,
                "httpd: listening on {}:{port} — serving the VFS over the kernel's own tcp",
                fmt_ip(nucleus.net.ip)
            )
        }
        ["status"] | [] => match &nucleus.httpd {
            Some(httpd) => {
                let s = &httpd.stats;
                writeln!(
                    out,
                    "httpd on port {}: accepted={} served={} 404={} bad={} bytes={} open={}",
                    httpd.port(),
                    s.accepted,
                    s.served,
                    s.not_found,
                    s.bad_requests,
                    s.bytes_sent,
                    httpd.open_conns()
                )
            }
            None => writeln!(out, "httpd not running (start with: httpd start [port])"),
        },
        _ => writeln!(out, "usage: httpd start [port] | status"),
    }
}

fn fetch_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(host) = args.first().copied() else {
        return writeln!(out, "usage: fetch <host> [path]");
    };
    let path = args.get(1).copied().unwrap_or("/");

    let ip = match resolve_a(nucleus, host, DEFAULT_DNS) {
        Ok(answers) => match crate::dns::a_records(&answers).first().copied() {
            Some(ip) => ip,
            None => return writeln!(out, "fetch: {host} has no A records"),
        },
        Err(e) => return writeln!(out, "fetch: dns: {e}"),
    };
    writeln!(out, "fetch: {host} → {}", fmt_ip(ip))?;

    let id = match nucleus.net.tcp_connect(ip, 80, Tier::Proven) {
        Ok(id) => id,
        Err(e) => return writeln!(out, "fetch: connect: {e:?}"),
    };
    let deadline = nucleus.sched.now() + 600;
    loop {
        nucleus.net.poll_until_quiet();
        let tick_now = nucleus.sched.now();
        nucleus.net.tick(tick_now);
        match nucleus.net.tcp_state(id) {
            Some(crate::tcp::TcpState::Established) => break,
            Some(crate::tcp::TcpState::Closed) | None => {
                return writeln!(out, "fetch: connection refused");
            }
            _ => {}
        }
        if nucleus.sched.now() >= deadline {
            return writeln!(out, "fetch: connect timed out");
        }
        core::hint::spin_loop();
    }

    let request =
        alloc::format!("GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if let Err(e) = nucleus.net.tcp_send(id, request.as_bytes()) {
        return writeln!(out, "fetch: send: {e:?}");
    }

    // Read until the server finishes (FIN → CloseWait/Closed) or we time out.
    let mut body = alloc::vec::Vec::new();
    loop {
        nucleus.net.poll_until_quiet();
        let tick_now = nucleus.sched.now();
        nucleus.net.tick(tick_now);
        body.extend_from_slice(&nucleus.net.tcp_recv(id));
        match nucleus.net.tcp_state(id) {
            Some(crate::tcp::TcpState::CloseWait) | Some(crate::tcp::TcpState::Closed) | None => {
                // One last drain — data and FIN can land in the same poll.
                body.extend_from_slice(&nucleus.net.tcp_recv(id));
                break;
            }
            _ => {}
        }
        if nucleus.sched.now() >= deadline {
            break;
        }
        core::hint::spin_loop();
    }
    nucleus.net.tcp_close(id);
    nucleus.net.poll_until_quiet();
    let tick_now = nucleus.sched.now();
    nucleus.net.tick(tick_now);

    if body.is_empty() {
        return writeln!(out, "fetch: no data (timed out)");
    }
    writeln!(out, "fetch: {} bytes from {host}:", body.len())?;
    // Print the status line + headers + a taste of the body, printably.
    let shown = body.len().min(400);
    for &b in &body[..shown] {
        match b {
            b'\n' => writeln!(out)?,
            b'\r' => {}
            0x20..=0x7E => out.write_char(b as char)?,
            _ => out.write_char('.')?,
        }
    }
    if body.len() > shown {
        writeln!(out, "\n… ({} more bytes)", body.len() - shown)?;
    }
    writeln!(out)
}

fn pci_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    if nucleus.pci.is_empty() {
        return writeln!(
            out,
            "no PCI devices enumerated (host build, or the boot layer has not scanned yet)"
        );
    }
    writeln!(out, "ADDR      VEND:DEV   CLASS  DESCRIPTION")?;
    for d in &nucleus.pci {
        writeln!(
            out,
            "{:02x}:{:02x}.{}  {:04x}:{:04x}  {:02x}.{:02x}  {} ({}){}",
            d.bus,
            d.slot,
            d.func,
            d.vendor,
            d.device,
            d.class,
            d.subclass,
            d.class_name(),
            d.vendor_name(),
            if d.is_virtio() { " [virtio]" } else { "" },
        )?;
    }
    writeln!(out, "{} device(s)", nucleus.pci.len())
}

/// Report the CPU topology the boot layer discovered (ACPI) and enacted (SMP).
fn cpus_cmd(nucleus: &Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let t = nucleus.cpu_topology;
    writeln!(
        out,
        "cpu topology: {} core(s) total, boot core + {} application core(s) started",
        t.cores_total, t.cores_online
    )?;
    if t.local_apic != 0 {
        writeln!(out, "local apic: {:#x}", t.local_apic)?;
    }
    if t.cores_online == 0 && t.cores_total <= 1 {
        writeln!(out, "(uniprocessor, or hosted runner — no SMP bring-up)")?;
    }
    Ok(())
}

/// Exercise the from-scratch TLS crypto suite from the shell — proof the
/// primitives run in the live kernel, not just in tests.
fn crypto_cmd(args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let hex = |bytes: &[u8]| -> alloc::string::String {
        let mut s = alloc::string::String::new();
        for b in bytes {
            let _ = write!(s, "{b:02x}");
        }
        s
    };
    match args {
        ["sha256", rest @ ..] => {
            let msg = rest.join(" ");
            let d = crate::crypto::sha256(msg.as_bytes());
            writeln!(out, "sha256 = {}", crate::crypto::hex32(&d))
        }
        ["hmac", key, rest @ ..] if !rest.is_empty() => {
            let mac = crate::crypto::hmac_sha256(key.as_bytes(), rest.join(" ").as_bytes());
            writeln!(out, "hmac-sha256 = {}", hex(&mac))
        }
        ["hkdf", ikm, info] => {
            let prk = crate::crypto::hkdf_extract(b"", ikm.as_bytes());
            let okm = crate::crypto::hkdf_expand(&prk, info.as_bytes(), 32);
            writeln!(out, "hkdf(32) = {}", hex(&okm))
        }
        ["x25519"] => {
            // A live Diffie-Hellman: two keypairs agree on a shared secret.
            let a_sk = [0x11u8; 32];
            let b_sk = [0x22u8; 32];
            let a_pk = crate::x25519::public_key(&a_sk);
            let b_pk = crate::x25519::public_key(&b_sk);
            let s_ab = crate::x25519::scalarmult(&a_sk, &b_pk);
            let s_ba = crate::x25519::scalarmult(&b_sk, &a_pk);
            writeln!(out, "x25519 A pub = {}", hex(&a_pk))?;
            writeln!(out, "x25519 B pub = {}", hex(&b_pk))?;
            writeln!(
                out,
                "shared secret {} ({})",
                hex(&s_ab),
                if s_ab == s_ba { "AGREED" } else { "MISMATCH" }
            )
        }
        _ => writeln!(
            out,
            "usage: crypto sha256 <text> | hmac <key> <text> | hkdf <ikm> <info> | x25519"
        ),
    }
}

/// Round-trip a sector through the virtio-blk driver against an in-kernel
/// device model — proof the split-virtqueue protocol runs live.
fn virtio_cmd(args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    if args.first() != Some(&"selftest") {
        return writeln!(out, "usage: virtio selftest");
    }
    match crate::virtio::shell_selftest() {
        Ok(()) => writeln!(
            out,
            "virtio-blk: sector written and read back through the split virtqueue — OK"
        ),
        Err(e) => writeln!(out, "virtio-blk selftest failed: {e:?}"),
    }
}

fn firewall_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let [tier, ip] = args else {
        return writeln!(out, "usage: firewall <proven|partial|unproven> <ip>");
    };
    let (Some(t), Some(dst)) = (parse_tier(tier), parse_ipv4(ip)) else {
        return writeln!(out, "usage: firewall <proven|partial|unproven> <ip>");
    };
    let ok = nucleus.net.egress_permitted(t, dst);
    writeln!(
        out,
        "{} → {}: {}",
        t.name(),
        fmt_ip(dst),
        if ok {
            "PERMITTED"
        } else {
            "DENIED — unproven code may not reach the wire"
        }
    )
}

fn sys_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    use crate::syscall::{dispatch, SysValue, Syscall};
    let Some(pid) = args.first().and_then(|v| v.parse::<u64>().ok()) else {
        return writeln!(out, "usage: sys <pid> <call> ...");
    };
    let call = match args.get(1).copied() {
        Some("getpid") => Syscall::GetPid,
        Some("log") => Syscall::Log(args[2..].join(" ")),
        Some("read") => match args.get(2) {
            Some(path) => Syscall::Read {
                path: (*path).to_string(),
            },
            None => return writeln!(out, "usage: sys <pid> read <path>"),
        },
        Some("write") => match args.get(2) {
            Some(path) => Syscall::Write {
                path: (*path).to_string(),
                data: args[3..].join(" ").into_bytes(),
            },
            None => return writeln!(out, "usage: sys <pid> write <path> <text...>"),
        },
        Some("spawn") => {
            let tier = match args.get(2).copied() {
                Some("proven") => Tier::Proven,
                Some("partial") => Tier::Partial,
                Some("unproven") | None => Tier::Unproven,
                Some(other) => return writeln!(out, "unknown tier '{other}'"),
            };
            let name = args.get(3).copied().unwrap_or("child").to_string();
            Syscall::Spawn {
                name,
                tier,
                elf: crate::elf::synth_executable(DEMO_VADDR, &DEMO_CODE),
            }
        }
        _ => return writeln!(out, "unknown call — try getpid|log|read|write|spawn"),
    };
    match dispatch(nucleus, pid, call) {
        Ok(value) => match value {
            SysValue::Pid(p) => writeln!(out, "= pid {p}"),
            SysValue::Logged(m) => writeln!(out, "[pid {pid}] {m}"),
            SysValue::Bytes(b) => match core::str::from_utf8(&b) {
                Ok(text) => writeln!(out, "= {} bytes: {text}", b.len()),
                Err(_) => writeln!(out, "= {} bytes (binary)", b.len()),
            },
            SysValue::Wrote(n) => writeln!(out, "= wrote {n} bytes"),
            SysValue::Spawned(p) => writeln!(out, "= spawned pid {p}"),
            SysValue::Exited(c) => writeln!(out, "= exited {c}"),
            SysValue::Ok => writeln!(out, "= ok"),
        },
        Err(e) => writeln!(
            out,
            "EPERM/error: {e:?}  (authority is set by proof, not by identity)"
        ),
    }
}

// ── operator primitives (os/nucleus/src/ops.rs) ──────────────────────────

fn ensure_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let (Some(key), Some(_)) = (args.first(), args.get(1)) else {
        return writeln!(out, "usage: ensure <key> <value...>");
    };
    let value = args[1..].join(" ");
    let now = nucleus.sched.now();
    let report = nucleus.ops.ensure(&mut nucleus.store, key, &value, now);
    use crate::ops::EnsureOutcome;
    match report.outcome {
        EnsureOutcome::AlreadyHeld => {
            writeln!(out, "ensure {key}: already holds — {}", report.detail)
        }
        EnsureOutcome::Converged => writeln!(
            out,
            "ensure {key}={value}: converged (was {:?}) — proven & committed, undoable",
            report.prior
        ),
        EnsureOutcome::Failed => writeln!(out, "ensure {key}: FAILED — {}", report.detail),
    }
}

fn undo_cmd(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let now = nucleus.sched.now();
    match nucleus.ops.undo(&mut nucleus.store, now) {
        Some(desc) => writeln!(out, "{desc}  ({} more to undo)", nucleus.ops.undo_depth()),
        None => writeln!(out, "nothing to undo"),
    }
}

fn redo_cmd(nucleus: &mut Nucleus, out: &mut dyn Write) -> core::fmt::Result {
    let now = nucleus.sched.now();
    match nucleus.ops.redo(&mut nucleus.store, now) {
        Some(desc) => writeln!(out, "{desc}  ({} more to redo)", nucleus.ops.redo_depth()),
        None => writeln!(out, "nothing to redo"),
    }
}

fn dry_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let (Some(key), Some(_)) = (args.first(), args.get(1)) else {
        return writeln!(out, "usage: dry <key> <value...>  (previews `ensure`)");
    };
    let value = args[1..].join(" ");
    let preview = nucleus.ops.dry_run(&nucleus.store, key, &value);
    if preview.would_change {
        writeln!(
            out,
            "dry: `ensure {key}` WOULD change {:?} -> \"{value}\" (nothing was touched — guaranteed)",
            preview.current
        )
    } else {
        writeln!(
            out,
            "dry: `ensure {key}` is already satisfied — would do nothing"
        )
    }
}

fn why_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    match args {
        ["kv", key] => match nucleus.ops.why_key(key) {
            Some(explanation) => writeln!(out, "{explanation}"),
            None => writeln!(
                out,
                "'{key}' has no recorded cause (set outside `ensure`, or never set)"
            ),
        },
        ["task", id] => {
            let Ok(task_id) = id.parse::<u64>() else {
                return writeln!(out, "usage: why task <id>");
            };
            let moves: Vec<_> = nucleus
                .ledger
                .transitions()
                .iter()
                .filter(|t| t.task == task_id)
                .collect();
            if moves.is_empty() {
                return writeln!(
                    out,
                    "task {task_id}: no tier transitions — it never left its birth tier"
                );
            }
            writeln!(out, "task {task_id} tier history (the causal chain):")?;
            for t in moves {
                writeln!(
                    out,
                    "  tick {}: {} -> {} ({})",
                    t.at_tick,
                    t.from.name(),
                    t.to.name(),
                    t.reason
                )?;
            }
            Ok(())
        }
        ["proc", id] => {
            let Ok(pid) = id.parse::<u64>() else {
                return writeln!(out, "usage: why proc <pid>");
            };
            match nucleus.procs.get(pid) {
                Some(p) => writeln!(
                    out,
                    "proc {pid} '{}' is tier {} because that is what its proofs earned; \
                     its capabilities (fs_write={}, spawn={}, device={}) follow from the tier, \
                     not from any user identity",
                    p.name,
                    p.tier.name(),
                    p.caps.fs_write,
                    p.caps.spawn,
                    p.caps.raw_device
                ),
                None => writeln!(out, "no process {pid}"),
            }
        }
        _ => writeln!(out, "usage: why <kv <key> | task <id> | proc <pid>>"),
    }
}

fn prove_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(pred) = args.first() else {
        return writeln!(out, "usage: prove <predicate> [recheck]");
    };
    let recheck = args.get(1).is_some_and(|a| *a == "recheck");
    let now = nucleus.sched.now();
    match nucleus.ops.prove(pred, &nucleus.store, &nucleus.fs, now, recheck) {
        Ok((holds, from_cache)) => writeln!(
            out,
            "{pred}: {} {}",
            if holds { "HOLDS" } else { "does not hold" },
            if from_cache {
                "(recalled — not recomputed)"
            } else {
                "(checked and remembered)"
            }
        ),
        Err(_) => writeln!(
            out,
            "unparseable predicate '{pred}' — use kv:<k>=<v> | kv:<k>~<t> | file:<path> | file:<path>~<t>"
        ),
    }
}

fn recall_cmd(nucleus: &Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    let Some(pred) = args.first() else {
        return writeln!(out, "usage: recall <predicate>");
    };
    match nucleus.ops.recall(pred) {
        Some(fact) => writeln!(
            out,
            "{pred}: {} — proven at tick {} ({} check(s) ever), recalled instantly",
            if fact.holds { "HOLDS" } else { "does not hold" },
            fact.proven_at_tick,
            fact.checks
        ),
        None => writeln!(out, "{pred}: not yet proven — run `prove {pred}` first"),
    }
}

fn when_cmd(nucleus: &mut Nucleus, args: &[&str], out: &mut dyn Write) -> core::fmt::Result {
    // Syntax: when <predicate> do <command...>
    let Some(do_at) = args.iter().position(|a| *a == "do") else {
        return writeln!(out, "usage: when <predicate> do <command...>");
    };
    if do_at != 1 {
        return writeln!(out, "usage: when <predicate> do <command...>");
    }
    let pred = args[0];
    let command = args[do_at + 1..].join(" ");
    if command.is_empty() {
        return writeln!(out, "usage: when <predicate> do <command...>");
    }
    let now = nucleus.sched.now();
    match nucleus.ops.arm(pred, &command, now) {
        Ok(id) => writeln!(
            out,
            "armed trigger #{id}: when `{pred}` holds → `{command}` (fires once, no polling)"
        ),
        Err(_) => writeln!(out, "unparseable predicate '{pred}'"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box as StdBox;

    fn nucleus() -> Nucleus {
        let tick = core::cell::Cell::new(0u64);
        // Leak a Cell-based clock for the test nucleus (std test context).
        let tick: &'static core::cell::Cell<u64> = StdBox::leak(StdBox::new(tick));
        let mut nucleus = Nucleus::new(
            StdBox::new(move || {
                tick.set(tick.get() + 1);
                tick.get()
            }),
            None,
        );
        // A synthetic memory map so `frames`/`exec` are live in tests too.
        nucleus.install_memory_map(&[
            crate::mem::MemoryRegion::reserved(0, 0x1000),
            crate::mem::MemoryRegion::usable(0x1000, 64 * 1024 * 1024),
        ]);
        nucleus
    }

    fn run_line(nucleus: &mut Nucleus, line: &str) -> String {
        let mut out = String::new();
        exec(nucleus, line, &mut out);
        out
    }

    #[test]
    fn spawn_run_ps_and_the_distrust_tax_are_visible() {
        let mut n = nucleus();
        run_line(&mut n, "spawn proven 50");
        run_line(&mut n, "spawn unproven 50");
        run_line(&mut n, "run 40");
        let ps = run_line(&mut n, "ps");
        assert!(ps.contains("demo-proven-1"));
        assert!(ps.contains("demo-unproven-2"));
        let tiers = run_line(&mut n, "tiers");
        assert!(tiers.contains("proven"));
    }

    #[test]
    fn kairos_decomposes_the_index_and_accepts_arrivals() {
        let mut n = nucleus();
        run_line(&mut n, "spawn proven 50");
        run_line(&mut n, "run 8");
        let pushed = run_line(&mut n, "kairos push 1 40");
        assert!(pushed.contains("+40 pending units"), "{pushed}");
        let table = run_line(&mut n, "kairos");
        assert!(table.contains("serve max K"), "{table}");
        assert!(table.contains("demo-proven-1"), "{table}");
        assert!(table.contains("40"), "declared backlog visible: {table}");
        let bogus = run_line(&mut n, "kairos push 99 5");
        assert!(bogus.contains("no runnable task"), "{bogus}");
    }

    #[test]
    fn drift_resync_transitions_flow_works_from_the_shell() {
        let mut n = nucleus();
        run_line(&mut n, "spawn proven 100");
        run_line(&mut n, "run 8");
        run_line(&mut n, "drift 1");
        let resynced = run_line(&mut n, "resync");
        assert!(resynced.contains("1 task(s) changed tier"));
        let log = run_line(&mut n, "transitions");
        assert!(log.contains("proven -> partial"));
    }

    #[test]
    fn intent_demo_shows_fusion_and_rollback() {
        let mut n = nucleus();
        let output = run_line(&mut n, "intent-demo");
        assert!(output.contains("4 ops submitted -> 2 executed"));
        assert!(output.contains("committed=true"));
        assert!(output.contains("committed=false"));
        assert!(output.contains("collateral=None"));
    }

    #[test]
    fn kv_round_trip_via_intents() {
        let mut n = nucleus();
        run_line(&mut n, "kv put greeting hello world");
        let get = run_line(&mut n, "kv get greeting");
        assert!(get.contains("hello world"));
        run_line(&mut n, "kv del greeting");
        assert!(run_line(&mut n, "kv get greeting").contains("absent"));
    }

    #[test]
    fn ipcbench_runs_and_reports() {
        let n = nucleus();
        let mut out = String::new();
        ipcbench(&n, &["100"], &mut out).unwrap();
        assert!(out.contains("zero-copy ownership handoff"));
    }

    #[test]
    fn spu_executive_flows_work_from_the_shell() {
        let mut n = nucleus();
        run_line(&mut n, "ctx new researcher 32 5000");
        run_line(&mut n, "ctx new archivist 32 1000");
        for _ in 0..10 {
            run_line(&mut n, "ctx touch 1");
        }
        let placed = run_line(&mut n, "ctx place");
        assert!(placed.contains("migrations"));
        let list = run_line(&mut n, "ctx list");
        assert!(list.contains("researcher"));

        let graft = run_line(&mut n, "graft 1 2 4000");
        assert!(graft.contains("COMMITTED"), "{graft}");
        let tiny = run_line(&mut n, "graft 1 2 5");
        assert!(tiny.contains("declined"), "{tiny}");

        let reason = run_line(&mut n, "reason 5000 900");
        assert!(reason.contains("best=candidate-"), "{reason}");

        run_line(&mut n, "modes demand stoch 40");
        run_line(&mut n, "modes demand det 40");
        let served = run_line(&mut n, "modes service 30");
        assert!(served.contains("served"), "{served}");

        run_line(&mut n, "consolidate record grasp ok 800");
        run_line(&mut n, "consolidate record grasp ok 800");
        let consolidated = run_line(&mut n, "consolidate");
        assert!(consolidated.contains("grasp"), "{consolidated}");

        let status = run_line(&mut n, "spu");
        assert!(status.contains("wear pacing"), "{status}");
    }

    #[test]
    fn filesystem_commands_persist_across_a_sync_fsck() {
        let mut n = nucleus();
        run_line(&mut n, "mkdir /etc");
        run_line(&mut n, "write /etc/motd proof buys speed");
        let cat = run_line(&mut n, "cat /etc/motd");
        assert!(cat.contains("proof buys speed"), "{cat}");
        let ls = run_line(&mut n, "ls /etc");
        assert!(ls.contains("motd"), "{ls}");

        // Snapshot to the persistent tier, wipe the live tree, reload.
        let synced = run_line(&mut n, "sync");
        assert!(synced.contains("snapshotted"), "{synced}");
        run_line(&mut n, "rm /etc/motd");
        assert!(run_line(&mut n, "cat /etc/motd").contains("NotFound"));
        let fsck = run_line(&mut n, "fsck");
        assert!(fsck.contains("reloaded"), "{fsck}");
        assert!(
            run_line(&mut n, "cat /etc/motd").contains("proof buys speed"),
            "data must survive the power cycle"
        );
    }

    #[test]
    fn processes_load_isolate_and_schedule_from_the_shell() {
        let mut n = nucleus();
        let proven = run_line(&mut n, "exec proven trusted");
        assert!(proven.contains("isolated"), "{proven}");
        let unproven = run_line(&mut n, "exec unproven sandbox");
        assert!(unproven.contains("pid 2"), "{unproven}");

        // The process table shows tier-derived capabilities.
        let proc = run_line(&mut n, "proc");
        assert!(
            proc.contains("trusted") && proc.contains("device"),
            "{proc}"
        );

        // pmap reads the loaded code back through the process's page tables.
        let pmap = run_line(&mut n, "pmap 1");
        assert!(pmap.contains("via its page tables"), "{pmap}");
        assert!(pmap.contains("b8"), "should show the demo opcode: {pmap}");

        // The scheduler favours the proven process.
        let mut proven_runs = 0;
        for _ in 0..12 {
            if run_line(&mut n, "sched").contains("'trusted'") {
                proven_runs += 1;
            }
        }
        assert!(proven_runs >= 6, "proven scheduled more, got {proven_runs}");

        let killed = run_line(&mut n, "kill 2");
        assert!(killed.contains("reclaimed"), "{killed}");
    }

    #[test]
    fn preemptive_scheduler_contains_a_runaway_from_the_shell() {
        let mut n = nucleus();
        // A well-behaved proven compute kernel and an unproven runaway.
        run_line(&mut n, "exec proven kernel");
        run_line(&mut n, "exec unproven runaway");
        // Their quanta differ by proof.
        let spawn = run_line(&mut n, "exec partial helper");
        assert!(spawn.contains("quantum=5"), "{spawn}");

        // Advance the hardware timer a good while.
        let q = run_line(&mut n, "quantum 300");
        assert!(q.contains("preemption"), "{q}");

        // top shows the proven thread got the lion's share of CPU and the
        // unproven runaway was preempted many times — it could never hang it.
        let top = run_line(&mut n, "top");
        assert!(top.contains("kernel") && top.contains("runaway"), "{top}");
        assert!(top.contains("proof"), "{top}");
        let proven = n.preempt.cpu_share_milli(1);
        let unproven = n.preempt.cpu_share_milli(2);
        assert!(proven > unproven, "proven {proven}‰ > unproven {unproven}‰");
    }

    #[test]
    fn syscalls_enforce_proof_derived_capabilities_from_the_shell() {
        let mut n = nucleus();
        run_line(&mut n, "exec proven trusted");
        run_line(&mut n, "exec unproven sandbox");

        // The proven process may write the filesystem.
        let proven_write = run_line(&mut n, "sys 1 write /data hello");
        assert!(proven_write.contains("wrote 5 bytes"), "{proven_write}");

        // The unproven process is refused the same write — capability denied.
        let unproven_write = run_line(&mut n, "sys 2 write /data tampered");
        assert!(
            unproven_write.contains("PermissionDenied"),
            "{unproven_write}"
        );
        assert!(unproven_write.contains("FsWrite"), "{unproven_write}");

        // But reading is always allowed, and the data is intact.
        let read = run_line(&mut n, "sys 2 read /data");
        assert!(read.contains("hello"), "{read}");

        // getpid works for anyone.
        assert!(run_line(&mut n, "sys 2 getpid").contains("pid 2"));
    }

    #[test]
    fn operator_primitives_reduce_problems_from_the_shell() {
        let mut n = nucleus();

        // ensure: declarative + idempotent. First converges, second is a no-op.
        assert!(run_line(&mut n, "ensure svc running").contains("converged"));
        assert!(run_line(&mut n, "ensure svc running").contains("already holds"));

        // dry: preview a change without touching anything.
        let dry = run_line(&mut n, "dry svc stopped");
        assert!(dry.contains("WOULD change"), "{dry}");
        // The dry run really touched nothing: it still holds the old value.
        assert!(run_line(&mut n, "recall kv:svc=running").contains("not yet proven"));
        assert!(run_line(&mut n, "prove kv:svc=running").contains("HOLDS"));

        // undo: reverse the last real change (svc back to absent).
        assert!(run_line(&mut n, "undo").contains("restored"));
        assert!(run_line(&mut n, "prove kv:svc=running recheck").contains("does not hold"));
        // redo brings it back.
        assert!(run_line(&mut n, "redo").contains("svc"));

        // why: causal history, recorded not reconstructed.
        assert!(run_line(&mut n, "why kv svc").contains("was set by"));

        // recall: the cached verdict, no recompute.
        assert!(run_line(&mut n, "recall kv:svc=running").contains("recalled instantly"));

        // when: a reactive trigger fires on the predicate with no poll loop.
        run_line(&mut n, "when kv:ready=yes do ensure booted true");
        let fired = run_line(&mut n, "ensure ready yes");
        assert!(fired.contains("[when]"), "trigger should fire: {fired}");
        assert!(run_line(&mut n, "prove kv:booted=true").contains("HOLDS"));
    }
}
