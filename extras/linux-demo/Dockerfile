# linux-demo: the certified Linux image plus demo userland (Micro Tetris).
# Layered on the pinned rvemu-linux-target build; nothing in the base recipe
# changes. Lockstep note: this image adds only userland binaries and an
# initramfs entry — it remains fully Spike-comparable.
FROM rvemu-linux-target

ARG TETRIS_COMMIT=aafa95ea5b4d126f53a38cced6469b50e8f7a244
RUN git clone https://github.com/troglobit/tetris.git /build/tetris && \
    cd /build/tetris && git checkout ${TETRIS_COMMIT} && \
    riscv64-linux-musl-gcc -O2 -static -o /build/initramfs/bin/tetris tetris.c && \
    file /build/initramfs/bin/tetris | grep -q 'statically linked'

# Serial ttys have no window size; tetris (and friends) need one.
COPY demo-init /build/initramfs/init

# Framebuffer console: simple-framebuffer at 0x90000000 (800x600 rgb565),
# fbcon + boot logo. Lockstep note: the fb region does not exist on the
# pinned Spike, so this image is not Spike-comparable (extras/README.md).
COPY platform-demo.dts /build/platform-demo.dts
RUN dtc -I dts -O dtb -o /build/platform-demo.dtb /build/platform-demo.dts && \
    cd /build/linux-6.12 && \
    scripts/config --enable CONFIG_FB --enable CONFIG_FB_SIMPLE \
      --enable CONFIG_FB_DEVICE \
      --enable CONFIG_FRAMEBUFFER_CONSOLE --enable CONFIG_FRAMEBUFFER_CONSOLE_DETECT_PRIMARY \
      --enable CONFIG_LOGO --enable CONFIG_LOGO_LINUX_CLUT224 --enable CONFIG_FONT_8x16

RUN printf 'file /bin/tetris /build/initramfs/bin/tetris 0755 0 0\n' >> /build/initramfs.list && \
    cd /build/linux-6.12 && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- olddefconfig && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- -j$(nproc) Image && \
    cp arch/riscv/boot/Image /build/Image-demo

RUN cd /build/opensbi-1.6 && \
    make CROSS_COMPILE=riscv64-linux-gnu- PLATFORM=generic \
         PLATFORM_RISCV_ISA=rv64imac_zicsr_zifencei PLATFORM_RISCV_ABI=lp64 \
         FW_PAYLOAD_PATH=/build/Image-demo FW_FDT_PATH=/build/platform-demo.dtb -j$(nproc) && \
    cp build/platform/generic/firmware/fw_payload.elf /build/fw_payload_demo.elf
