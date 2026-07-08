//! Praxis OS image builder — wrap the freestanding kernel ELF in a bootable disk.
//!
//! Usage:
//!   1. build the kernel:  cargo build -p praxis-boot --target x86_64-unknown-none --release
//!   2. build the image:   cargo run -p praxis-image
//!
//! Produces `os/dist/praxis-os-bios.img` (raw BIOS) and `os/dist/praxis-os-uefi.img`
//! (GPT/UEFI). Boot either under QEMU:
//!   qemu-system-x86_64 -drive format=raw,file=os/dist/praxis-os-bios.img -serial stdio

use std::path::PathBuf;
use std::process::exit;

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let os_root = manifest.parent().unwrap_or(&manifest).to_path_buf();

    // Kernel ELF: explicit arg, else the default release artifact.
    let kernel = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        os_root.join("target/x86_64-unknown-none/release/praxis-boot")
    });
    if !kernel.exists() {
        eprintln!("kernel ELF not found: {}", kernel.display());
        eprintln!(
            "build it first:\n  cargo build -p praxis-boot --target x86_64-unknown-none --release"
        );
        exit(1);
    }

    let dist = os_root.join("dist");
    if let Err(e) = std::fs::create_dir_all(&dist) {
        eprintln!("could not create {}: {e}", dist.display());
        exit(1);
    }
    let bios = dist.join("praxis-os-bios.img");
    let uefi = dist.join("praxis-os-uefi.img");

    println!("kernel : {}", kernel.display());
    println!("building BIOS image …");
    if let Err(e) = bootloader::BiosBoot::new(&kernel).create_disk_image(&bios) {
        eprintln!("BIOS image build failed: {e}");
        exit(1);
    }
    println!("building UEFI image …");
    if let Err(e) = bootloader::UefiBoot::new(&kernel).create_disk_image(&uefi) {
        eprintln!("UEFI image build failed: {e}");
        exit(1);
    }

    let bios_sz = std::fs::metadata(&bios).map(|m| m.len()).unwrap_or(0);
    let uefi_sz = std::fs::metadata(&uefi).map(|m| m.len()).unwrap_or(0);
    println!("\nwrote {} ({} KiB)", bios.display(), bios_sz / 1024);
    println!("wrote {} ({} KiB)", uefi.display(), uefi_sz / 1024);
    println!(
        "\nboot BIOS:  qemu-system-x86_64 -drive format=raw,file={} -serial stdio",
        bios.display()
    );
    println!(
        "boot UEFI:  qemu-system-x86_64 -bios OVMF.fd -drive format=raw,file={} -serial stdio",
        uefi.display()
    );
}
