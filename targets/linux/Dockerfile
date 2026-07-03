# Reproducible build of the Gate C Linux target: OpenSBI fw_payload.elf
# containing Linux 6.12.10 (rv64imac soft-float, single hart) with a static
# BusyBox 1.36.1 initramfs and the platform DTB baked in.
FROM debian:bookworm-20250630

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential gcc-riscv64-linux-gnu flex bison bc libssl-dev \
    libelf-dev cpio xz-utils wget ca-certificates device-tree-compiler \
    python3 file && rm -rf /var/lib/apt/lists/*

WORKDIR /build
ARG KERNEL_VERSION=6.12.10
ARG BUSYBOX_VERSION=1.36.1
ARG OPENSBI_VERSION=1.6

RUN wget -q https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz && \
    wget -q https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2 && \
    wget -q https://github.com/riscv-software-src/opensbi/archive/refs/tags/v${OPENSBI_VERSION}.tar.gz -O opensbi-${OPENSBI_VERSION}.tar.gz && \
    tar xf linux-${KERNEL_VERSION}.tar.xz && tar xf busybox-${BUSYBOX_VERSION}.tar.bz2 && tar xf opensbi-${OPENSBI_VERSION}.tar.gz

# --- BusyBox, static.
RUN cd busybox-${BUSYBOX_VERSION} && \
    make CROSS_COMPILE=riscv64-linux-gnu- defconfig && \
    sed -i 's/^# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config && \
    make CROSS_COMPILE=riscv64-linux-gnu- -j$(nproc) busybox && \
    file busybox | grep -q 'statically linked'

# --- Initramfs tree.
COPY initramfs-init /build/initramfs/init
RUN cd /build/initramfs && mkdir -p bin sbin proc sys dev etc tmp && \
    cp /build/busybox-${BUSYBOX_VERSION}/busybox bin/busybox && \
    chmod +x init bin/busybox

# --- Kernel: defconfig, then force the target platform down to
# rv64imac / single hart / soft-float, initramfs baked in.
COPY kernel-config.sh /build/kernel-config.sh
RUN cd linux-${KERNEL_VERSION} && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- defconfig && \
    bash /build/kernel-config.sh && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- olddefconfig && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- -j$(nproc) Image && \
    cp arch/riscv/boot/Image /build/Image && cp .config /build/kernel.config

# --- Platform DTB.
COPY platform.dts /build/platform.dts
RUN dtc -I dts -O dtb -o /build/platform.dtb /build/platform.dts

# --- OpenSBI generic firmware with kernel payload and DTB embedded.
RUN cd opensbi-${OPENSBI_VERSION} && \
    make CROSS_COMPILE=riscv64-linux-gnu- PLATFORM=generic \
         PLATFORM_RISCV_ISA=rv64imac_zicsr_zifencei PLATFORM_RISCV_ABI=lp64 \
         FW_PAYLOAD_PATH=/build/Image FW_FDT_PATH=/build/platform.dtb -j$(nproc) && \
    cp build/platform/generic/firmware/fw_payload.elf /build/fw_payload.elf

CMD ["true"]
