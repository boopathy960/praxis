//! A from-scratch ELF64 loader — parsing the format real programs ship in.
//!
//! An operating system that can only run code compiled into the kernel is a
//! demo. This parses the ELF64 container (the same format `cargo` emits for
//! `x86_64-unknown-none`), validates it, and extracts the loadable segments —
//! each a `(vaddr, file bytes, mem size, permissions)` the process layer maps
//! into a fresh address space. Combined with [paging](crate::vmem), that is
//! the path from "a byte buffer on disk" to "an isolated running program".
//!
//! Scope: 64-bit little-endian x86_64 executables, `PT_LOAD` segments only
//! (no dynamic linking / relocations — a static blob with an entry point).

use alloc::vec::Vec;

use crate::vmem::PageFlags;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    TooShort,
    BadMagic,
    Not64Bit,
    NotLittleEndian,
    NotExecutable,
    NotX86_64,
    BadProgramHeaders,
    SegmentOutOfFile,
}

/// One loadable segment, resolved against the file.
#[derive(Debug, Clone)]
pub struct Segment {
    /// Virtual address the segment must live at.
    pub vaddr: u64,
    /// Bytes to copy from the file (may be shorter than `mem_size`; the tail
    /// is zero-filled — that is how `.bss` works).
    pub bytes: Vec<u8>,
    /// Total in-memory size (>= `bytes.len()`).
    pub mem_size: u64,
    pub flags: PageFlags,
}

/// A parsed, ready-to-map ELF image.
#[derive(Debug, Clone)]
pub struct ElfImage {
    pub entry: u64,
    pub segments: Vec<Segment>,
}

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

fn u16_at(buf: &[u8], off: usize) -> Option<u16> {
    buf.get(off..off + 2)
        .map(|b| u16::from_le_bytes(b.try_into().unwrap()))
}
fn u32_at(buf: &[u8], off: usize) -> Option<u32> {
    buf.get(off..off + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
}
fn u64_at(buf: &[u8], off: usize) -> Option<u64> {
    buf.get(off..off + 8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
}

/// Parse and validate an ELF64 executable, returning its loadable image.
pub fn parse(buf: &[u8]) -> Result<ElfImage, ElfError> {
    if buf.len() < 64 {
        return Err(ElfError::TooShort);
    }
    if &buf[0..4] != b"\x7FELF" {
        return Err(ElfError::BadMagic);
    }
    if buf[4] != 2 {
        return Err(ElfError::Not64Bit); // EI_CLASS = ELFCLASS64
    }
    if buf[5] != 1 {
        return Err(ElfError::NotLittleEndian); // EI_DATA = ELFDATA2LSB
    }
    let e_type = u16_at(buf, 16).ok_or(ElfError::TooShort)?;
    // ET_EXEC (2) or ET_DYN (3, position-independent executable).
    if e_type != 2 && e_type != 3 {
        return Err(ElfError::NotExecutable);
    }
    if u16_at(buf, 18) != Some(0x3E) {
        return Err(ElfError::NotX86_64); // EM_X86_64
    }

    let entry = u64_at(buf, 24).ok_or(ElfError::TooShort)?;
    let phoff = u64_at(buf, 32).ok_or(ElfError::TooShort)? as usize;
    let phentsize = u16_at(buf, 54).ok_or(ElfError::TooShort)? as usize;
    let phnum = u16_at(buf, 56).ok_or(ElfError::TooShort)? as usize;
    if phentsize < 56 {
        return Err(ElfError::BadProgramHeaders);
    }

    let mut segments = Vec::new();
    for i in 0..phnum {
        let base = phoff + i * phentsize;
        let p_type = u32_at(buf, base).ok_or(ElfError::BadProgramHeaders)?;
        if p_type != PT_LOAD {
            continue;
        }
        let p_flags = u32_at(buf, base + 4).ok_or(ElfError::BadProgramHeaders)?;
        let p_offset = u64_at(buf, base + 8).ok_or(ElfError::BadProgramHeaders)? as usize;
        let p_vaddr = u64_at(buf, base + 16).ok_or(ElfError::BadProgramHeaders)?;
        let p_filesz = u64_at(buf, base + 32).ok_or(ElfError::BadProgramHeaders)? as usize;
        let p_memsz = u64_at(buf, base + 40).ok_or(ElfError::BadProgramHeaders)?;

        let bytes = buf
            .get(p_offset..p_offset + p_filesz)
            .ok_or(ElfError::SegmentOutOfFile)?
            .to_vec();

        segments.push(Segment {
            vaddr: p_vaddr,
            bytes,
            mem_size: p_memsz,
            flags: PageFlags {
                writable: p_flags & PF_W != 0,
                user: true,
                executable: p_flags & PF_X != 0,
            },
        });
    }

    if segments.is_empty() {
        return Err(ElfError::BadProgramHeaders);
    }
    Ok(ElfImage { entry, segments })
}

/// Build a minimal valid ELF64 executable in memory — one `PT_LOAD` segment of
/// executable code at `vaddr`, entry at its start. Used by the shell's `exec`
/// demo (no on-disk toolchain in the kernel) and by tests.
pub fn synth_executable(vaddr: u64, code: &[u8]) -> Vec<u8> {
    const EHSIZE: usize = 64;
    const PHENTSIZE: usize = 56;
    let mut buf = Vec::new();
    // ── ELF header ──
    buf.extend_from_slice(b"\x7FELF");
    buf.push(2); // 64-bit
    buf.push(1); // little-endian
    buf.push(1); // version
    buf.resize(16, 0); // pad EI_PAD
    buf.extend_from_slice(&2u16.to_le_bytes()); // e_type = ET_EXEC
    buf.extend_from_slice(&0x3Eu16.to_le_bytes()); // e_machine = x86_64
    buf.extend_from_slice(&1u32.to_le_bytes()); // e_version
    buf.extend_from_slice(&vaddr.to_le_bytes()); // e_entry
    buf.extend_from_slice(&(EHSIZE as u64).to_le_bytes()); // e_phoff
    buf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff
    buf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    buf.extend_from_slice(&(EHSIZE as u16).to_le_bytes()); // e_ehsize
    buf.extend_from_slice(&(PHENTSIZE as u16).to_le_bytes()); // e_phentsize
    buf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
    buf.extend_from_slice(&0u16.to_le_bytes()); // e_shentsize
    buf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    buf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx
    debug_assert_eq!(buf.len(), EHSIZE);

    // ── one program header (PT_LOAD, R+X) ──
    let p_offset = (EHSIZE + PHENTSIZE) as u64;
    buf.extend_from_slice(&PT_LOAD.to_le_bytes()); // p_type
    buf.extend_from_slice(&(PF_X | 4).to_le_bytes()); // p_flags = R+X
    buf.extend_from_slice(&p_offset.to_le_bytes()); // p_offset
    buf.extend_from_slice(&vaddr.to_le_bytes()); // p_vaddr
    buf.extend_from_slice(&vaddr.to_le_bytes()); // p_paddr
    buf.extend_from_slice(&(code.len() as u64).to_le_bytes()); // p_filesz
    buf.extend_from_slice(&(code.len() as u64).to_le_bytes()); // p_memsz
    buf.extend_from_slice(&PAGE_ALIGN.to_le_bytes()); // p_align
    debug_assert_eq!(buf.len(), EHSIZE + PHENTSIZE);

    buf.extend_from_slice(code);
    buf
}

const PAGE_ALIGN: u64 = 0x1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_synthetic_executable() {
        let code = [0x90, 0x90, 0xC3]; // nop; nop; ret
        let elf = synth_executable(0x40_0000, &code);
        let image = parse(&elf).unwrap();
        assert_eq!(image.entry, 0x40_0000);
        assert_eq!(image.segments.len(), 1);
        let seg = &image.segments[0];
        assert_eq!(seg.vaddr, 0x40_0000);
        assert_eq!(seg.bytes, code);
        assert!(seg.flags.executable && !seg.flags.writable);
    }

    #[test]
    fn bss_segment_zero_fills_the_tail() {
        // memsz > filesz: the extra is .bss, zero-filled at load.
        let code = [1, 2, 3, 4];
        let mut elf = synth_executable(0x1000, &code);
        // Bump p_memsz to filesz + 4096 by rewriting the field (offset 64+40).
        let memsz_off = 64 + 40;
        elf[memsz_off..memsz_off + 8].copy_from_slice(&((code.len() as u64) + 4096).to_le_bytes());
        let image = parse(&elf).unwrap();
        let seg = &image.segments[0];
        assert_eq!(seg.bytes.len(), 4);
        assert_eq!(seg.mem_size, 4 + 4096);
    }

    #[test]
    fn rejects_garbage_and_wrong_arch() {
        assert_eq!(parse(&[0u8; 8]).unwrap_err(), ElfError::TooShort);
        assert_eq!(parse(&[0u8; 128]).unwrap_err(), ElfError::BadMagic);

        let mut elf = synth_executable(0x1000, &[0xC3]);
        elf[18] = 0x28; // e_machine = ARM
        assert_eq!(parse(&elf).unwrap_err(), ElfError::NotX86_64);

        let mut elf = synth_executable(0x1000, &[0xC3]);
        elf[4] = 1; // 32-bit
        assert_eq!(parse(&elf).unwrap_err(), ElfError::Not64Bit);
    }
}
