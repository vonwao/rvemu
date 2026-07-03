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

# fs.img must exist before kernel objects link it in; build it first.
make TOOLPREFIX=riscv-none-elf- fs.img
make TOOLPREFIX=riscv-none-elf- kernel/kernel

echo "xv6 kernel: $BUILD/kernel/kernel"
"$TOOLBIN/riscv-none-elf-readelf" -h kernel/kernel | grep -E 'Entry|Machine'
