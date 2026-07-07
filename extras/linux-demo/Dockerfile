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

# Framebuffer: simple-framebuffer at 0x90000000 (800x600 rgb565), exposed as
# /dev/fb0 only — no fbcon, no boot logo, so the screen stays black until a
# program draws real graphics (the page shows the canvas on the first nonzero
# pixel). Kernel messages go to the serial console alone. Lockstep note: the
# fb region does not exist on the pinned Spike, so this image is not
# Spike-comparable (extras/README.md).
COPY platform-demo.dts /build/platform-demo.dts
RUN dtc -I dts -O dtb -o /build/platform-demo.dtb /build/platform-demo.dts && \
    cd /build/linux-6.12 && \
    scripts/config --enable CONFIG_FB --enable CONFIG_FB_SIMPLE \
      --enable CONFIG_FB_DEVICE \
      --disable CONFIG_FRAMEBUFFER_CONSOLE --disable CONFIG_LOGO

# Networking: virtio-mmio NIC bridged to the page's JS gateway (extras only;
# the certified base image has CONFIG_NET disabled per the charter).
RUN cd /build/linux-6.12 && \
    scripts/config --enable CONFIG_NET --enable CONFIG_INET --enable CONFIG_UNIX \
      --enable CONFIG_PACKET --enable CONFIG_NETDEVICES --enable CONFIG_NET_CORE \
      --enable CONFIG_VIRTIO_MENU --enable CONFIG_VIRTIO_MMIO --enable CONFIG_VIRTIO_NET

# Input: virtio-input (keyboard + mouse events from the page canvas),
# evdev for fbDOOM, mousedev (/dev/input/mice) for links-style fb apps.
RUN cd /build/linux-6.12 && \
    scripts/config --enable CONFIG_INPUT --enable CONFIG_INPUT_EVDEV \
      --enable CONFIG_INPUT_MOUSEDEV --enable CONFIG_VIRTIO_INPUT

# fbDOOM (pinned) drawing to /dev/fb0, static musl; shareware doom1.wad
# (freely distributable, pinned sha256). `doom` wrapper for the demo.
ARG FBDOOM_COMMIT=17280163bc95e5d954d2efaa0633489b763b4cd1
ARG DOOM1_WAD_SHA256=1d7d43be501e67d927e415e0b8f3e29c3bf33075e859721816f652a526cac771
RUN git clone https://github.com/maximevince/fbDOOM.git /build/fbdoom-src && \
    cd /build/fbdoom-src && git checkout ${FBDOOM_COMMIT} && \
    make -C fbdoom CROSS_COMPILE=riscv64-linux-musl- NOSDL=1 LDFLAGS=-static && \
    cp fbdoom/fbdoom /build/initramfs/bin/fbdoom && \
    file /build/initramfs/bin/fbdoom | grep -q 'statically linked'
RUN wget -q -O /build/initramfs/doom1.wad https://distro.ibiblio.org/slitaz/sources/packages/d/doom1.wad && \
    echo "${DOOM1_WAD_SHA256}  /build/initramfs/doom1.wad" | sha256sum -c - && \
    printf '#!/bin/sh\nexec fbdoom -iwad /doom1.wad "$@"\n' > /build/initramfs/bin/doom && \
    chmod +x /build/initramfs/bin/doom

# links2 in graphics mode: fb output (/dev/fb0), mouse via /dev/input/mice,
# plain http (the page gateway terminates TLS). Static musl with zlib+libpng.
ARG ZLIB_URL=https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz
ARG LIBPNG_URL=https://downloads.sourceforge.net/project/libpng/libpng16/1.6.43/libpng-1.6.43.tar.gz
# links + gpm tarballs are pre-fetched by build-linux-demo.sh (twibright.com
# is not reliably reachable from inside Docker) and sha256-verified there.
COPY links-2.30.tar.gz /build/links.tgz
COPY gpm-1.20.7.tar.gz /build/gpm.tgz

# gpm: links2's fb driver requires libgpm at build time and the gpm daemon
# at runtime for the mouse (kernel mousedev -> gpm -> links).
RUN apt-get update -q && apt-get install -y -q autoconf automake libtool && \
    cd /build && tar xzf gpm.tgz && cd gpm-1.20.7 && \
    ./autogen.sh && \
    grep -q '#include <fcntl.h>' src/daemon/open_console.c && \
    sed -i 's|#include <fcntl.h>                  /\* open and co.      \*/|#include <fcntl.h>\n#include <sys/sysmacros.h> /* major() on musl */|' src/daemon/open_console.c && \
    grep -q sysmacros src/daemon/open_console.c && \
    CC=riscv64-linux-musl-gcc LDFLAGS=-static \
      ./configure --host=riscv64-linux-musl --prefix=/opt/sysroot --without-curses && \
    make -C src -j$(nproc) gpm lib/libgpm.a && \
    mkdir -p /opt/sysroot/lib /opt/sysroot/include && \
    cp src/lib/libgpm.a /opt/sysroot/lib/ && \
    cp src/headers/gpm.h /opt/sysroot/include/ && \
    cp src/gpm /build/initramfs/bin/gpm && \
    file /build/initramfs/bin/gpm | grep -q 'statically linked'
RUN mkdir -p /opt/sysroot && cd /build && \
    wget -q -O zlib.tgz ${ZLIB_URL} && tar xzf zlib.tgz && cd zlib-1.3.1 && \
    CC=riscv64-linux-musl-gcc ./configure --static --prefix=/opt/sysroot && \
    make -j$(nproc) && make install
RUN cd /build && wget -q -O libpng.tgz ${LIBPNG_URL} && tar xzf libpng.tgz && cd libpng-1.6.43 && \
    ./configure --host=riscv64-linux-musl --disable-shared --prefix=/opt/sysroot \
      CC=riscv64-linux-musl-gcc CPPFLAGS=-I/opt/sysroot/include LDFLAGS=-L/opt/sysroot/lib && \
    make -j$(nproc) && make install
RUN cd /build && tar xzf links.tgz && cd links-2.30 && \
    CC=riscv64-linux-musl-gcc CPPFLAGS=-I/opt/sysroot/include \
      LDFLAGS="-static -L/opt/sysroot/lib" \
      ./configure --host=riscv64-linux-musl --enable-graphics --with-fb \
      --without-x --without-directfb --without-libjpeg --without-libtiff --without-openmp \
      --without-ssl --without-zstd --without-brotli --without-freetype --without-librsvg && \
    make -j$(nproc) && \
    cp links /build/initramfs/bin/links && \
    file /build/initramfs/bin/links | grep -q 'statically linked' && \
    printf '#!/bin/sh\n# links -g needs a real VT; the shell is on the serial console, so point\n# links at tty1 (the framebuffer the page mirrors to its canvas).\nexec setsid sh -c '"'"'exec links -g "${1:-http://raw.githubusercontent.com/vonwao/rvemu/main/README.md}" </dev/tty1 >/dev/tty1 2>&1'"'"' -- "$@"\n' > /build/initramfs/bin/browse && \
    chmod +x /build/initramfs/bin/browse

RUN printf 'file /bin/tetris /build/initramfs/bin/tetris 0755 0 0\nfile /bin/fbdoom /build/initramfs/bin/fbdoom 0755 0 0\nfile /bin/doom /build/initramfs/bin/doom 0755 0 0\nfile /doom1.wad /build/initramfs/doom1.wad 0644 0 0\nfile /bin/links /build/initramfs/bin/links 0755 0 0\nfile /bin/browse /build/initramfs/bin/browse 0755 0 0\nfile /bin/gpm /build/initramfs/bin/gpm 0755 0 0\n' >> /build/initramfs.list && \
    cd /build/linux-6.12 && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- olddefconfig && \
    make ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- -j$(nproc) Image && \
    cp arch/riscv/boot/Image /build/Image-demo

RUN cd /build/opensbi-1.6 && \
    make CROSS_COMPILE=riscv64-linux-gnu- PLATFORM=generic \
         PLATFORM_RISCV_ISA=rv64imac_zicsr_zifencei PLATFORM_RISCV_ABI=lp64 \
         FW_PAYLOAD_PATH=/build/Image-demo FW_FDT_PATH=/build/platform-demo.dtb -j$(nproc) && \
    cp build/platform/generic/firmware/fw_payload.elf /build/fw_payload_demo.elf
