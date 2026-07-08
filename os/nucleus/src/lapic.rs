//! The Local APIC — sending the inter-processor interrupts that start cores.
//!
//! ACPI ([`crate::acpi`]) *finds* the application processors; the Local APIC is
//! how the boot core *starts* them. Multi-core bring-up is the INIT-SIPI-SIPI
//! sequence: an INIT IPI resets the target core, then a STARTUP IPI (SIPI)
//! points it at a real-mode trampoline page (`vector` → physical `vector<<12`).
//! This module encodes those IPIs into the LAPIC's Interrupt Command Register
//! exactly as the Intel SDM specifies.
//!
//! The register access goes through the [`LapicMmio`] trait so the command
//! encoding — the part that is easy to get subtly wrong — is unit-tested on the
//! host; the bare-metal side supplies the real memory-mapped LAPIC over the
//! bootloader's physical-memory window.

/// LAPIC register offsets (bytes from the MMIO base).
pub const REG_ID: u32 = 0x020;
pub const REG_EOI: u32 = 0x0B0;
pub const REG_SVR: u32 = 0x0F0; // Spurious Interrupt Vector
pub const REG_ICR_LOW: u32 = 0x300;
pub const REG_ICR_HIGH: u32 = 0x310;

/// SVR bit 8 enables the APIC.
const SVR_ENABLE: u32 = 1 << 8;

// ICR (Interrupt Command Register) fields.
const DELIVERY_INIT: u32 = 0x5 << 8;
const DELIVERY_STARTUP: u32 = 0x6 << 8;
const LEVEL_ASSERT: u32 = 1 << 14;
/// Delivery-status bit — set by hardware while an IPI is in flight.
const DELIVERY_PENDING: u32 = 1 << 12;

/// The ICR-low value for an INIT IPI (assert, edge-triggered): `0x0000_4500`.
#[must_use]
pub fn init_ipi() -> u32 {
    DELIVERY_INIT | LEVEL_ASSERT
}

/// The ICR-low value for a STARTUP IPI to `vector` (the trampoline page):
/// `0x0000_4600 | vector`.
#[must_use]
pub fn startup_ipi(vector: u8) -> u32 {
    DELIVERY_STARTUP | LEVEL_ASSERT | u32::from(vector)
}

/// Memory-mapped LAPIC access. Bare metal implements this over the LAPIC's
/// physical base; tests implement it over a register array.
pub trait LapicMmio {
    fn read(&self, reg: u32) -> u32;
    fn write(&mut self, reg: u32, value: u32);
}

/// The Local APIC of the current core.
pub struct Lapic<M: LapicMmio> {
    mmio: M,
}

impl<M: LapicMmio> Lapic<M> {
    pub fn new(mmio: M) -> Self {
        Self { mmio }
    }

    /// This core's APIC id (the boot processor's, when called on the BSP).
    #[must_use]
    pub fn id(&self) -> u32 {
        self.mmio.read(REG_ID) >> 24
    }

    /// Enable the APIC via the spurious-vector register (needed before it will
    /// send IPIs).
    pub fn enable(&mut self) {
        let svr = self.mmio.read(REG_SVR);
        self.mmio.write(REG_SVR, svr | SVR_ENABLE | 0xFF);
    }

    /// Busy-wait until any in-flight IPI has been delivered.
    fn wait_idle(&self) {
        let mut spins = 0;
        while self.mmio.read(REG_ICR_LOW) & DELIVERY_PENDING != 0 {
            spins += 1;
            if spins > 1_000_000 {
                break;
            }
        }
    }

    /// Send an INIT IPI to `apic_id` — resets the target core.
    pub fn send_init(&mut self, apic_id: u8) {
        self.mmio.write(REG_ICR_HIGH, u32::from(apic_id) << 24);
        self.mmio.write(REG_ICR_LOW, init_ipi());
        self.wait_idle();
    }

    /// Send a STARTUP IPI: the target core begins executing at physical
    /// address `vector << 12` (the trampoline page).
    pub fn send_startup(&mut self, apic_id: u8, vector: u8) {
        self.mmio.write(REG_ICR_HIGH, u32::from(apic_id) << 24);
        self.mmio.write(REG_ICR_LOW, startup_ipi(vector));
        self.wait_idle();
    }

    /// The full INIT-SIPI-SIPI bring-up handshake for one core (two SIPIs, as
    /// the SDM recommends). `vector` is the trampoline page number.
    pub fn start_core(&mut self, apic_id: u8, vector: u8) {
        self.send_init(apic_id);
        self.send_startup(apic_id, vector);
        self.send_startup(apic_id, vector);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// A recording LAPIC: a small register file plus a log of every write, so
    /// the exact IPI command sequence can be asserted.
    #[derive(Default)]
    struct MockLapic {
        regs: alloc::collections::BTreeMap<u32, u32>,
        writes: Vec<(u32, u32)>,
    }
    impl LapicMmio for MockLapic {
        fn read(&self, reg: u32) -> u32 {
            *self.regs.get(&reg).unwrap_or(&0)
        }
        fn write(&mut self, reg: u32, value: u32) {
            self.regs.insert(reg, value);
            self.writes.push((reg, value));
        }
    }

    #[test]
    fn ipi_command_words_match_the_sdm() {
        // INIT assert = 0x4500; STARTUP to page 8 = 0x4608.
        assert_eq!(init_ipi(), 0x0000_4500);
        assert_eq!(startup_ipi(0x08), 0x0000_4608);
        assert_eq!(startup_ipi(0x00), 0x0000_4600);
    }

    #[test]
    fn enable_sets_the_apic_bit() {
        let mut lapic = Lapic::new(MockLapic::default());
        lapic.enable();
        assert_ne!(lapic.mmio.read(REG_SVR) & SVR_ENABLE, 0);
    }

    #[test]
    fn start_core_emits_init_then_two_sipis_to_the_right_target() {
        let mut lapic = Lapic::new(MockLapic::default());
        lapic.start_core(3, 0x08);
        // Filter to the ICR writes in order: (high=target, low=command) × 3.
        let icr: Vec<(u32, u32)> = lapic
            .mmio
            .writes
            .iter()
            .copied()
            .filter(|(r, _)| *r == REG_ICR_LOW || *r == REG_ICR_HIGH)
            .collect();
        assert_eq!(
            icr,
            alloc::vec![
                (REG_ICR_HIGH, 3 << 24),
                (REG_ICR_LOW, 0x4500), // INIT
                (REG_ICR_HIGH, 3 << 24),
                (REG_ICR_LOW, 0x4608), // SIPI #1 → page 8
                (REG_ICR_HIGH, 3 << 24),
                (REG_ICR_LOW, 0x4608), // SIPI #2
            ]
        );
    }
}
