//! The Praxis system-call ABI — the contract between ring 3 and the kernel.
//!
//! One place defines the syscall numbers and the register convention, shared by
//! both sides: the kernel's `syscall` dispatcher (`boot/src/gdt.rs` →
//! `usermode::handle_syscall`) and any user program built against Praxis. A
//! program written to this ABI is portable across every Praxis build; a kernel
//! that speaks it can run any such program.
//!
//! ## Register convention
//!
//! `rax` = syscall number; arguments in `rsi`, `rdx`, `r10`, `r8` (the SysV
//! order minus `rdi`/`rcx`, which the `syscall` instruction and the dispatch
//! stub reserve); the result comes back in `rax`. This mirrors what the
//! `syscall_entry` stub actually does (`rax → rdi` as the dispatcher's first
//! argument, the user's `rsi`/`rdx` flowing straight through as args 2 and 3).
//!
//! ## The calls
//!
//! | # | name  | args               | returns              |
//! |---|-------|--------------------|----------------------|
//! | 0 | exit  | —                  | never                |
//! | 1 | write | ptr, len           | bytes written        |
//! | 2 | yield | —                  | 0 (cooperative slice)|
//! | 3 | getpid| —                  | the caller's pid     |

/// A decoded system call. The kernel turns the raw `rax` number into this so
/// dispatch is a total, explicit match rather than scattered magic constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syscall {
    Exit,
    Write,
    Yield,
    GetPid,
    /// An unrecognised number — the kernel answers [`ENOSYS`].
    Unknown(u64),
}

/// syscall numbers (the `rax` values).
pub const SYS_EXIT: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_YIELD: u64 = 2;
pub const SYS_GETPID: u64 = 3;

/// The error a call returns in `rax` when the number is not implemented.
pub const ENOSYS: u64 = u64::MAX;

impl Syscall {
    /// Decode a raw syscall number.
    #[must_use]
    pub fn from_number(n: u64) -> Self {
        match n {
            SYS_EXIT => Syscall::Exit,
            SYS_WRITE => Syscall::Write,
            SYS_YIELD => Syscall::Yield,
            SYS_GETPID => Syscall::GetPid,
            other => Syscall::Unknown(other),
        }
    }

    /// The number this call is invoked with.
    #[must_use]
    pub fn number(self) -> u64 {
        match self {
            Syscall::Exit => SYS_EXIT,
            Syscall::Write => SYS_WRITE,
            Syscall::Yield => SYS_YIELD,
            Syscall::GetPid => SYS_GETPID,
            Syscall::Unknown(n) => n,
        }
    }

    /// A stable name for logs and the shell.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Syscall::Exit => "exit",
            Syscall::Write => "write",
            Syscall::Yield => "yield",
            Syscall::GetPid => "getpid",
            Syscall::Unknown(_) => "unknown",
        }
    }
}

// ── userland wrappers ────────────────────────────────────────────────────
//
// These are what a user program compiled *into* Praxis would call — the libc
// layer, in miniature. They emit the `syscall` instruction with the ABI's
// register convention. They live behind `target_arch = "x86_64"` +
// `target_os = "none"` because they only make sense inside a ring-3 process
// (executing `syscall` in the host test binary would trap), yet the numbers
// and [`Syscall`] decoding above are validated on the host.

/// Userland `write(ptr, len)`: ask the kernel to emit `len` bytes at `ptr` to
/// the console. Returns the number of bytes written.
///
/// # Safety
/// Only callable from a Praxis ring-3 process; `ptr`/`len` must describe a
/// valid, mapped buffer in the caller's address space.
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub unsafe fn sys_write(ptr: *const u8, len: usize) -> u64 {
    let ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WRITE,
            in("rsi") ptr,
            in("rdx") len,
            lateout("rax") ret,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

/// Userland `exit()`: never returns.
///
/// # Safety
/// Only callable from a Praxis ring-3 process.
#[cfg(all(target_arch = "x86_64", target_os = "none"))]
pub unsafe fn sys_exit() -> ! {
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_EXIT,
            options(nostack, noreturn),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_round_trip_through_the_decoder() {
        for call in [
            Syscall::Exit,
            Syscall::Write,
            Syscall::Yield,
            Syscall::GetPid,
        ] {
            assert_eq!(Syscall::from_number(call.number()), call);
            assert!(!call.name().is_empty());
        }
    }

    #[test]
    fn unknown_numbers_decode_and_preserve() {
        assert_eq!(Syscall::from_number(999), Syscall::Unknown(999));
        assert_eq!(Syscall::Unknown(999).number(), 999);
        assert_eq!(Syscall::from_number(999).name(), "unknown");
    }
}
