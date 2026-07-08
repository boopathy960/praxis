/* Raspberry Pi 3 (AArch64) load layout: the GPU firmware / QEMU `-M raspi3b`
 * loads kernel8.img at 0x80000 and jumps there, so `_start` must be the very
 * first byte. RAM below 0x80000 is the descending boot stack. */
ENTRY(_start)

SECTIONS
{
    . = 0x80000;

    .text : {
        KEEP(*(.text.boot))   /* the AArch64 entry stub, first */
        *(.text .text.*)
    }

    .rodata : { *(.rodata .rodata.*) }
    .data   : { *(.data .data.*) }

    . = ALIGN(16);
    __bss_start = .;
    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    }
    . = ALIGN(16);
    __bss_end = .;
}
