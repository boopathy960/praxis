//! Bare-metal wiring for the e1000 driver: the real bus under the portable
//! core in `nucleus/src/e1000.rs`.
//!
//! Everything hardware-specific is here and it is tiny — BAR0 register access
//! through the bootloader's physical-memory mapping, a DMA arena carved out
//! of the physical frame allocator, and the PCI command-register write that
//! grants the chip bus mastering. The driver logic itself never changes
//! between this and the host-side test model.

use praxis_nucleus::e1000::{E1000Bus, ARENA_BYTES, ARENA_FRAMES, E1000};
use praxis_nucleus::net::{NetDevice, NetStack};
use praxis_nucleus::Nucleus;

/// The real chip: BAR0 MMIO registers and guest RAM, both reached through the
/// bootloader's complete physical-memory mapping.
struct MmioBus {
    regs: u64, // virtual address of BAR0
    phys_offset: u64,
}

impl E1000Bus for MmioBus {
    fn read32(&mut self, reg: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.regs + u64::from(reg)) as *const u32) }
    }
    fn write32(&mut self, reg: u32, value: u32) {
        unsafe { core::ptr::write_volatile((self.regs + u64::from(reg)) as *mut u32, value) }
    }
    fn mem(&mut self, phys: u64, len: usize) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut((self.phys_offset + phys) as *mut u8, len) }
    }
}

/// Find the e1000 on the enumerated bus, bring it up, and install a NetStack
/// over it as the kernel's interface (QEMU slirp addressing: we are 10.0.2.15,
/// the gateway is 10.0.2.2). Returns the hardware MAC.
pub fn init(
    nucleus: &mut Nucleus,
    phys_offset: u64,
    read_config: fn(u8, u8, u8, u8) -> u32,
    write_config: fn(u8, u8, u8, u8, u32),
) -> Result<[u8; 6], &'static str> {
    let dev = nucleus
        .pci
        .iter()
        .find(|d| d.vendor == 0x8086 && d.device == 0x100E)
        .ok_or("no e1000 on the PCI bus")?;
    let (bus, slot, func) = (dev.bus, dev.slot, dev.func);

    // Memory-space decoding + bus mastering: without the latter the chip's
    // DMA engine is not allowed to touch the rings at all.
    let cmd = read_config(bus, slot, func, 0x04);
    write_config(bus, slot, func, 0x04, cmd | 0x0006);
    let bar0 = u64::from(read_config(bus, slot, func, 0x10) & 0xFFFF_FFF0);

    // The DMA arena: physically contiguous, zeroed, owned by the driver.
    let frames = nucleus.frames.as_mut().ok_or("frame allocator offline")?;
    let arena = frames
        .allocate_contiguous(ARENA_FRAMES)
        .ok_or("no contiguous frames for the DMA arena")?
        .start_addr();
    unsafe {
        core::ptr::write_bytes((phys_offset + arena) as *mut u8, 0, ARENA_BYTES as usize);
    }

    let mmio = MmioBus {
        regs: phys_offset + bar0,
        phys_offset,
    };
    let nic = E1000::new(mmio, arena)?;
    let mac = nic.mac();
    let mut stack = NetStack::new([10, 0, 2, 15], alloc::boxed::Box::new(nic));
    // The route off-subnet: everything not on 10.0.2.0/24 is L2-addressed to
    // the slirp gateway — this is what lets the kernel reach the internet.
    stack.gateway = Some([10, 0, 2, 2]);
    nucleus.net = stack;
    Ok(mac)
}
