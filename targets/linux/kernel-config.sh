#!/bin/bash
# Kernel config overrides applied on top of riscv defconfig, per the charter:
# soft-float rv64imac, single hart, initramfs baked in, no networking, no
# block/virtio devices, no modules. Run from the kernel source directory.
set -euo pipefail
cfg() { scripts/config "$@"; }

cfg --disable CONFIG_SMP
cfg --disable CONFIG_FPU
cfg --disable CONFIG_RISCV_ISA_V
cfg --disable CONFIG_MODULES
cfg --disable CONFIG_NET
cfg --disable CONFIG_BLOCK
cfg --disable CONFIG_PCI
cfg --disable CONFIG_VIRTIO_MENU
cfg --disable CONFIG_HVC_RISCV_SBI
cfg --disable CONFIG_RISCV_SBI_V01
cfg --disable CONFIG_KVM
cfg --disable CONFIG_EFI
# Sv39 only: no 4- or 5-level page tables (also forced via no4lvl/no5lvl in
# bootargs; belt and braces).
cfg --disable CONFIG_RISCV_MMU_SV48 2>/dev/null || true
cfg --set-str CONFIG_INITRAMFS_SOURCE "/build/initramfs"
cfg --enable CONFIG_SERIAL_8250
cfg --enable CONFIG_SERIAL_8250_CONSOLE
cfg --enable CONFIG_SERIAL_OF_PLATFORM
cfg --enable CONFIG_DEVTMPFS
cfg --enable CONFIG_DEVTMPFS_MOUNT
