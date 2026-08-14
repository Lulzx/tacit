// Boot and exception-level bring-up for the Tacit AArch64 stub.
//
// This file is the only assembly in the image.  It does exactly the
// architecture work the spec allows: exception levels, stack setup, BSS
// clear, vector table installation, interrupt entry, and FP enable.  Policy,
// fusion, placement and the graph all live above this in Uiua/UIR.

.section .text._start
.global _start
_start:
    mov x19, x0                     // x0 = fdt pointer from QEMU
    mrs x9, CurrentEL
    lsr x9, x9, #2
    and x9, x9, #0x3
    cmp x9, #3
    beq from_el3
    cmp x9, #2
    beq from_el2
    b from_el1

from_el3:
    mov x10, #0
    orr x10, x10, #(1 << 10)        // SCR_EL3.RW = 1 (AArch64 lower EL)
    orr x10, x10, #(1 << 0)         // SCR_EL3.NS = 1
    msr scr_el3, x10
    mov x10, #0x3c5                 // EL1h, DAIF masked
    msr spsr_el3, x10
    adr x10, from_el1
    msr elr_el3, x10
    isb
    eret

from_el2:
    mov x10, #(1 << 31)             // HCR_EL2.RW = 1 (AArch64 EL1)
    msr hcr_el2, x10
    mov x10, #0x3c5
    msr spsr_el2, x10
    adr x10, from_el1
    msr elr_el2, x10
    isb
    eret

from_el1:
    adrp x10, _stack_top
    add x10, x10, :lo12:_stack_top
    mov sp, x10

    adrp x10, __bss_start
    add x10, x10, :lo12:__bss_start
    adrp x11, __bss_end
    add x11, x11, :lo12:__bss_end
1:
    cmp x10, x11
    b.ge 2f
    str xzr, [x10], #8
    b 1b
2:
    adrp x10, _vector_table
    add x10, x10, :lo12:_vector_table
    msr vbar_el1, x10

    // Enable FP/SIMD (needed for f32 elementwise work)
    mrs x10, cpacr_el1
    orr x10, x10, #(3 << 20)        // FPEN = 0b11
    msr cpacr_el1, x10
    isb

    mov x0, x19
    bl kernel_main

halt:
    wfe
    b halt

.global halt_loop
halt_loop:
    wfe
    b halt_loop

// ---------------------------------------------------------------------------
// Vector table (EL1).  Each entry is 128 bytes.
// ---------------------------------------------------------------------------
.section .text.vectors
.balign 0x800
_vector_table:
.macro VENT label
    .balign 0x80
    b \label
.endm
    VENT sync_handler        // 0: cur EL, SP0, sync
    VENT irq_handler         // 1: cur EL, SP0, IRQ
    VENT hang                // 2: cur EL, SP0, FIQ
    VENT hang                // 3: cur EL, SP0, SError
    VENT sync_handler        // 4: cur EL, SPx, sync
    VENT irq_handler         // 5: cur EL, SPx, IRQ
    VENT hang                // 6: cur EL, SPx, FIQ
    VENT hang                // 7: cur EL, SPx, SError
    VENT sync_handler        // 8: lower EL AArch64, sync
    VENT irq_handler         // 9: lower EL AArch64, IRQ
    VENT hang                // 10
    VENT hang                // 11
    VENT hang                // 12: lower EL AArch32, sync
    VENT hang                // 13
    VENT hang                // 14
    VENT hang                // 15

hang:
    wfe
    b hang

// ---------------------------------------------------------------------------
// Context save / restore for the IRQ handler
// ---------------------------------------------------------------------------
.macro SAVE_CONTEXT
    sub sp, sp, #(8 * 34)
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    stp x6, x7, [sp, #48]
    stp x8, x9, [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    mrs x0, elr_el1
    mrs x1, spsr_el1
    stp x0, x1, [sp, #248]
.endm

.macro RESTORE_CONTEXT
    ldp x0, x1, [sp, #248]
    msr elr_el1, x0
    msr spsr_el1, x1
    ldr x30, [sp, #240]
    ldp x28, x29, [sp, #224]
    ldp x26, x27, [sp, #208]
    ldp x24, x25, [sp, #192]
    ldp x22, x23, [sp, #176]
    ldp x20, x21, [sp, #160]
    ldp x18, x19, [sp, #144]
    ldp x16, x17, [sp, #128]
    ldp x14, x15, [sp, #112]
    ldp x12, x13, [sp, #96]
    ldp x10, x11, [sp, #80]
    ldp x8, x9, [sp, #64]
    ldp x6, x7, [sp, #48]
    ldp x4, x5, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp, #0]
    add sp, sp, #(8 * 34)
.endm

irq_handler:
    SAVE_CONTEXT
    bl irq_dispatch
    RESTORE_CONTEXT
    eret

sync_handler:
    // Pass ESR_EL1, ELR_EL1, FAR_EL1 to Rust; it prints and halts.
    sub sp, sp, #(8 * 6)
    stp x0, x1, [sp, #0]
    stp x2, x3, [sp, #16]
    stp x4, x5, [sp, #32]
    mrs x0, esr_el1
    mrs x1, elr_el1
    mrs x2, far_el1
    bl sync_exception
    ldp x4, x5, [sp, #32]
    ldp x2, x3, [sp, #16]
    ldp x0, x1, [sp, #0]
    add sp, sp, #(8 * 6)
    b halt

// ---------------------------------------------------------------------------
// Stack
// ---------------------------------------------------------------------------
.section .bss.stack
.balign 16
.global _stack_bottom
_stack_bottom:
    .space 1024 * 64
.global _stack_top
_stack_top:
