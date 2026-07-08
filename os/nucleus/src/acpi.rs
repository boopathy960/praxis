//! ACPI — discovering the machine's CPU cores (the first step of SMP).
//!
//! Praxis runs on one core, but a real OS uses every core the board has. Before
//! any of them can be started, the kernel has to *find* them, and the firmware
//! describes them in the ACPI **MADT** (Multiple APIC Description Table): one
//! "Local APIC" entry per logical processor, each with an APIC id and an
//! enabled flag. This module walks the ACPI tables from the RSDP the bootloader
//! hands us down to the MADT and enumerates those entries — so `cpus` reports
//! the true core count, and a later bring-up step (INIT-SIPI-SIPI to each
//! non-boot APIC id) has the list it needs.
//!
//! As with the NIC and disk drivers, the walk is portable and hardware touches
//! go through a [`PhysMem`] trait (read N bytes at a physical address), so the
//! whole parser is unit-tested on the host against a synthetic firmware image;
//! the bare-metal side supplies the thin real reader over the bootloader's
//! physical-memory mapping.

use alloc::vec::Vec;

/// Read physical memory. Bare metal implements this over the bootloader's
/// physical-memory window; tests implement it over an in-memory table image.
pub trait PhysMem {
    /// Read `len` bytes at physical address `addr`, or `None` if unavailable.
    fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiError {
    BadRsdp,
    NoRootTable,
    NoMadt,
    Malformed,
}

/// One logical CPU the firmware described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCore {
    pub processor_id: u8,
    pub apic_id: u8,
    /// `true` if the firmware marked it usable (MADT flags bit 0).
    pub enabled: bool,
}

/// What the ACPI walk found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpiInfo {
    /// Physical address of the Local APIC MMIO block.
    pub local_apic_addr: u64,
    /// Every logical processor the MADT listed, in table order.
    pub cpus: Vec<CpuCore>,
}

impl AcpiInfo {
    /// How many cores are usable (enabled) — the SMP width.
    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.cpus.iter().filter(|c| c.enabled).count()
    }

    /// The APIC ids of the *non-boot* cores — the ones a bring-up step would
    /// send INIT-SIPI-SIPI to. The boot core is assumed to be the first
    /// enabled entry.
    #[must_use]
    pub fn application_processor_ids(&self) -> Vec<u8> {
        let mut enabled = self.cpus.iter().filter(|c| c.enabled);
        let _boot = enabled.next(); // the BSP
        enabled.map(|c| c.apic_id).collect()
    }
}

const MADT_SIG: &[u8; 4] = b"APIC";
const SDT_HEADER_LEN: usize = 36;

/// Walk RSDP → RSDT/XSDT → MADT starting at the `rsdp_addr` the bootloader
/// reported, and enumerate the CPU cores.
pub fn discover(rsdp_addr: u64, mem: &impl PhysMem) -> Result<AcpiInfo, AcpiError> {
    // ── RSDP ── "RSD PTR " signature; revision picks 32-bit RSDT vs 64-bit XSDT.
    let rsdp = mem.read(rsdp_addr, 36).ok_or(AcpiError::BadRsdp)?;
    if rsdp.len() < 20 || &rsdp[0..8] != b"RSD PTR " {
        return Err(AcpiError::BadRsdp);
    }
    let revision = rsdp[15];
    let (root_addr, ptr_size) = if revision >= 2 && rsdp.len() >= 34 {
        (u64_le(&rsdp[24..32]), 8) // XSDT, 64-bit pointers
    } else {
        (u32_le(&rsdp[16..20]) as u64, 4) // RSDT, 32-bit pointers
    };

    // ── root table ── a standard SDT header then an array of table pointers.
    let root_header = mem
        .read(root_addr, SDT_HEADER_LEN)
        .ok_or(AcpiError::NoRootTable)?;
    if root_header.len() < SDT_HEADER_LEN {
        return Err(AcpiError::Malformed);
    }
    let root_len = u32_le(&root_header[4..8]) as usize;
    if root_len < SDT_HEADER_LEN {
        return Err(AcpiError::Malformed);
    }
    let entries_bytes = root_len - SDT_HEADER_LEN;
    let root = mem.read(root_addr, root_len).ok_or(AcpiError::Malformed)?;

    // ── scan the pointers for the MADT ──
    let mut madt_addr = None;
    let count = entries_bytes / ptr_size;
    for i in 0..count {
        let off = SDT_HEADER_LEN + i * ptr_size;
        let table_addr = if ptr_size == 8 {
            u64_le(&root[off..off + 8])
        } else {
            u32_le(&root[off..off + 4]) as u64
        };
        if let Some(sig) = mem.read(table_addr, 4) {
            if sig.as_slice() == MADT_SIG {
                madt_addr = Some(table_addr);
                break;
            }
        }
    }
    let madt_addr = madt_addr.ok_or(AcpiError::NoMadt)?;
    parse_madt(madt_addr, mem)
}

/// Parse a MADT at `addr`: header + 32-bit local-APIC address + flags, then a
/// variable list of entries; type 0 is a Local APIC (one per logical CPU).
fn parse_madt(addr: u64, mem: &impl PhysMem) -> Result<AcpiInfo, AcpiError> {
    let header = mem
        .read(addr, SDT_HEADER_LEN + 8)
        .ok_or(AcpiError::NoMadt)?;
    if header.len() < SDT_HEADER_LEN + 8 || &header[0..4] != MADT_SIG {
        return Err(AcpiError::Malformed);
    }
    let total_len = u32_le(&header[4..8]) as usize;
    if total_len < SDT_HEADER_LEN + 8 {
        return Err(AcpiError::Malformed);
    }
    let local_apic_addr = u32_le(&header[SDT_HEADER_LEN..SDT_HEADER_LEN + 4]) as u64;
    let body = mem.read(addr, total_len).ok_or(AcpiError::Malformed)?;

    let mut cpus = Vec::new();
    let mut at = SDT_HEADER_LEN + 8; // past header + lapic addr + flags
    while at + 2 <= body.len() {
        let entry_type = body[at];
        let entry_len = body[at + 1] as usize;
        if entry_len < 2 || at + entry_len > body.len() {
            break; // malformed / truncated entry list — stop cleanly
        }
        // Type 0: Processor Local APIC = proc_id(1), apic_id(1), flags(4).
        if entry_type == 0 && entry_len >= 8 {
            let processor_id = body[at + 2];
            let apic_id = body[at + 3];
            let flags = u32_le(&body[at + 4..at + 8]);
            cpus.push(CpuCore {
                processor_id,
                apic_id,
                enabled: flags & 1 != 0,
            });
        }
        at += entry_len;
    }
    if cpus.is_empty() {
        return Err(AcpiError::NoMadt);
    }
    Ok(AcpiInfo {
        local_apic_addr,
        cpus,
    })
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}
fn u64_le(b: &[u8]) -> u64 {
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    /// An in-memory firmware image: physical address → bytes.
    #[derive(Default)]
    struct Firmware {
        regions: BTreeMap<u64, Vec<u8>>,
    }
    impl Firmware {
        fn place(&mut self, addr: u64, bytes: Vec<u8>) {
            self.regions.insert(addr, bytes);
        }
    }
    impl PhysMem for Firmware {
        fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
            // Find the region that contains [addr, addr+len).
            for (&base, bytes) in &self.regions {
                if addr >= base && addr + len as u64 <= base + bytes.len() as u64 {
                    let off = (addr - base) as usize;
                    return Some(bytes[off..off + len].to_vec());
                }
            }
            None
        }
    }

    /// A standard SDT header with the given signature and total length.
    fn sdt_header(sig: &[u8; 4], total_len: u32) -> Vec<u8> {
        let mut h = Vec::new();
        h.extend_from_slice(sig);
        h.extend_from_slice(&total_len.to_le_bytes());
        h.push(1); // revision
        h.push(0); // checksum (not verified here)
        h.extend_from_slice(b"PRAXIS"); // OEM id
        h.extend_from_slice(b"PRAXISOS"); // OEM table id
        h.extend_from_slice(&1u32.to_le_bytes()); // OEM revision
        h.extend_from_slice(b"PRX "); // creator id
        h.extend_from_slice(&1u32.to_le_bytes()); // creator revision
        assert_eq!(h.len(), SDT_HEADER_LEN);
        h
    }

    fn local_apic_entry(proc_id: u8, apic_id: u8, enabled: bool) -> Vec<u8> {
        alloc::vec![
            0, // type: Local APIC
            8, // length
            proc_id,
            apic_id,
            if enabled { 1 } else { 0 },
            0,
            0,
            0,
        ]
    }

    /// Build a firmware image with `n` cores (all but the last enabled) and
    /// return (firmware, rsdp_addr).
    fn firmware_with(cores: &[(u8, u8, bool)]) -> (Firmware, u64) {
        let mut fw = Firmware::default();
        let madt_addr = 0x10_0000u64;
        let xsdt_addr = 0x20_0000u64;
        let rsdp_addr = 0x30_0000u64;

        // MADT: header + lapic addr + flags + one entry per core.
        let mut entries = Vec::new();
        for &(pid, aid, en) in cores {
            entries.extend_from_slice(&local_apic_entry(pid, aid, en));
        }
        let madt_len = (SDT_HEADER_LEN + 8 + entries.len()) as u32;
        let mut madt = sdt_header(b"APIC", madt_len);
        madt.extend_from_slice(&0xFEE0_0000u32.to_le_bytes()); // local APIC addr
        madt.extend_from_slice(&1u32.to_le_bytes()); // flags (PC-AT compat)
        madt.extend_from_slice(&entries);
        fw.place(madt_addr, madt);

        // XSDT: header + one 64-bit pointer to the MADT.
        let xsdt_len = (SDT_HEADER_LEN + 8) as u32;
        let mut xsdt = sdt_header(b"XSDT", xsdt_len);
        xsdt.extend_from_slice(&madt_addr.to_le_bytes());
        fw.place(xsdt_addr, xsdt);

        // RSDP v2: signature, ..., revision 2, xsdt address.
        let mut rsdp = Vec::new();
        rsdp.extend_from_slice(b"RSD PTR ");
        rsdp.push(0); // checksum
        rsdp.extend_from_slice(b"PRAXIS"); // OEM id
        rsdp.push(2); // revision 2 → use XSDT
        rsdp.extend_from_slice(&0u32.to_le_bytes()); // RSDT addr (unused)
        rsdp.extend_from_slice(&36u32.to_le_bytes()); // length
        rsdp.extend_from_slice(&xsdt_addr.to_le_bytes()); // XSDT addr
        rsdp.extend_from_slice(&[0, 0, 0, 0]); // ext checksum + reserved
        fw.place(rsdp_addr, rsdp);

        (fw, rsdp_addr)
    }

    #[test]
    fn discovers_all_cores_from_a_synthetic_madt() {
        let (fw, rsdp) = firmware_with(&[(0, 0, true), (1, 1, true), (2, 2, true), (3, 3, true)]);
        let info = discover(rsdp, &fw).unwrap();
        assert_eq!(info.cpus.len(), 4);
        assert_eq!(info.enabled_count(), 4);
        assert_eq!(info.local_apic_addr, 0xFEE0_0000);
        // The BSP is core 0; the other three are the APs to start.
        assert_eq!(info.application_processor_ids(), alloc::vec![1, 2, 3]);
    }

    #[test]
    fn a_disabled_core_is_counted_but_not_an_ap_to_start() {
        let (fw, rsdp) = firmware_with(&[(0, 0, true), (1, 1, true), (2, 2, false)]);
        let info = discover(rsdp, &fw).unwrap();
        assert_eq!(info.cpus.len(), 3, "all listed");
        assert_eq!(info.enabled_count(), 2, "one disabled");
        assert_eq!(info.application_processor_ids(), alloc::vec![1]);
    }

    #[test]
    fn a_single_core_machine_has_no_aps() {
        let (fw, rsdp) = firmware_with(&[(0, 0, true)]);
        let info = discover(rsdp, &fw).unwrap();
        assert_eq!(info.enabled_count(), 1);
        assert!(info.application_processor_ids().is_empty());
    }

    #[test]
    fn a_bad_rsdp_is_rejected_cleanly() {
        let mut fw = Firmware::default();
        fw.place(0x1000, alloc::vec![0u8; 36]); // no "RSD PTR "
        assert_eq!(discover(0x1000, &fw), Err(AcpiError::BadRsdp));
        assert_eq!(discover(0x9999, &fw), Err(AcpiError::BadRsdp)); // unreadable
    }
}
