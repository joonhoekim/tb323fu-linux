# Installing Linux

How to get from a rooted TB323FU (Android with KernelSU, see [Rooting and dual boot setup](rooting.md)) to mainline
Linux booting from its own partition, with Android kept as the way back.

> **No release has been published yet.** A release will be a kernel `Image` (with all its modules inside) and a repack tool — never
> a ready-made `boot.img` ([why](#why-there-is-no-ready-made-bootimg)); until then you build the kernel yourself (step 3)
> and put together a root filesystem with the scripts in this repository. This guide was **reconstructed from the development records** of one
> tablet; the individual steps were done on that tablet, but the guide as a whole has **not been re-run end to end**
> by someone following it. Read it once completely before starting.

Every step carries one of these labels:

| Label | Meaning |
|---|---|
| **[verified]** | done on the development tablet with the command or tool shown |
| **[from records]** | done on the development tablet, but the exact commands here are reconstructed from notes and were not re-run in this form |
| **[untested]** | not done on any tablet; follows from how the tools work |

There is also a guided script, [`tools/install/`](../tools/install/README.md), that walks through the same steps
interactively. It is a **prototype, untested on real hardware**.

## What you end up with

```
 UFS (internal storage)                                microSD card (optional, multiboot)
 ┌──────────────────────────────────────────────┐      ┌──────────────────────────────────────┐
 │ LUN 4: boot_a  ← Linux boot image             │      │ GPT                                   │
 │        boot_b  ← copy of your Android boot    │      │  tb323fu-ubuntu   ext4  (a root)      │
 │                  image (the way back)         │      │  tb323fu-arch     ext4  (a root)      │
 │        init_boot_a ← KernelSU (from rooting)  │      │  tb323fu-nixos    ext4  (a root)      │
 │ LUN 0: … userdata (Android data, shrunk)      │      │  …                                    │
 │        baldur-root  ext4 (default Linux root) │      │  or: baldur-root-sd / baldur-root     │
 └──────────────────────────────────────────────┘      └──────────────────────────────────────┘
```

The bootloader always boots slot `_a`. `boot_a` holds either Android's boot image or the Linux one; switching
copies one or the other into it. The Linux boot image carries its own device tree, command line and initramfs; the
initramfs picks a root filesystem **by GPT partition name** ([details](../kernel/initramfs/README.md#root-partitions-and-multiboot)).

## Overview

| Step | What changes | Reversible? | Status |
|---|---|---|---|
| 0. Rooting and backup ([rooting.md](rooting.md)) | `efisp`, `init_boot_a` | yes ([recovery](recovery.md#undoing-the-root)) | [verified] |
| 1. Extract firmware | nothing on the tablet (a copy in `/data/local/tmp`) | — | [verified] |
| 2. The way back: Android boot image into `boot_b` | `boot_b` | yes (`boot_b` is never booted) | [verified] |
| 3. The Linux boot image: repack a release kernel, or build one | nothing (PC) | — | repack [verified], build [from records] |
| 4. Partition a microSD card | **the card is wiped** | the card only | [verified] (from Android) / [untested] (from a PC) |
| 5. Root filesystem | the new partition | yes (delete it) | [verified] on the tablet, [untested] on another host |
| 6. Write `boot_a` and boot Linux | `boot_a` | yes: back-to-android, emergency keys, EDL | [verified] |
| 7. First-boot checks | nothing | — | [verified] |
| 8. Optional: Linux root on internal storage | GPT of LUN 0, **Android's data is wiped** | only with another factory reset | [from records] |

Steps 1–2 happen in rooted Android, 3 on a PC, 4–5 on a PC or in Android, 6 from Android. After step 6 Linux runs
from the microSD card. Step 8 moves (or adds) a root on the internal storage and is the only step that costs Android
its data.

## Prerequisites

- Steps 1–3 of [rooting.md](rooting.md) done: a **full EDL dump of your own tablet stored off the PC**, LTBox's
  `init_boot` backup, KernelSU with **Shell** allowed, OTA apps disabled.
- `adb` on the PC ([platform-tools](https://developer.android.com/tools/releases/platform-tools)).
- A **Linux PC** for building the kernel (x86-64 is fine: the kernel cross-builds with `LLVM=1`). macOS or Windows:
  use a Linux VM or WSL2 [untested].
- A **Linux host that runs arm64 programs** for the root filesystem builders in [`rootfs/`](../rootfs/): an arm64
  machine (on the development tablet they ran on the tablet itself, under Linux), or an x86-64 Linux with
  `qemu-user-binfmt` (arm64 programs through qemu; the builders check that it is registered) — **on Windows,
  WSL2 with Ubuntu works**: `sudo apt install qemu-user-binfmt debootstrap libarchive-tools gpg curl e2fsprogs`.
  Through qemu the Arch builder took about 4 min and the Ubuntu builder 18 min without a desktop on the development
  PC (WSL2, Ubuntu 26.04) [verified: the builds; such a root has not been booted yet]; with GNOME expect much longer.
- A **microSD card**, 64 GB or more (each root takes 10–30 GB; GNOME roots are on the larger side).
- Battery above 50 %, a USB-C data cable.

## 1. Extract the firmware

**[verified]** No firmware is distributed by this project; it is copied from your own Android `/vendor`. Details and the manifest
are in [`firmware/README.md`](../firmware/README.md).

```sh
adb push firmware/manifest.tsv firmware/extract-on-device.sh /data/local/tmp/
adb shell su -c 'sh /data/local/tmp/extract-on-device.sh /data/local/tmp/tb323fu-firmware'
# copy it to the PC (tar keeps the layout; toybox tar is enough)
adb shell su -c 'tar -C /data/local/tmp -cf /data/local/tmp/tb323fu-firmware.tar tb323fu-firmware'
adb pull /data/local/tmp/tb323fu-firmware.tar
mkdir -p fw && tar -C fw -xf tb323fu-firmware.tar     # → fw/tb323fu-firmware/lib/firmware/...
```

The extraction was checked; the `tar` + `pull` transport is [from records] (the development PC copied it differently).
A hash mismatch is a warning (another firmware version), a missing file an error.

## 2. The way back: Android's boot image in `boot_b`

**[verified]** From [rooting.md → Set up the way back](rooting.md#set-up-the-way-back):

```sh
android/install-module.sh prepare-boot-b      # copies the running Android boot image (boot_a) into boot_b; asks first
android/install-module.sh install             # refuses unless boot_a == boot_b; installs the "Switch to Linux" module
android/install-module.sh status              # prints the hashes
```

Keep the printed sha256 of `boot_b`: it goes into the boot image (`-a`) and into every root filesystem as
`/etc/tb323fu/android-boot.sha256`. Put it in a small directory the builders read:

```sh
mkdir -p tb323fu-config
echo <sha256 of boot_b> > tb323fu-config/android-boot.sha256
```

Also get your **stock boot image** for step 3 (it is the same image as `boot_b`). Every Linux boot image for your
tablet is made from it, so keep it with your backup:

```sh
adb shell su -c 'dd if=/dev/block/by-name/boot_b of=/data/local/tmp/stock-boot.img'
adb pull /data/local/tmp/stock-boot.img
sha256sum stock-boot.img        # must equal the hash above
```

## 3. The Linux boot image

A Linux boot image for this tablet is **your stock `boot.img` with only the kernel replaced**: the header, the GKI
boot signature and the vbmeta blob with its AVB footer are copied from the stock image, and the bootloader (with the
GBL from rooting) accepts that. The kernel carries its own device tree, command line and initramfs.

### Why there is no ready-made `boot.img`

A boot image built by this project would carry the builder's stock header plus Lenovo's GKI signature and vbmeta
blobs. Those are Lenovo's and are not ours to redistribute, and they have to match the firmware version and region of
the tablet they are written to. So a release ships the kernel `Image`, the modules and the repack tool, and you make
the image from your own stock `boot.img` (step 2). The same applies to images you build yourself: only ever use your
own tablet's stock image.

### 3a. From a release

**[untested as a whole]** (the repack tool and its output are [verified]: every kernel on the development tablet was
packed this way). Releases are GitHub Releases of this repository (tag `kernel-tNN`). A release has
`Image-tb323fu-tNN` (and the same as `Image-tb323fu-tNN.gz`), `boot-repack-kernel.py` and `SHA256SUMS` (plus the
kernel configuration and initramfs lists for the GPL sources, and `modules-tb323fu-tNN.tar.gz`, which you normally do
not need, below). `SHA256SUMS` catches a damaged download; it is not a signature:

```sh
sha256sum -c SHA256SUMS
python3 boot-repack-kernel.py stock-boot.img Image-tb323fu-tNN linux-boot.img
```

**Modules come with the kernel.** The `Image` carries all modules of its kernel (the out-of-tree speaker amplifier
driver included) as a squashfs in its initramfs, and the initramfs mounts it read-only on `/lib/modules/<release>`
of whichever root it boots ([kernel/initramfs/README.md](../kernel/initramfs/README.md#kernel-modules-shared)). Nothing
is installed into the root filesystems, and a kernel update is only the new boot image. **[verified]** on the
development tablet (Debian, Ubuntu, Arch, Fedora, NixOS, 2026-10-02). The modules tarball is the same tree, for a root
that opts out (`own` in its `/etc/tb323fu/modules`, e.g. for DKMS).

`tools/install/install.sh boot` runs the repack for you (`KERNEL_IMAGE=…`). The tool also accepts a gzip-compressed
kernel (`Image.gz`; the bootloader decompresses it — checked once on the development tablet); LZ4 is refused, the
bootloader has no LZ4 decompressor. The release initramfs has **no firmware** (it cannot be redistributed): drivers that
need firmware get it from the root filesystem once it is mounted, which is why every root needs the firmware from
step 1 — see [kernel/initramfs/README.md](../kernel/initramfs/README.md).

### 3b. Build it yourself

**[from records]** The tools are the ones used for every kernel on the development tablet; the sequence below follows
[`kernel/README.md`](../kernel/README.md), [`kernel/initramfs/README.md`](../kernel/initramfs/README.md) and
[`tools/build-boot.sh`](../tools/build-boot.sh). Needs clang/LLVM, make, python3, an aarch64 C compiler for two
small static helpers (`aarch64-linux-gnu-gcc`), a static aarch64 busybox (e.g. Debian's `busybox-static` for arm64),
console-setup's `Lat15-Terminus28x14` font and wireless-regdb's `regulatory.db`.

```sh
# kernel tree: upstream base + this series
git clone --depth 1 -b v7.3-rc4 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git linux-tb323fu
cd linux-tb323fu
git am ../tb323fu-linux/kernel/patches/*.patch
make ARCH=arm64 LLVM=1 O=out defconfig
scripts/kconfig/merge_config.sh -m -O out out/.config \
    arch/arm64/configs/kaanapali-oneplus-infiniti_defconfig ../tb323fu-linux/kernel/config/baldur.fragment \
    ../tb323fu-linux/kernel/config/baldur-display.fragment ../tb323fu-linux/kernel/config/baldur-netfilter.fragment
make ARCH=arm64 LLVM=1 O=out olddefconfig
# a release name of your own, unique per build (old and new modules never collide)
scripts/config --file out/.config --set-str LOCALVERSION -tb323fu-mybuild1
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= olddefconfig
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= -j"$(nproc)" dtbs modules headers_install
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= INSTALL_MOD_PATH="$PWD/mods" INSTALL_MOD_STRIP=1 modules_install
# the speaker amplifier driver (out of tree) into mods/lib/modules/<release>/extra/
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= M="$PWD/../tb323fu-linux/kernel/out-of-tree/aw882xx" \
    CONFIG_SND_SOC_AW882XX=m INSTALL_MOD_PATH="$PWD/mods" INSTALL_MOD_STRIP=1 modules modules_install
rel=$(make -s ARCH=arm64 LLVM=1 O=out LOCALVERSION= kernelrelease)
rm -f mods/lib/modules/$rel/build mods/lib/modules/$rel/source; depmod -b mods $rel

# initramfs: all modules (-m, needs mksquashfs), firmware from step 1, hash from step 2
../tb323fu-linux/kernel/initramfs/build.sh -k out -b /path/to/busybox-static -m mods/lib/modules/$rel \
    -f ../fw/tb323fu-firmware -a ../tb323fu-config/android-boot.sha256 \
    -l initramfs.list initramfs.cpio.gz
# set CONFIG_INITRAMFS_SOURCE in out/.config to the cpio (see kernel/README.md), then:
../tb323fu-linux/tools/build-boot.sh -k . -o out -s ../stock-boot.img -i initramfs.list ../linux-boot.img
```

Notes:

- Use `kernel/config/baldur.fragment` (the copy in this repository), not the one patch 0055 adds — the patch's copy
  names the original build machine's initramfs path.
- `build-boot.sh` keeps the stock image's header, signature blob and AVB footer layout and only replaces the kernel
  (it calls `tools/boot-repack-kernel.py`). An image packed with plain `mkbootimg` is rejected by the bootloader.
- **Speakers:** the aw882xx amplifier driver is an out-of-tree module
  ([`kernel/out-of-tree/aw882xx`](../kernel/out-of-tree/aw882xx/)); without it the build boots and everything else
  works, but the speakers stay silent. `modules_install` with `M=` puts it in `extra/` and runs `depmod`. **[untested]**
  as written here (the development builds use the same `make … M=… CONFIG_SND_SOC_AW882XX=m` line).
- With `-m` the boot image carries `mods/lib/modules/<release>` itself; the roots need no copy. Keep the tree only for
  a root in `own` mode. (Without `-m` you get the old layout: every root needs its own copy of exactly these modules.)
- Once Linux runs with the helper, install later builds of your own with `tb323fu-ctl kernel install-local` instead
  of repeating step 6: the same trial and automatic rollback as an official update — see
  [Building and installing your own kernel](custom-kernel.md).

## 4. Partition a microSD card

> **Warning: this erases the whole card.** Nothing on the tablet changes.

The card gets a GPT with one ext4 partition per root. Partition **names** matter (the initramfs looks them up):

| Name | Role |
|---|---|
| `baldur-root` | the default root when present. Normally on the internal storage (step 8) |
| `baldur-root-sd` | fallback root, tried after `baldur-root` |
| `tb323fu-<anything>` | further roots, chosen with `tb323fu-ctl boot …` or the boot menu |

No name is required: the selection files (`/etc/tb323fu/boot-*`) live on the **state root**, the first present of
`baldur-root`, `baldur-root-sd`, then the `tb323fu-*` partitions in sorted order, and it is also the default and
the first fallback. A card with only `tb323fu-arch` and `tb323fu-ubuntu` therefore boots `tb323fu-arch` unless
told otherwise [untested on the device]. Once `baldur-root` exists on the internal storage, it takes over that
role — so set the default again after step 8.

### From a Linux PC with a card reader [untested]

```sh
lsblk -o NAME,SIZE,MODEL,TRAN          # find the card, e.g. /dev/sdX — double-check
sudo sgdisk --zap-all /dev/sdX
sudo sgdisk -n 1:0:+64G -t 1:8300 -c 1:baldur-root \
            -n 2:0:+64G -t 2:8300 -c 2:tb323fu-ubuntu /dev/sdX
sudo mkfs.ext4 -L baldur-root /dev/sdX1
sudo mkfs.ext4 -L tb323fu-ubuntu /dev/sdX2
```

Ext4 labels equal to the GPT names keep things readable; the initramfs only uses the GPT name.

### From rooted Android, GPT built on a PC [verified]

Android's own `sgdisk` can only zap a table, so the development card got its GPT like this: build the table on a PC
in a sparse file of the card's exact size, then write its first 34 and last 33 sectors from Android. In Android the
card is `mmcblk1` (in mainline Linux it is `mmcblk0`).

```sh
# on the tablet: card size in 512-byte sectors
adb shell cat /sys/block/mmcblk1/size                                   # → N
# on the PC (sgdisk from gdisk: Linux, Homebrew on macOS, gdisk for Windows)
truncate -s $((N*512)) sd.img
sgdisk -n 1:0:+64G -t 1:8300 -c 1:baldur-root sd.img
dd if=sd.img of=gpt-head.bin bs=512 count=34
dd if=sd.img of=gpt-tail.bin bs=512 skip=$((N-33)) count=33
adb push gpt-head.bin gpt-tail.bin /data/local/tmp/
# on the tablet, as root (adb shell, then su); replace N
sm unmount public:179,2            # if Android mounted the card (the volume id may differ: sm list-volumes)
sgdisk --zap-all /dev/block/mmcblk1
dd if=/data/local/tmp/gpt-head.bin of=/dev/block/mmcblk1 bs=512 conv=fsync
dd if=/data/local/tmp/gpt-tail.bin of=/dev/block/mmcblk1 bs=512 seek=$((N-33)) conv=fsync
blockdev --rereadpt /dev/block/mmcblk1
```

The development card then received a ready ext4 image with `dd` (step 5). Android reports the card as unsupported
afterwards; that is expected.

Later partitions on the development card (`tb323fu-ubuntu`, `-arch`, `-nixos`, `-fedora`, `-spare`) were added
from Linux on the tablet with `sgdisk` and `mkfs.ext4` [from records].

## 5. Put a root filesystem on it

The builders in [`rootfs/`](../rootfs/) install a distribution into a **mounted, empty ext4 partition**. What every
root needs (modules, firmware, masked services) is listed in [distros.md](distros.md#what-every-root-needs); the
builders do all of it. Status per distribution is in [distros.md](distros.md).

On the development tablet every builder ran **on the tablet itself, under Linux** [verified]. Running them on a
separate arm64 machine with the card in a reader is [untested]. Point them at the files from steps 1–3, because their
defaults (`/lib/firmware`, `/etc/tb323fu`) are the build host's own. No kernel modules: the boot image brings them
(`MODULES_FROM` only for a root in `own` mode, see 3a):

```sh
sudo mount /dev/sdX2 /mnt/t
sudo env ROOT_PARTLABEL=tb323fu-ubuntu \
    FIRMWARE_FROM="$PWD/fw/tb323fu-firmware/lib/firmware" \
    CONFIG_FROM="$PWD/tb323fu-config" \
    DESKTOP=gnome DEV_USER=<your user> DEBS_FROM="$PWD/debs" \
    sh tb323fu-linux/rootfs/ubuntu/build-rootfs.sh /mnt/t
sudo umount /mnt/t
```

- **Platform packages for Ubuntu:** download the `tb323fu-*.deb` files and `SHA256SUMS` of the newest
  `helper-v…` release from the repository's [Releases](https://github.com/joonhoekim/tb323fu-linux/releases) into
  `debs/`, check them (`cd debs && sha256sum -c SHA256SUMS`) and pass the directory as `DEBS_FROM=` (in WSL2, copy
  them from your Windows Downloads folder under `/mnt/c`). [`packaging/debian/build-debs.sh`](../packaging/) builds the same
  packages on an arm64 machine.
- Each builder lists its options at the top (Ubuntu: `DEBS_FROM`; Arch: `PKGS_FROM`; Fedora and SteamOS build or copy the platform
  files themselves; NixOS uses `flake.nix`). Without the platform packages the root boots, but audio routing, sensors,
  the emergency key and the helper are missing.
- `ROOT_PARTLABEL` must equal the partition's GPT name: it is written into `/etc/fstab` as `PARTLABEL=…`.
- Without the builders' dev-only password option (see the top of each builder) root is locked and the user has no password; with GNOME and `DEV_USER` the user is
  logged in automatically. Set a password on first boot.
- `DEV_ACCESS=1` adds the developer way in (USB network `192.168.7.2`, root shell on the USB serial port, SSH root
  login). Useful for a first install; turn it off for daily use.

**Without a card reader on the build host** (for example WSL2, which cannot see the tablet's card): build the root
into an image file (`truncate -s 24G root.img && mkfs.ext4 -L tb323fu-ubuntu root.img && sudo mount -o loop root.img
/mnt/t`, then the builder [verified in WSL2 with qemu]), and write the image into the partition — from a PC with a card reader, or from rooted Android the way the
development card got its first root [verified for a Debian image]:

```sh
gzip -c root.img > root.img.gz && adb push root.img.gz /sdcard/
adb shell su -c 'zcat /sdcard/root.img.gz | dd of=/dev/block/mmcblk1p1 bs=4194304'
```

Then grow the filesystem to the partition once Linux runs (`resize2fs /dev/mmcblk0p1`).

## 6. Write `boot_a` and boot Linux

**[verified]** Both ways go through Android and verify hashes before rebooting.

**KernelSU module** (no PC needed after staging):

```sh
android/install-module.sh stage linux-boot.img
```

then KernelSU manager → Modules → **Switch to Linux** → Action. It writes the staged image to `boot_a`, reads it back
and reboots; on a bad write it restores Android from `boot_b` first.

**From the PC** with [`tools/cycle.sh`](../tools/cycle.sh):

```sh
ANDROID_BOOT_SHA256=<sha256 of boot_b> tools/cycle.sh linux-boot.img
```

Back to Android at any time: `back-to-android <hash>` as root in Linux, the **Android** tile / "Switch to Android" in
the desktop, or **volume up + volume down held for 10 s** (works with a frozen desktop while the kernel runs).

## 7. First boot checks

**[verified]** What a good first boot looks like:

1. The bootloader logo, then the initramfs **boot summary** on the panel (kernel, CPUs, block devices, battery, …).
   Holding **volume up** during the summary keeps you in the initramfs, with a shell on the USB serial port.
2. `switching to root <name> on /dev/… (default)`, then the distribution starts. A root that is missing, does not
   mount or has no init is skipped, and the screen says why.
3. The desktop (or a login prompt).

Then check from a terminal on the tablet (or over SSH/serial with `DEV_ACCESS=1`):

```sh
uname -r                                   # the release you built
findmnt /lib/modules/$(uname -r)           # squashfs on /dev/loop0: the boot image's own modules
systemctl is-system-running                # running (or degraded: systemctl --failed)
cat /etc/tb323fu/android-boot.sha256       # your boot_b hash
tb323fu-ctl boot list                      # the roots the initramfs can see (with the helper installed)
```

Then test the way back once, while you are still at the desk: `back-to-android <hash>` or the 10-second key chord.
Android must come up; return to Linux with the **Switch to Linux** module.

If Linux does not come up: [Recovery → Linux does not boot](recovery.md#linux-does-not-boot).

## 8. Optional: a Linux root on the internal storage

**[from records]**

> **Warning: this wipes Android's data (a factory reset).** Apps, accounts and files in Android's internal storage are
> gone. `boot_a`, `boot_b`, root in `init_boot` and the bootloader are not affected. Back up what you need from
> Android first.

The internal storage is much faster than a card (2.3 GB/s write, 3.4 GB/s read vs. 85 MB/s on the development card).
Android's `userdata` is the last partition of UFS LUN 0, f2fs (cannot be shrunk) and encrypted, so making room means
shrinking the partition and letting Android format it again.

On the development tablet: `userdata` was cut to 128 GiB (same start, same type and unique GUID), a ~322 GiB
partition `baldur-root` (type 8300, 1 MiB aligned) added after it, `userdata` and `metadata` cleared the way
`fastboot -w` does, `baldur-root` formatted ext4 and filled from the running SD root with `rsync`. Android formatted
both partitions on its next boot and started at the setup wizard. The commands below reconstruct that from the notes;
**they were not re-run in this form.** Work from Linux booted from the SD card; Android's `sgdisk` cannot edit the table.

```sh
# 1. find LUN 0 and userdata by name, never by number
for u in /sys/class/block/sd*/uevent; do grep -q '^PARTNAME=userdata$' $u && grep -E '^(DEVNAME|PARTN)=' $u; done
#    e.g. DEVNAME=sda16 PARTN=16  → disk /dev/sda, partition 16 (yours may differ)
D=/dev/sda; P=16
sgdisk -p $D                          # 4096-byte sectors; note the table's entry count (32 on the dev unit)
sgdisk -i $P $D                       # note: first sector, type GUID, unique GUID, attribute flags, name

# 2. back up the table and copy the backup off the tablet
sgdisk -b lun0-gpt.bak $D

# 3. recreate userdata smaller (same start, type, GUID, name), add baldur-root after it
S=<first sector of userdata>; T=<type GUID>; G=<unique GUID>
sgdisk -d $P -n $P:$S:+128G -t $P:$T -u $P:$G -c $P:userdata $D
#    if `sgdisk -i` showed non-zero attribute flags, set them again (sgdisk -A $P:set:<bit> $D)
R=$((P+1))                            # must be a free entry (it was on the dev unit)
sgdisk -n $R:0:0 -t $R:8300 -c $R:baldur-root $D
partprobe $D; sgdisk -p $D            # check before going on

# 4. clear userdata and metadata (this is the factory reset)
U=/dev/disk/by-partlabel/userdata; M=/dev/disk/by-partlabel/metadata
blkdiscard $U
dd if=/dev/zero of=$U bs=1M count=64 conv=fsync
dd if=/dev/zero of=$U bs=1M count=64 seek=$(( $(blockdev --getsize64 $U) / 1048576 - 64 )) conv=fsync
dd if=/dev/zero of=$M bs=1M conv=fsync      # metadata is small (64 MiB on the dev unit); "No space left" at the end is expected

# 5. the new root
mkfs.ext4 -L baldur-root /dev/disk/by-partlabel/baldur-root
mount /dev/disk/by-partlabel/baldur-root /mnt
rsync -aHAXx / /mnt/                  # copy the running SD root
sed -i 's/PARTLABEL=[^ \t]*/PARTLABEL=baldur-root/' /mnt/etc/fstab
```

If the SD partition was called `baldur-root`, rename it so the internal one wins clearly:
`sgdisk -c 1:baldur-root-sd /dev/mmcblk0` and `e2label /dev/mmcblk0p1 baldur-root-sd` (and its fstab). The initramfs
searches `sd*` (UFS) before `mmcblk*` anyway.

Reboot. Linux now starts from `baldur-root` on the internal storage.

**Android after the reset** [verified on the development tablet]: it formats `userdata` and `metadata` and starts at
the setup wizard; root in `init_boot` stays but `/data/adb` (KernelSU's userspace, allow list and modules) is gone.
Then:

1. Enable USB debugging, reinstall the KernelSU manager APK of the same version with `adb install`, allow **Shell**.
2. Disable the OTA apps again ([rooting.md, step 3](rooting.md#3-root-on-android-kernelsu)).
3. `android/install-module.sh install` again (the module lived in `/data/adb`; `boot_b` is unchanged, so the hash
   stays the same).

From Linux, [`tools/flash-boot.sh`](../tools/flash-boot.sh) writes new boot images directly (over SSH), so the
round trip through Android is no longer needed for kernel updates.

## Multiboot

Several roots can live side by side, one per partition named `tb323fu-*` (on the card or anywhere else). They all
share the one kernel in `boot_a` and its modules: the initramfs mounts the boot image's modules on
`/lib/modules/<release>` of whichever root it starts, so a new kernel needs nothing installed in any root (NixOS
included: its system holds a kernel stub). A root can opt out with `own` in `/etc/tb323fu/modules`; it then needs its
own copy of the modules of every kernel it boots.

Choosing a root [verified]:

```sh
tb323fu-ctl boot list
tb323fu-ctl boot next tb323fu-arch       # once; falls back to the default if it fails to start
tb323fu-ctl boot default tb323fu-ubuntu  # persistent
tb323fu-ctl boot reboot tb323fu-nixos    # set next and reboot
```

or the **Systems** page of Open Device Helper, or an on-screen menu at boot (create `/etc/tb323fu/boot-menu` on
the state root, normally `baldur-root`; volume up = next, 5 s idle = boot). See [helper.md](helper.md#multiboot) and
[kernel/initramfs/README.md](../kernel/initramfs/README.md#root-partitions-and-multiboot).

## Undo

| To undo | How | Status |
|---|---|---|
| Linux in `boot_a` | `back-to-android <hash>`, the 10 s key chord, or EDL → LTBox Flash Partitions → `boot_a` ← your Android boot image ([recovery](recovery.md#linux-does-not-boot)) | [verified] |
| a root on the card | delete the partition (`sgdisk -d`) or reformat the card in Android | [untested] |
| the Linux root on internal storage | write the saved table back (`sgdisk -l lun0-gpt.bak $D`) and factory-reset Android again so `userdata` is formatted at full size (Android: Settings → Reset, or LTBox Flash Firmware → Wipe Data) | [untested] |
| `boot_b` | not needed: it is never booted. A stock copy is also in your dump and in the firmware package (same version) | — |
| the root itself | [recovery → Undoing the root](recovery.md#undoing-the-root) | [from LTBox's source] |

Never restore a partition table backup taken before a firmware update over a newer layout, and never restore another
tablet's table: the backup contains your device's unique GUIDs.
