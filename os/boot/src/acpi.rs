//! Bare-metal ACPI wiring: read physical memory through the bootloader's
//! physical-memory window and run the portable MADT walk in
//! `nucleus::acpi` to enumerate the machine's CPU cores.

use alloc::vec::Vec;

use praxis_nucleus::acpi::{self, AcpiInfo, PhysMem};

/// Reads physical memory via the bootloader's complete physical-memory mapping.
struct MappedPhys {
    offset: u64,
}

impl PhysMem for MappedPhys {
    fn read(&self, addr: u64, len: usize) -> Option<Vec<u8>> {
        // The whole of physical memory is mapped at `offset`; copy the bytes
        // out of that window. Bounded by `len`, which the parser keeps small.
        let src = (self.offset + addr) as *const u8;
        let mut out = alloc::vec![0u8; len];
        unsafe { core::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), len) };
        Some(out)
    }
}

/// Discover the CPU cores the firmware describes, given the RSDP address the
/// bootloader found and the physical-memory offset.
pub fn discover_cpus(rsdp_addr: u64, phys_offset: u64) -> Result<AcpiInfo, acpi::AcpiError> {
    acpi::discover(
        rsdp_addr,
        &MappedPhys {
            offset: phys_offset,
        },
    )
}
