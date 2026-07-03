#!/bin/bash
# Build the pinned xv6-riscv into a self-contained kernel image (embedded
# ramdisk fs, no block device), adapted to the pinned Spike's platform.
# Produces targets/vendor/xv6-build/kernel/kernel (ELF, entry 0x80000000).
#
# Adaptations (this script is the recorded recipe; targets/xv6-patches/ holds
# the new driver source):
#  - kernel/virtio_disk.c replaced by memdisk.c: fs.img is embedded in the
#    kernel binary via objcopy and served synchronously from memory.
#  - UART0_IRQ 10 -> 1: the pinned Spike wires its ns16550 to PLIC source 1
#    (qemu-virt uses 10). rvemu models the Spike platform so lockstep and
#    boot behave identically.
#  - PTEs get PTE_A|PTE_D at creation: the pinned Spike implements Svade
#    (page fault on access with A=0 / store with D=0) rather than qemu's
#    hardware A/D update; unpatched xv6 silently trap-loops at paging-on.
#  - UART: IER enables RX only and uartputc transmits synchronously. Spike's
#    ns16550 asserts a level-triggered THR-empty interrupt that xv6 (tuned
#    to qemu's edge behavior) never clears, livelocking the kernel in an
#    interrupt storm.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SRC="$HERE/vendor/xv6-riscv"
BUILD="$HERE/vendor/xv6-build"
TOOLBIN="$HERE/vendor/xpack-riscv-none-elf-gcc-15.2.0-1/bin"
export PATH="$TOOLBIN:$PATH"

rm -rf "$BUILD"
cp -R "$SRC" "$BUILD"
cd "$BUILD"

cp "$HERE/xv6-patches/memdisk.c" kernel/memdisk.c
rm kernel/virtio_disk.c

# Swap the disk driver object and add the embedded fs image object.
sed -i.bak 's|\$K/virtio_disk.o|$K/memdisk.o \\\n  $K/fs_img.o|' Makefile
sed -i.bak2 's|#define UART0_IRQ 10|#define UART0_IRQ 1|' kernel/memlayout.h
# Soft-float rv64imac target (no F/D), explicit ABI for the xPack toolchain.
sed -i.bak3 's|-march=rv64gc|-march=rv64imac_zicsr_zifencei -mabi=lp64|' Makefile
# xPack ld needs the 64-bit emulation named explicitly.
sed -i.bak4 's|^LDFLAGS = |LDFLAGS = -m elf64lriscv |' Makefile
cat >> Makefile <<'EOF'

$K/fs_img.o: fs.img
	$(OBJCOPY) -I binary -O elf64-littleriscv fs.img $K/fs_img.o
EOF

python3 - <<'PYEOF'
import re

r = open('kernel/riscv.h').read()
if 'PTE_A' not in r:
    r = r.replace('#define PTE_U (1L << 4) // user can access',
                  '#define PTE_U (1L << 4) // user can access\n#define PTE_A (1L << 6)\n#define PTE_D (1L << 7)')
    open('kernel/riscv.h', 'w').write(r)

v = open('kernel/vm.c').read()
v = v.replace('    *pte = PA2PTE(pa) | perm | PTE_V;',
              '    *pte = PA2PTE(pa) | perm | PTE_V | PTE_A | PTE_D;')
open('kernel/vm.c', 'w').write(v)

u = open('kernel/uart.c').read()
u = u.replace('  WriteReg(IER, IER_TX_ENABLE | IER_RX_ENABLE);',
              '  WriteReg(IER, IER_RX_ENABLE);')
u = u.replace('''  acquire(&tx_lock);

  int i = 0;
  while (i < n) {
    while (tx_busy != 0) {
      // wait for a UART transmit-complete interrupt
      // to set tx_busy to 0.
      sleep(&tx_chan, &tx_lock);
    }

    WriteReg(THR, buf[i]);
    i += 1;
    tx_busy = 1;
  }

  release(&tx_lock);''', '''  // Synchronous transmit; see build-xv6.sh header. Spike's THR never
  // stays busy, so this cannot block.
  acquire(&tx_lock);
  for (int i = 0; i < n; i++)
    uartputc_sync(buf[i]);
  release(&tx_lock);''')
open('kernel/uart.c', 'w').write(u)
PYEOF

# fs.img must exist before kernel objects link it in; build it first.
make TOOLPREFIX=riscv-none-elf- fs.img
make TOOLPREFIX=riscv-none-elf- kernel/kernel

echo "xv6 kernel: $BUILD/kernel/kernel"
"$TOOLBIN/riscv-none-elf-readelf" -h kernel/kernel | grep -E 'Entry|Machine'
