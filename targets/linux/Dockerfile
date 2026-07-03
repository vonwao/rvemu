# Reproducible build of the Gate C Linux target: OpenSBI fw_payload.elf
# containing Linux 6.12.10 (rv64imac soft-float, single hart) with a static
# BusyBox 1.36.1 initramfs and the platform DTB baked in.
FROM debian:bookworm-20250630

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential gcc-riscv64-linux-gnu libc6-dev-riscv64-cross flex bison bc libssl-dev \
    libelf-dev cpio xz-utils wget ca-certificates device-tree-compiler \
    python3 file git rsync && rm -rf /var/lib/apt/lists/*

WORKDIR /build
# kernel.org's CDN is unreachable from the build network; the kernel comes
# from the official GitHub mirror instead, pinned to the mainline v6.12 tag.
ARG KERNEL_VERSION=6.12
ARG BUSYBOX_VERSION=1.36.1
ARG OPENSBI_VERSION=1.6

RUN wget -q https://github.com/torvalds/linux/archive/refs/tags/v${KERNEL_VERSION}.tar.gz -O linux-${KERNEL_VERSION}.tar.gz && \
    wget -q https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2 && \
    wget -q https://github.com/riscv-software-src/opensbi/archive/refs/tags/v${OPENSBI_VERSION}.tar.gz -O opensbi-${OPENSBI_VERSION}.tar.gz && \
    tar xf linux-${KERNEL_VERSION}.tar.gz && tar xf busybox-${BUSYBOX_VERSION}.tar.bz2 && tar xf opensbi-${OPENSBI_VERSION}.tar.gz

# --- Soft-float userland toolchain. Debian's riscv64-linux-gnu targets
# rv64gc/lp64d (hard float); a BusyBox built with it dies with SIGILL on our
# rv64imac target. musl-cross-make builds a pinned rv64imac/lp64 musl
# toolchain instead.
ARG MCM_COMMIT=227df8b99103f9c59f6570babf892978e293082f
RUN git clone https://github.com/richfelker/musl-cross-make.git && \
    cd musl-cross-make && git checkout ${MCM_COMMIT} && \
    { echo 'TARGET = riscv64-linux-musl'; \
      echo 'GCC_CONFIG += --with-arch=rv64imac --with-abi=lp64 --disable-shared'; \
      echo 'OUTPUT = /opt/cross'; } > config.mak && \
    make -j$(nproc) install && cd .. && rm -rf musl-cross-make
ENV PATH="/opt/cross/bin:${PATH}"

# --- BusyBox, static, soft-float.
RUN cd busybox-${BUSYBOX_VERSION} && \
    make CROSS_COMPILE=riscv64-linux-musl- defconfig && \
    sed -i 's/^# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config && \
    make CROSS_COMPILE=riscv64-linux-musl- -j$(nproc) busybox && \
    file busybox | grep -q 'statically linked' && \
    ! riscv64-linux-musl-objdump -d busybox | grep -qE '^\s+\w+:\s+\w+\s+f(l|s)[wd]\s' \
        || { echo 'FP instructions found in busybox'; exit 1; }

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
