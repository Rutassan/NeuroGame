MEMORY {
  RAM : ORIGIN = 0x0010_0000, LENGTH = 16M
}
SECTIONS {
  . = ORIGIN(RAM);
  .text : { *(.text*) }
  .rodata : { *(.rodata*) }
  .data : { *(.data*) }
  .bss : { *(.bss*) }
}
ENTRY(_start);
