// Memory-backed disk for xv6: the file-system image is embedded in the
// kernel binary at build time (fs_img.o, produced by objcopy in the
// Makefile), so no block device is required. Drop-in replacement for the
// virtio_disk.c driver interface; reads and writes complete synchronously.
#include "types.h"
#include "riscv.h"
#include "defs.h"
#include "param.h"
#include "fs.h"
#include "spinlock.h"
#include "sleeplock.h"
#include "buf.h"

extern char _binary_fs_img_start[];
extern char _binary_fs_img_size[];

void
virtio_disk_init(void)
{
}

void
virtio_disk_rw(struct buf *b, int write)
{
  uint64 size = (uint64)_binary_fs_img_size;
  uint64 off = (uint64)b->blockno * BSIZE;

  if(off + BSIZE > size)
    panic("memdisk: block out of range");
  if(write)
    memmove(_binary_fs_img_start + off, b->data, BSIZE);
  else
    memmove(b->data, _binary_fs_img_start + off, BSIZE);
}

void
virtio_disk_intr(void)
{
}
