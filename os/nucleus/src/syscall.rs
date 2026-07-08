//! The syscall boundary — where proof becomes enforced authority.
//!
//! A syscall is how a process asks the kernel to do something it cannot do
//! itself. In every other OS the kernel then decides *may you?* by consulting a
//! uid, a group list, an SELinux label — ambient authority the process carries
//! because of *who ran it*. Praxis asks a different question: **what has been
//! proven about this code?** Each process's capability set is derived from its
//! [proof tier](crate::proof::Tier) at load time (see
//! [`crate::process::Capabilities`]); this dispatcher checks the required
//! capability on every call. An unproven process that asks to write the
//! filesystem or spawn a child is refused with `PermissionDenied` — not because
//! of who it is, but because nothing has proven it may. A proven one is trusted
//! because it earned it. This is the ambient-authority fix landing at the exact
//! boundary where authority is actually exercised.
//!
//! The typed [`Syscall`] enum is the in-kernel form; the numeric ABI constants
//! below are what the bare-metal `syscall`-instruction entry stub
//! (`boot/src/gdt.rs`) decodes register arguments into.

use alloc::string::String;
use alloc::vec::Vec;

use crate::fs::FsError;
use crate::process::{Cap, Pid, SpawnError};
use crate::proof::Tier;

/// Numeric syscall ABI (what `rax` holds at the `syscall` instruction).
pub mod nr {
    pub const LOG: u64 = 0;
    pub const GETPID: u64 = 1;
    pub const YIELD: u64 = 2;
    pub const EXIT: u64 = 3;
    pub const READ: u64 = 4;
    pub const WRITE: u64 = 5;
    pub const SPAWN: u64 = 6;
}

/// A decoded system call.
#[derive(Debug, Clone)]
pub enum Syscall {
    /// Emit a line to the kernel console. Always permitted.
    Log(String),
    /// The caller's own pid. Always permitted.
    GetPid,
    /// Yield the CPU to the scheduler. Always permitted.
    Yield,
    /// Terminate the caller with an exit code. Always permitted.
    Exit(i64),
    /// Read a whole file. Reading is always permitted (confinement is about
    /// what a process may *change* or *reach*, not what it may see of the FS).
    Read { path: String },
    /// Write a whole file. Requires [`Cap::FsWrite`].
    Write { path: String, data: Vec<u8> },
    /// Load an ELF as a new process at `tier`. Requires [`Cap::Spawn`].
    Spawn {
        name: String,
        tier: Tier,
        elf: Vec<u8>,
    },
}

impl Syscall {
    /// The capability this call requires, or `None` if unconditionally allowed.
    pub fn required_cap(&self) -> Option<Cap> {
        match self {
            Syscall::Write { .. } => Some(Cap::FsWrite),
            Syscall::Spawn { .. } => Some(Cap::Spawn),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SysError {
    /// The calling pid is unknown.
    NoSuchProcess,
    /// The process's proof tier does not grant the required capability.
    PermissionDenied(Cap),
    /// A filesystem error surfaced from the service.
    Fs(FsError),
    /// A process could not be spawned.
    Spawn(SpawnError),
    /// The service needs the frame allocator, which is not installed.
    NoMemoryMap,
}

/// What a successful syscall returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SysValue {
    Ok,
    Logged(String),
    Pid(Pid),
    Bytes(Vec<u8>),
    Wrote(usize),
    Spawned(Pid),
    Exited(i64),
}

pub type SyscallResult = Result<SysValue, SysError>;

/// Dispatch one syscall on behalf of `pid`, enforcing its proof-derived
/// capabilities before touching any kernel service. This is the single choke
/// point where a process's authority is checked against what its proofs earned.
pub fn dispatch(nucleus: &mut crate::Nucleus, pid: Pid, call: Syscall) -> SyscallResult {
    // The caller must exist, and we read its capabilities up front.
    let caps = nucleus.procs.get(pid).ok_or(SysError::NoSuchProcess)?.caps;

    // The universal gate: a required capability the tier did not grant is a
    // hard refusal, before the service ever runs.
    if let Some(required) = call.required_cap() {
        let granted = match required {
            Cap::FsWrite => caps.fs_write,
            Cap::Spawn => caps.spawn,
            Cap::RawDevice => caps.raw_device,
            Cap::InProcessExec => caps.in_process_exec,
            Cap::GrowMemory => caps.grow_memory,
        };
        if !granted {
            return Err(SysError::PermissionDenied(required));
        }
    }

    match call {
        Syscall::Log(message) => Ok(SysValue::Logged(message)),
        Syscall::GetPid => Ok(SysValue::Pid(pid)),
        Syscall::Yield => {
            nucleus.procs.schedule();
            Ok(SysValue::Ok)
        }
        Syscall::Exit(code) => {
            if let Some(frames) = nucleus.frames.as_mut() {
                nucleus.procs.exit(pid, frames);
            }
            Ok(SysValue::Exited(code))
        }
        Syscall::Read { path } => {
            let bytes = nucleus.fs.read_all(&path).map_err(SysError::Fs)?;
            Ok(SysValue::Bytes(bytes))
        }
        Syscall::Write { path, data } => {
            nucleus.fs.write_all(&path, &data).map_err(SysError::Fs)?;
            Ok(SysValue::Wrote(data.len()))
        }
        Syscall::Spawn { name, tier, elf } => {
            let frames = nucleus.frames.as_mut().ok_or(SysError::NoMemoryMap)?;
            let child = nucleus
                .procs
                .spawn(name, tier, &elf, frames)
                .map_err(SysError::Spawn)?;
            Ok(SysValue::Spawned(child))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::synth_executable;
    use crate::mem::MemoryRegion;
    use crate::Nucleus;

    fn nucleus() -> Nucleus {
        let mut n = Nucleus::new(alloc::boxed::Box::new(|| 0), None);
        n.install_memory_map(&[
            MemoryRegion::reserved(0, 0x1000),
            MemoryRegion::usable(0x1000, 64 * 1024 * 1024),
        ]);
        n
    }

    fn spawn(n: &mut Nucleus, tier: Tier) -> Pid {
        let elf = synth_executable(0x40_0000, &[0xC3]);
        let frames = n.frames.as_mut().unwrap();
        n.procs.spawn("p", tier, &elf, frames).unwrap()
    }

    #[test]
    fn proven_process_may_write_and_spawn() {
        let mut n = nucleus();
        let pid = spawn(&mut n, Tier::Proven);
        let wrote = dispatch(
            &mut n,
            pid,
            Syscall::Write {
                path: "/f".into(),
                data: b"hello".to_vec(),
            },
        );
        assert_eq!(wrote, Ok(SysValue::Wrote(5)));
        // And it can be read back through the same boundary.
        let read = dispatch(&mut n, pid, Syscall::Read { path: "/f".into() });
        assert_eq!(read, Ok(SysValue::Bytes(b"hello".to_vec())));

        let child = dispatch(
            &mut n,
            pid,
            Syscall::Spawn {
                name: "child".into(),
                tier: Tier::Unproven,
                elf: synth_executable(0x40_0000, &[0xC3]),
            },
        );
        assert!(matches!(child, Ok(SysValue::Spawned(_))));
    }

    #[test]
    fn unproven_process_is_refused_write_and_spawn_but_may_read() {
        let mut n = nucleus();
        // Seed a file with a proven process first.
        let proven = spawn(&mut n, Tier::Proven);
        dispatch(
            &mut n,
            proven,
            Syscall::Write {
                path: "/secret".into(),
                data: b"data".to_vec(),
            },
        )
        .unwrap();

        let caged = spawn(&mut n, Tier::Unproven);
        // Writing is denied — not because of who it is, but because nothing
        // proved it may.
        assert_eq!(
            dispatch(
                &mut n,
                caged,
                Syscall::Write {
                    path: "/secret".into(),
                    data: b"tampered".to_vec()
                }
            ),
            Err(SysError::PermissionDenied(Cap::FsWrite))
        );
        // Spawning is denied.
        assert_eq!(
            dispatch(
                &mut n,
                caged,
                Syscall::Spawn {
                    name: "x".into(),
                    tier: Tier::Unproven,
                    elf: synth_executable(0x40_0000, &[0xC3])
                }
            ),
            Err(SysError::PermissionDenied(Cap::Spawn))
        );
        // But reading is always allowed, and the file is untouched.
        assert_eq!(
            dispatch(
                &mut n,
                caged,
                Syscall::Read {
                    path: "/secret".into()
                }
            ),
            Ok(SysValue::Bytes(b"data".to_vec()))
        );
    }

    #[test]
    fn always_allowed_calls_need_no_capability() {
        let mut n = nucleus();
        let caged = spawn(&mut n, Tier::Unproven);
        assert_eq!(
            dispatch(&mut n, caged, Syscall::GetPid),
            Ok(SysValue::Pid(caged))
        );
        assert!(matches!(
            dispatch(&mut n, caged, Syscall::Log("hi".into())),
            Ok(SysValue::Logged(_))
        ));
        assert_eq!(dispatch(&mut n, caged, Syscall::Yield), Ok(SysValue::Ok));
    }

    #[test]
    fn unknown_process_is_rejected() {
        let mut n = nucleus();
        assert_eq!(
            dispatch(&mut n, 999, Syscall::GetPid),
            Err(SysError::NoSuchProcess)
        );
    }
}
