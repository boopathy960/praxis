//! PCI bus enumeration — the kernel discovering the real hardware beneath it.
//!
//! Config space is read through the classic 0xCF8/0xCFC I/O mechanism, but the
//! port access itself is abstracted behind a `read_config` closure: the bare-
//! metal boot crate passes the real `in/out` port I/O, while host tests pass a
//! synthetic config space. Same split as [`crate::net::NetDevice`] — the logic
//! is portable and unit-tested, and the hardware lives at the edge.
//!
//! Enumerating the bus is the prerequisite for every real driver: it is how the
//! kernel finds the NIC, the disk controller, and the display — by vendor,
//! device, and class code, exactly as it exists on the metal.

use alloc::vec::Vec;

/// One PCI function found on the bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PciDevice {
    pub bus: u8,
    pub slot: u8,
    pub func: u8,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header_type: u8,
}

impl PciDevice {
    /// A human-readable class, from the PCI class code.
    #[must_use]
    pub fn class_name(&self) -> &'static str {
        match self.class {
            0x00 => "Unclassified",
            0x01 => "Mass storage controller",
            0x02 => "Network controller",
            0x03 => "Display controller",
            0x04 => "Multimedia controller",
            0x05 => "Memory controller",
            0x06 => "Bridge",
            0x07 => "Communication controller",
            0x08 => "Base system peripheral",
            0x09 => "Input device",
            0x0C => "Serial bus controller",
            0x0D => "Wireless controller",
            _ => "Other",
        }
    }

    /// A human-readable vendor, for the handful QEMU commonly presents.
    #[must_use]
    pub fn vendor_name(&self) -> &'static str {
        match self.vendor {
            0x8086 => "Intel",
            0x1234 => "QEMU/Bochs",
            0x1af4 => "Red Hat (virtio)",
            0x1b36 => "Red Hat (QEMU)",
            0x10ec => "Realtek",
            0x1022 => "AMD",
            0x10de => "NVIDIA",
            _ => "unknown",
        }
    }

    /// True for a virtio device (vendor 0x1af4) — the family of paravirtual
    /// NIC / block / console devices Praxis's real drivers will target next.
    #[must_use]
    pub fn is_virtio(&self) -> bool {
        self.vendor == 0x1af4
    }
}

/// Build the 32-bit config address for mechanism #1 (0xCF8/0xCFC). Public so the
/// bare-metal port-I/O layer builds the exact same address the CPU expects.
#[must_use]
pub fn config_address(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC)
}

/// Enumerate bus 0 (where QEMU places its devices), reading each function's
/// vendor/device/class through `read_config(bus, slot, func, offset) -> u32`.
/// A vendor of `0xFFFF` means "no device". Multi-function slots are probed for
/// all eight functions.
pub fn enumerate(read_config: impl Fn(u8, u8, u8, u8) -> u32) -> Vec<PciDevice> {
    let mut devices = Vec::new();
    for slot in 0..32u8 {
        let vendor = (read_config(0, slot, 0, 0x00) & 0xFFFF) as u16;
        if vendor == 0xFFFF {
            continue;
        }
        // Header-type bit 7 marks a multi-function device.
        let header0 = ((read_config(0, slot, 0, 0x0C) >> 16) & 0xFF) as u8;
        let funcs = if header0 & 0x80 != 0 { 8 } else { 1 };
        for func in 0..funcs {
            let id = read_config(0, slot, func, 0x00);
            let vendor = (id & 0xFFFF) as u16;
            if vendor == 0xFFFF {
                continue;
            }
            let device = (id >> 16) as u16;
            let class_reg = read_config(0, slot, func, 0x08);
            let prog_if = ((class_reg >> 8) & 0xFF) as u8;
            let subclass = ((class_reg >> 16) & 0xFF) as u8;
            let class = ((class_reg >> 24) & 0xFF) as u8;
            let header_type = ((read_config(0, slot, func, 0x0C) >> 16) & 0xFF) as u8;
            devices.push(PciDevice {
                bus: 0,
                slot,
                func,
                vendor,
                device,
                class,
                subclass,
                prog_if,
                header_type,
            });
        }
    }
    devices
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic config space resembling QEMU's default: a 440FX host bridge
    /// at slot 0, a Bochs VGA at slot 2, and an Intel e1000 NIC at slot 3.
    fn qemu_like(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
        if bus != 0 || func != 0 {
            return 0xFFFF_FFFF;
        }
        match (slot, offset) {
            (0, 0x00) => 0x1237_8086, // Intel 440FX host bridge
            (0, 0x08) => 0x0600_0000, // class 0x06 (bridge), subclass 0x00
            (0, 0x0C) => 0x0000_0000, // header type 0 (single function)
            (2, 0x00) => 0x1111_1234, // QEMU/Bochs VGA
            (2, 0x08) => 0x0300_0000, // class 0x03 (display)
            (2, 0x0C) => 0x0000_0000,
            (3, 0x00) => 0x100E_8086, // Intel e1000 NIC
            (3, 0x08) => 0x0200_0000, // class 0x02 (network)
            (3, 0x0C) => 0x0000_0000,
            (_, 0x00) => 0xFFFF_FFFF, // empty slot
            _ => 0x0000_0000,
        }
    }

    #[test]
    fn config_address_matches_the_spec() {
        // bus 0, slot 3, func 0, offset 0x08 → enable | slot<<11 | 0x08.
        assert_eq!(
            config_address(0, 3, 0, 0x08),
            0x8000_0000 | (3 << 11) | 0x08
        );
        // Offset is dword-aligned (low two bits cleared).
        assert_eq!(config_address(0, 0, 0, 0x0E) & 0x3, 0);
    }

    #[test]
    fn enumerates_the_qemu_like_bus() {
        let devs = enumerate(qemu_like);
        assert_eq!(devs.len(), 3);

        let nic = devs.iter().find(|d| d.class == 0x02).expect("a NIC");
        assert_eq!(nic.vendor, 0x8086);
        assert_eq!(nic.device, 0x100E);
        assert_eq!(nic.vendor_name(), "Intel");
        assert_eq!(nic.class_name(), "Network controller");
        assert_eq!(nic.slot, 3);

        assert!(devs.iter().any(|d| d.class == 0x06)); // host bridge
        assert!(devs.iter().any(|d| d.class == 0x03)); // display
    }

    #[test]
    fn empty_bus_yields_nothing() {
        let devs = enumerate(|_, _, _, _| 0xFFFF_FFFF);
        assert!(devs.is_empty());
    }

    #[test]
    fn detects_virtio_vendor() {
        let virtio = PciDevice {
            bus: 0,
            slot: 4,
            func: 0,
            vendor: 0x1af4,
            device: 0x1000,
            class: 0x02,
            subclass: 0,
            prog_if: 0,
            header_type: 0,
        };
        assert!(virtio.is_virtio());
        assert_eq!(virtio.vendor_name(), "Red Hat (virtio)");
    }
}
