ENTRY(_start)

PHDRS
{
    text PT_LOAD FLAGS(5);
    data PT_LOAD FLAGS(6);
}

SECTIONS
{
    . = 0x40200000;

    .text : {
        KEEP(*(.text._start))
        KEEP(*(.text.vectors))
        *(.text .text.*)
    } :text

    .rodata : { *(.rodata .rodata.*) } :text

    .data : { *(.data .data.*) } :data

    .bss : {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } :data

    . = ALIGN(16);
    __end = .;
}
