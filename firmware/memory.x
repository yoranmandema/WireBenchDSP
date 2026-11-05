/* STM32H723VG memory map (LQFP100, 1 MB flash, 564 KB RAM).
 *
 *   ITCM    64 KB  0x0000_0000
 *   DTCM   128 KB  0x2000_0000
 *   AXI    320 KB  0x2400_0000   <- main RAM (.data/.bss), DMA accessible
 *   AHB     32 KB  0x3000_0000   (SRAM1+SRAM2, D2 domain)
 *   D3      16 KB  0x3800_0000
 *   FLASH 1024 KB  0x0800_0000
 *
 * cortex-m-rt places .data/.bss and the stack in the region named RAM, so the
 * big DSP buffers land in AXI SRAM where DMA1/DMA2 can reach them.
 */
MEMORY
{
  ITCM   : ORIGIN = 0x00000000, LENGTH = 64K
  DTCM   : ORIGIN = 0x20000000, LENGTH = 128K
  RAM    : ORIGIN = 0x24000000, LENGTH = 320K
  RAM_D2 : ORIGIN = 0x30000000, LENGTH = 32K
  RAM_D3 : ORIGIN = 0x38000000, LENGTH = 16K
  FLASH  : ORIGIN = 0x08000000, LENGTH = 1024K
}
