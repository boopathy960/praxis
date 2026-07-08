//! praxis-host — the same nucleus the bare-metal image boots, driven from a
//! std console. The platform surface is identical (a clock and a console), so
//! what you exercise here is exactly the kernel core that ships to metal.
//!
//!   cargo run -p praxis-host            interactive praxsh
//!   cargo run -p praxis-host -- --demo  scripted tour (used for verification)

use std::io::{BufRead, Write as IoWrite};
use std::time::Instant;

use praxis_nucleus::mem::MemoryRegion;
use praxis_nucleus::Nucleus;

fn main() {
    let epoch = Instant::now();
    let clock = Box::new(move || epoch.elapsed().as_nanos() as u64);
    let mut nucleus = Nucleus::new(clock, None);
    // A synthetic memory map so the frame allocator is live on the hosted
    // runner too (bare metal gets the real one from the bootloader): 1 MiB
    // reserved, 256 MiB usable.
    nucleus.install_memory_map(&[
        MemoryRegion::reserved(0, 1024 * 1024),
        MemoryRegion::usable(1024 * 1024, 256 * 1024 * 1024),
    ]);
    let mut out = String::new();

    let demo = std::env::args().any(|arg| arg == "--demo");
    if demo {
        // The scripted tour: every novel mechanism, end to end.
        for line in [
            "uname",
            "spawn proven 200",
            "spawn partial 200",
            "spawn unproven 200",
            "run 300",
            "ps",
            "tiers",
            "suspects",
            "claims",
            "drift 1",
            "resync",
            "transitions",
            "run 100",
            "ipcbench 20000",
            "intent-demo",
            "kv put motd proof buys speed",
            "kv get motd",
            "mem",
            // The SPU executive (os/SPU-OS.md): contexts, placement, grafts,
            // anytime reasoning, mode economics, consolidation.
            "ctx new researcher 32 5000",
            "ctx new archivist 64 2000",
            "ctx touch 1",
            "ctx touch 1",
            "ctx touch 1",
            "ctx place",
            "ctx list",
            "graft 1 2 4000",
            "graft 1 2 5",
            "reason 5000 900",
            "modes demand stoch 40",
            "modes demand det 40",
            "modes service 30",
            "consolidate record grasp-cup ok 800",
            "consolidate record grasp-cup ok 800",
            "consolidate record open-door fail 900",
            "consolidate",
            "ctx suspend 2",
            "spu",
            // The storage stack: a real VFS over a block device, plus the
            // physical frame allocator.
            "mkdir /etc",
            "write /etc/motd proof buys speed",
            "mkdir /etc/agents",
            "write /etc/agents/researcher.ctx persistent reasoning state",
            "ls /etc",
            "cat /etc/motd",
            "stat /etc/agents/researcher.ctx",
            "sync",
            "rm /etc/motd",
            "fsck",
            "cat /etc/motd",
            "frames",
            // Processes & virtual memory: load ELF programs into isolated,
            // paged address spaces with tier-derived capabilities.
            "exec proven init",
            "exec unproven sandbox",
            "proc",
            "pmap 1",
            "pmap 2",
            "sched",
            "sched",
            "sched",
            // The syscall boundary: authority checked against proof, not uid.
            // The proven process may write; the unproven one is refused the
            // same call; both may read.
            "sys 1 write /data proven wrote this",
            "sys 2 write /data sandbox tampering",
            "sys 2 read /data",
            "sys 1 spawn unproven grandchild",
            "sys 2 spawn unproven blocked",
            "sys 2 getpid",
            "proc",
            "kill 2",
            "frames",
        ] {
            println!("praxsh> {line}");
            out.clear();
            nucleus.exec_line(line, &mut out);
            print!("{out}");
        }
        return;
    }

    println!("Praxis nucleus (hosted). `help` lists kernel commands; ctrl-d exits.");
    let stdin = std::io::stdin();
    loop {
        print!("praxsh> ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        out.clear();
        nucleus.exec_line(&line, &mut out);
        print!("{out}");
        // Keep the kernel alive between commands, like the bare-metal loop.
        nucleus.sched.run_slice(256);
        nucleus.net.poll();
        let now = nucleus.sched.now();
        nucleus.net.tick(now);
        nucleus.pump_services();
    }
}
