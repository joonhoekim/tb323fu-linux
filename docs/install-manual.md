# Installing by hand

The steps from a rooted TB323FU (Android with KernelSU, see [Rooting](rooting.md)) to mainline Linux booting from its
own partition, with Android kept as the way back. It covers what the guided script does not: other distributions, a
kernel you build yourself, a card reader on the PC and a root on the internal storage. Read it once before starting.

> **For a first install, use the guide for your PC instead:** [Installing Linux](install.md) picks the page
> (Windows, Linux or macOS); they run [`tools/install/install.sh`](../tools/install/README.md), which does steps 1–7
> below for a distribution on the microSD card. Its status per PC is on [Installing Linux](install.md#your-pc).

> **Releases** are on [GitHub Releases](https://github.com/joonhoekim/tb323fu-linux/releases). A kernel release is a
> kernel `Image` (modules inside) and a repack tool, never a ready-made `boot.img` ([why](#why-there-is-no-ready-made-bootimg)).

Each step carries one evidence label in brackets: **[verified]**[^verified-runs] or **[untested]**
([what they mean](hardware-status.md#evidence-labels)).

## What you end up with

| Where | Partition | Holds |
|---|---|---|
| UFS, LUN 4 | `boot_a` | the Linux boot image |
| | `boot_b` | a copy of your Android boot image (the way back) |
| | `init_boot_a` | KernelSU (from rooting) |
| UFS, LUN 0 | `userdata` | Android's data; shrunk only for a root on the internal storage ([step 8](#8-optional-a-linux-root-on-the-internal-storage)) |
| | `baldur-root` | optional (step 8): ext4, the default Linux root when present |
| microSD card (optional) | `tb323fu-ubuntu`, `tb323fu-arch`, `tb323fu-nixos`, … | ext4, one root per system (multiboot) |
| | or `baldur-root-sd` / `baldur-root` | ext4, a single root |

The bootloader always boots slot `_a`; switching copies Android's or the Linux boot image into `boot_a`. The Linux
image carries its own device tree, command line and initramfs, and the initramfs picks a root **by GPT partition
name** ([details](../kernel/initramfs/README.md#root-partitions-and-multiboot)).

## Overview

| Step | What changes | Reversible? | Status |
|---|---|---|---|
| 0. Rooting and backup ([rooting.md](rooting.md)) | `efisp`, `init_boot_a` | yes ([recovery](recovery.md#undoing-the-root)) | [verified] |
| 1. Extract firmware | nothing on the tablet (a copy in `/data/local/tmp`) | — | [verified] |
| 2. The way back: Android boot image into `boot_b` | `boot_b` | yes (`boot_b` is never booted) | [verified] |
| 3. The Linux boot image: repack a release kernel, or build one | nothing (PC) | — | repack [verified], build [verified] |
| 4. Partition a microSD card | **the card is wiped** | the card only | from Android [verified], from a PC [untested] |
| 5. Root filesystem | the new partition | yes (delete it) | [verified][^verified-rootfs] |
| 6. Write `boot_a` and boot Linux | `boot_a` | yes: back-to-android, emergency keys, EDL | [verified] |
| 7. First-boot checks | nothing | — | [verified] |
| 8. Optional: Linux root on internal storage | GPT of LUN 0, **Android's data is wiped** | only with another factory reset | [untested][^steps-8] |
| 8b. Optional: one more partition next to it | GPT of LUN 0, the existing root is shrunk | yes (grow it back) | [verified] |

Steps 1–2 happen in rooted Android, 3 on a PC, 4–5 on a PC or in Android, 6 from Android. Step 8 is the only step that
costs Android its data.

## Prerequisites

- Steps 1–3 of [rooting.md](rooting.md) done: a **full EDL dump of your own tablet stored off the PC**, LTBox's
  `init_boot` backup, KernelSU with **Shell** allowed, OTA apps disabled.
- `adb` on the PC ([platform-tools](https://developer.android.com/tools/releases/platform-tools)).
- A **Linux PC** to build the kernel (x86-64 is fine, it cross-builds with `LLVM=1`). On Windows or macOS use WSL2 or a
  VM ([Windows](install-windows.md), [macOS](install-macos.md)) **[untested]**.
- A **Linux host that runs arm64 programs** for the root filesystem builders in [`rootfs/`](../rootfs/): an arm64
  machine (the tablet itself works) or x86-64 with `qemu-user-binfmt`.
- A **microSD card**, 64 GB or more (each root takes 10–30 GB), battery above 50 %, a USB-C data cable.

<details>
<summary>Windows: WSL2 with Ubuntu as the arm64 host [verified]</summary>

```sh
sudo apt install qemu-user-binfmt debootstrap libarchive-tools gpg curl e2fsprogs
```

An Ubuntu root with GNOME built this way booted on the tablet.
</details>

## 1. Extract the firmware

**[verified]** The firmware is not distributed; it is copied from your own Android `/vendor`
([`firmware/README.md`](../firmware/README.md) has the manifest).

```sh
adb push firmware/manifest.tsv firmware/extract-on-device.sh /data/local/tmp/
adb shell su -c 'sh /data/local/tmp/extract-on-device.sh /data/local/tmp/tb323fu-firmware'
# copy it to the PC (tar keeps the layout; toybox tar is enough)
adb shell su -c 'tar -C /data/local/tmp -cf /data/local/tmp/tb323fu-firmware.tar tb323fu-firmware'
adb pull /data/local/tmp/tb323fu-firmware.tar
mkdir -p fw && tar -C fw -xf tb323fu-firmware.tar     # → fw/tb323fu-firmware/lib/firmware/...
# the audio topology is built in this repository, not taken from Android (firmware/audio/README.md)
cp firmware/audio/LENOVO-TB323FU-tplg.bin fw/tb323fu-firmware/lib/firmware/qcom/kaanapali/
```

A hash mismatch is a warning, a missing file an error. A mismatch means another Android firmware version ([Updating Android firmware](firmware-updates.md)). Without the topology the root has no sound card.

## 2. The way back: Android's boot image in `boot_b`

**[verified]** The guided script's `wayback` step does the same:

```sh
android/install-module.sh prepare-boot-b      # copies the running Android boot image (boot_a) into boot_b; asks first
android/install-module.sh install             # refuses unless boot_a == boot_b; installs the "Switch to Linux" module
android/install-module.sh status              # prints the hashes
```

Keep the printed sha256 of `boot_b`. It goes into the boot image and into every root as
`/etc/tb323fu/android-boot.sha256`, and `tools/cycle.sh` takes it as `ANDROID_BOOT_SHA256`.[^stale-hash] Put it in a
small directory the builders read:

```sh
mkdir -p tb323fu-config
echo <sha256 of boot_b> > tb323fu-config/android-boot.sha256
```

Also get your **stock boot image** for step 3 (the same image as `boot_b`) and keep it with your backup:

```sh
adb shell su -c 'dd if=/dev/block/by-name/boot_b of=/data/local/tmp/stock-boot.img'
adb pull /data/local/tmp/stock-boot.img
sha256sum stock-boot.img        # must equal the hash above
```

## 3. The Linux boot image

A Linux boot image is **your stock `boot.img` with only the kernel replaced**; the bootloader (with the GBL from
rooting) accepts it.[^boot-image]

### Why there is no ready-made `boot.img`

It would carry Lenovo's GKI signature and vbmeta blobs, which are not ours to redistribute and must match the firmware
version and region of the tablet they are written to. A release ships the kernel `Image`, the modules and the repack
tool; you make the image from your own stock `boot.img`. Only ever use your own tablet's stock image.

### 3a. From a release

**[verified]** The guided script's `bootimg` step runs exactly this. A release (tag `kernel-tNN`) has
`Image-tb323fu-tNN` (also as `.gz`), `boot-repack-kernel.py`, `SHA256SUMS`, GPL sources lists and
`modules-tb323fu-tNN.tar.gz` (normally not needed). `SHA256SUMS` catches a damaged download; it is not a signature:

```sh
sha256sum -c SHA256SUMS
python3 boot-repack-kernel.py stock-boot.img Image-tb323fu-tNN linux-boot.img
```

The modules come with the kernel: the `Image` carries them as a squashfs in its initramfs, mounted on
`/lib/modules/<release>` of whichever root boots, so a kernel update is only a new boot image **[verified]**
([details](../kernel/initramfs/README.md#kernel-modules-shared)). A root can opt out with `own` in
`/etc/tb323fu/modules` (e.g. for DKMS) and then uses the modules tarball. LZ4 kernels are refused. The release
initramfs has **no firmware** (it cannot be redistributed), so every root needs the firmware from step 1.
`tools/install/install.sh boot` runs the repack for you (`KERNEL_IMAGE=…`).

### 3b. Build it yourself

**[verified]**[^built-3b] Follows [`kernel/README.md`](../kernel/README.md), [`kernel/initramfs/README.md`](../kernel/initramfs/README.md)
and [`tools/build-boot.sh`](../tools/build-boot.sh). On Debian or Ubuntu:

```sh
sudo apt install clang lld llvm make flex bison bc libssl-dev libelf-dev rsync cpio kmod squashfs-tools \
    python3 git gcc-aarch64-linux-gnu console-setup-linux
# a static arm64 busybox and regulatory.db, unpacked from the arm64 packages (works on an x86-64 PC)
apt-get download busybox-static:arm64 wireless-regdb    # add the arm64 architecture or fetch the .deb from ports
dpkg-deb -x busybox-static_*_arm64.deb bb && dpkg-deb -x wireless-regdb_*.deb regdb
# busybox: bb/usr/bin/busybox    regulatory.db: regdb/lib/firmware/
```

```sh
# kernel tree: upstream base + this series
git clone --depth 1 -b v7.3 https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git linux-tb323fu
cd linux-tb323fu
git am ../tb323fu-linux/kernel/patches/*.patch
make ARCH=arm64 LLVM=1 O=out defconfig
scripts/kconfig/merge_config.sh -m -O out out/.config \
    arch/arm64/configs/kaanapali-oneplus-infiniti_defconfig ../tb323fu-linux/kernel/config/baldur.fragment \
    ../tb323fu-linux/kernel/config/baldur-display.fragment ../tb323fu-linux/kernel/config/baldur-netfilter.fragment \
    ../tb323fu-linux/kernel/config/baldur-extra.fragment
make ARCH=arm64 LLVM=1 O=out olddefconfig
# a release name of your own, unique per build (old and new modules never collide)
scripts/config --file out/.config --set-str LOCALVERSION -tb323fu-mybuild1
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= olddefconfig
# LOCALVERSION= on every make: otherwise the release gets a "+" and the modules are not found
# Image too: modules_install needs the modules.builtin the first link writes
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= -j"$(nproc)" Image dtbs modules headers_install
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= INSTALL_MOD_PATH="$PWD/mods" INSTALL_MOD_STRIP=1 modules_install
# the speaker amplifier driver (out of tree) into mods/lib/modules/<release>/extra/
make ARCH=arm64 LLVM=1 O=out LOCALVERSION= M="$PWD/../tb323fu-linux/kernel/out-of-tree/aw882xx" \
    CONFIG_SND_SOC_AW882XX=m INSTALL_MOD_PATH="$PWD/mods" INSTALL_MOD_DIR=extra INSTALL_MOD_STRIP=1 modules modules_install
rel=$(make -s ARCH=arm64 LLVM=1 O=out LOCALVERSION= kernelrelease)
rm -f mods/lib/modules/$rel/build mods/lib/modules/$rel/source; depmod -b mods $rel

# initramfs: all modules (-m), regulatory.db (-r); optional: firmware from step 1 (-f) and the
# boot_b hash from step 2 (-a; sha256sum ../stock-boot.img | cut -c1-64 > android-boot.sha256)
../tb323fu-linux/kernel/initramfs/build.sh -k out -b ../bb/usr/bin/busybox -r ../regdb/lib/firmware \
    -m mods/lib/modules/$rel -f ../fw/tb323fu-firmware -a ../tb323fu-config/android-boot.sha256 \
    -l initramfs.list initramfs.cpio.gz
scripts/config --file out/.config --set-str INITRAMFS_SOURCE "$PWD/initramfs.cpio.gz"
../tb323fu-linux/tools/build-boot.sh -k . -o out -s ../stock-boot.img -i initramfs.list ../linux-boot.img
```

- Use `kernel/config/baldur.fragment` from this repository, not the copy patch 0055 adds (it names the original build
  machine's initramfs path).
- `build-boot.sh` only replaces the kernel in the stock image; a plain `mkbootimg` image is rejected by the bootloader.
- **Speakers:** without the out-of-tree [`aw882xx`](../kernel/out-of-tree/aw882xx/) amplifier driver everything else
  works but the speakers stay silent. **[untested]** as written here.[^aw882xx-line]
- With `-m` the boot image carries the modules; keep `mods/` only for a root in `own` mode.
- Later builds: `tb323fu-ctl kernel install-local` instead of repeating step 6, with the same trial and automatic
  rollback as an official update ([custom-kernel.md](custom-kernel.md)).

## 4. Partition a microSD card

> **Warning: this erases the whole card.** Nothing on the tablet changes.

The card gets a GPT with one ext4 partition per root. Partition **names** matter (the initramfs looks them up):

| Name | Role |
|---|---|
| `baldur-root` | the default root when present. Normally on the internal storage (step 8) |
| `baldur-root-sd` | fallback root, tried after `baldur-root` |
| `tb323fu-<anything>` | further roots, chosen with `tb323fu-ctl boot …` or the boot menu |

No name is required. The selection files (`/etc/tb323fu/boot-*`) live on the **state root**: the first present of
`baldur-root`, `baldur-root-sd`, then the `tb323fu-*` partitions in sorted order, which is also the default and first
fallback (so a card with only `tb323fu-arch` and `tb323fu-ubuntu` boots `tb323fu-arch` **[untested]**). Once
`baldur-root` exists on the internal storage it takes over, so set the default again after step 8.

### From rooted Android, GPT built on a PC **[verified]**

Android's `sgdisk` can only zap a table, so build the table on a PC in a sparse file of the card's size and write its
first 34 and last 33 sectors from Android. The card is `mmcblk1` in Android (`mmcblk0` in mainline Linux).

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

Then write a ready ext4 image with `dd` (step 5). Android reports the card as unsupported afterwards; that is expected.
Further partitions go into the free space later, from Linux on the tablet, without touching the others **[verified]**:

```sh
D=/dev/mmcblk0                                  # the card (lsblk); N = the next free partition number
sgdisk -n N:0:+64G -t N:8300 -c N:tb323fu-arch $D
partx -a -n N $D                                # the kernel sees it while other partitions are mounted
mkfs.ext4 -L tb323fu-arch /dev/disk/by-partlabel/tb323fu-arch
```

<details>
<summary>From a Linux PC with a card reader [untested]</summary>

```sh
lsblk -o NAME,SIZE,MODEL,TRAN          # find the card, e.g. /dev/sdX — double-check
sudo sgdisk --zap-all /dev/sdX
sudo sgdisk -n 1:0:+64G -t 1:8300 -c 1:baldur-root \
            -n 2:0:+64G -t 2:8300 -c 2:tb323fu-ubuntu /dev/sdX
sudo mkfs.ext4 -L baldur-root /dev/sdX1
sudo mkfs.ext4 -L tb323fu-ubuntu /dev/sdX2
```

Ext4 labels equal to the GPT names keep things readable; the initramfs only uses the GPT name.
</details>

## 5. Put a root filesystem on it

The builders in [`rootfs/`](../rootfs/) install a distribution into a **mounted, empty ext4 partition** and do
everything a root needs ([distros.md](distros.md#what-every-root-needs) lists it, with the status per distribution).
Running them on the tablet itself is **[verified]**; on a separate arm64 machine with the card in a reader it is
**[untested]**. Point them at the files from steps 1–3, because their defaults (`/lib/firmware`, `/etc/tb323fu`) are the
build host's own:

```sh
sudo mount /dev/sdX2 /mnt/t
sudo env ROOT_PARTLABEL=tb323fu-ubuntu \
    FIRMWARE_FROM="$PWD/fw/tb323fu-firmware/lib/firmware" \
    CONFIG_FROM="$PWD/tb323fu-config" \
    DESKTOP=gnome DEV_USER=<your user> DEBS_FROM="$PWD/debs" \
    sh tb323fu-linux/rootfs/ubuntu/build-rootfs.sh /mnt/t
sudo umount /mnt/t
```

- **Platform packages (Ubuntu):** download the `tb323fu-*.deb` files and `SHA256SUMS` of the newest `helper-v…`
  [release](https://github.com/joonhoekim/tb323fu-linux/releases) into `debs/`, check them
  (`cd debs && sha256sum -c SHA256SUMS`) and pass the directory as `DEBS_FROM=`.
  [`packaging/debian/build-debs.sh`](../packaging/) builds the same packages on an arm64 machine. Without them the
  root boots, but audio routing, sensors, the emergency key and the helper are missing.
- Other builders list their options at the top (Arch: `DEBS_FROM` unpacked or `PKGS_FROM`; Fedora and SteamOS handle
  the platform files themselves; NixOS uses `flake.nix`).
- `ROOT_PARTLABEL` must equal the partition's GPT name; it goes into `/etc/fstab` as `PARTLABEL=…`.
- Without the builders' dev-only password option, root is locked and the user has no password (with GNOME and
  `DEV_USER` the user is logged in automatically). Set a password on first boot.
- `DEV_ACCESS=1` adds the developer way in: USB network `192.168.7.2`, a root shell on the USB serial port and SSH
  root login (needs a key: the builder copies the host's `/root/.ssh/authorized_keys` if present, otherwise add one
  from the serial port). Turn it off for daily use.

<details>
<summary>Without a card reader on the build host (WSL2 and similar) [verified]</summary>

Build the root into an image file (`truncate -s 24G root.img && mkfs.ext4 -L tb323fu-ubuntu root.img && sudo mount -o loop root.img
/mnt/t`, then the builder; in WSL2 copy the `.deb` files from your Windows Downloads folder under `/mnt/c`). Write
the image into the partition from a PC with a card reader, or from rooted Android (verified for a Debian image):

```sh
gzip -c root.img > root.img.gz && adb push root.img.gz /sdcard/
adb shell su -c 'zcat /sdcard/root.img.gz | dd of=/dev/block/mmcblk1p1 bs=4194304'
```

Then grow the filesystem to the partition once Linux runs (`resize2fs /dev/mmcblk0p1`).
</details>

## 6. Write `boot_a` and boot Linux

**[verified]** Both ways go through Android and verify hashes before rebooting.

**KernelSU module** (no PC needed after staging):

```sh
android/install-module.sh stage linux-boot.img
```

then KernelSU manager → Modules → **Switch to Linux** → Action. It writes `boot_a`, reads it back and reboots; on a
bad write it restores Android from `boot_b` first.

**From the PC** with [`tools/cycle.sh`](../tools/cycle.sh):

```sh
ANDROID_BOOT_SHA256=<sha256 of boot_b> tools/cycle.sh linux-boot.img
```

Back to Android at any time: `back-to-android <hash>` as root in Linux, the **Android** tile / "Switch to Android" in
the desktop, or **volume up + volume down held for 10 s** (works with a frozen desktop while the kernel runs).

## 7. First boot checks

**[verified]** A good first boot: the bootloader logo, then the initramfs **boot summary** on the panel (holding
**volume up** keeps you in the initramfs with a shell on the USB serial port), then
`switching to root <name> on /dev/… (default)` and the desktop or a login prompt. A root that is missing, does not mount
or has no init is skipped, and the screen says why.

Check from a terminal on the tablet (or over SSH/serial with `DEV_ACCESS=1`):

```sh
uname -r                                   # the release you built
findmnt /lib/modules/$(uname -r)           # squashfs on /dev/loop0: the boot image's own modules
systemctl is-system-running                # running (or degraded: systemctl --failed)
cat /etc/tb323fu/android-boot.sha256       # your boot_b hash
tb323fu-ctl boot list                      # the roots the initramfs can see (with the helper installed)
```

Then test the way back once, while you are at the desk: `back-to-android <hash>` or the 10-second key chord. Android
must come up; return with **Switch to Linux**.

If Linux does not come up: [Recovery → Linux does not boot](recovery.md#linux-does-not-boot). Switching, updates and
removing Linux: [After installing](after-install.md).

## 8. Optional: a Linux root on the internal storage

**[untested]**[^steps-8]

> **Warning: this wipes Android's data (a factory reset).** Apps, accounts and files in Android's internal storage are
> gone. `boot_a`, `boot_b`, root in `init_boot` and the bootloader are not affected. Back up what you need first.

The internal storage is much faster than a card.[^storage-speed] Android's `userdata` is the last partition of UFS
LUN 0, f2fs (cannot be shrunk) and encrypted, so you shrink it and let Android format it again. Work from Linux booted
from the SD card (Android's `sgdisk` cannot edit the table).[^layout-8]

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
`sgdisk -c 1:baldur-root-sd /dev/mmcblk0` and `e2label /dev/mmcblk0p1 baldur-root-sd` (and its fstab). Then reboot;
Linux starts from `baldur-root` on the internal storage.

**Android after the reset** **[verified]** starts at the setup wizard; root in `init_boot` stays but `/data/adb`
(KernelSU's userspace, allow list and modules) is gone. Then:

1. Enable USB debugging, reinstall the KernelSU manager APK of the same version with `adb install`, allow **Shell**.
2. Disable the OTA apps again ([rooting.md, step 3](rooting.md#3-root-on-android-kernelsu)).
3. `android/install-module.sh install` again (`boot_b` is unchanged, so the hash stays the same).

From Linux, [`tools/flash-boot.sh`](../tools/flash-boot.sh) writes new boot images directly over SSH, so kernel updates
no longer go through Android.

### 8b. One more partition on the internal storage

**[verified]** To try other systems, shrink the internal root and add a partition behind it; Android's data is not
touched. Work from Linux booted from the SD card (the root being shrunk must not be running), with `gdisk`,
`e2fsprogs` and `rsync` installed there. The internal storage has 4096-byte sectors, equal to the ext4 block, so a size
in blocks is also a size in sectors: 200 GiB = 52428800.

```sh
# 0. keep the tablet awake while this runs: GNOME suspends after 15 min idle, and a suspended
#    tablet drops off USB and the network (resize2fs simply continues after waking up)
systemctl mask --runtime sleep.target suspend.target

# 1. the partition and its file system; nothing on LUN 0 may be mounted
D=/dev/sda; P=17                                   # baldur-root (find it by name, as in 8)
sgdisk -i $P $D                                    # note first sector, unique GUID, name
sgdisk -b lun0-gpt.bak $D                          # copy this off the tablet

# 2. shrink the file system first, then the partition
e2fsck -fy /dev/sda$P                              # exit code 1 = errors fixed, fine
resize2fs -p -z /mnt/other/resize.e2undo /dev/sda$P 52428800   # undo file on another disk
e2fsck -fn /dev/sda$P                              # must be 0

# 3. the partition with the same start, unique GUID and name, ending after 52428800 sectors;
#    the new one behind it, to the end of the disk
S=<first sector>; G=<unique GUID>
sgdisk -d $P -n $P:$S:$((S + 52428800 - 1)) -t $P:8300 -u $P:$G -c $P:baldur-root \
       -n $((P + 1)):$((S + 52428800)):0 -t $((P + 1)):8300 -c $((P + 1)):tb323fu-ufs $D
partprobe $D; sgdisk -v $D; e2fsck -fn /dev/sda$P
```

`resize2fs` moves every block behind the new end, so keep the undo file on a fast disk; delete it once the shrunk root
has booted. Until the new partition is formatted, `tb323fu-ctl boot list` shows it as "could not be mounted
read-only". `baldur-root` stays the first root the initramfs finds and holds the boot choice; start the new
partition with `tb323fu-ctl boot next tb323fu-ufs` or make it the default ([Multiboot](#multiboot)).

## Multiboot

Roots live side by side, one per partition named `tb323fu-*` (on the card or anywhere else), and share the one kernel
in `boot_a` and its modules (NixOS included).[^multiboot-modules] Choose one **[verified]**:

```sh
tb323fu-ctl boot list
tb323fu-ctl boot next tb323fu-arch       # once; falls back to the default if it fails to start
tb323fu-ctl boot default tb323fu-ubuntu  # persistent
tb323fu-ctl boot reboot tb323fu-nixos    # set next and reboot
```

Or use the **Systems** page of Open Device Helper, or an on-screen menu at boot (create `/etc/tb323fu/boot-menu` on the
state root, normally `baldur-root`; volume up = next, 5 s idle = boot). See [helper.md](helper.md#multiboot) and
[kernel/initramfs/README.md](../kernel/initramfs/README.md#root-partitions-and-multiboot).

## Undo

Removing Linux and going back to the stock tablet, including a root on the internal storage:
[After installing → Removing Linux](after-install.md#removing-linux).

[^verified-runs]: Steps 1–7 in the form the guided script runs them were run on 2026-10-04 and 2026-10-29 ([Installing from Windows](install-windows.md)). The other steps were done on the development tablet. This page was reconstructed from its development records.
[^verified-rootfs]: On the tablet itself and on x86-64 PCs through qemu.
[^stale-hash]: If the Android boot image ever changes (an OTA, a different Android kernel), `boot_b` and the recorded hash are stale. The Linux tools then refuse to write (safe, but there is no way back from Linux) until you redo `prepare-boot-b` and `install` in Android and update the hash on the Linux side. `back-to-android`, the emergency key service and the helper all read the hash from `/etc/tb323fu/android-boot.sha256`.
[^boot-image]: The header, the GKI boot signature and the vbmeta blob with its AVB footer are copied from the stock image. The kernel carries its own device tree, command line and initramfs.
[^aw882xx-line]: The development builds use the same `make … M=… CONFIG_SND_SOC_AW882XX=m` line. `modules_install` with `M=` puts the driver in `extra/` and runs `depmod`.
[^storage-speed]: 2.3 GB/s write and 3.4 GB/s read, against 85 MB/s on the development card.
[^layout-8]: The layout used: `userdata` cut to 128 GiB, a ~322 GiB `baldur-root` behind it.
[^multiboot-modules]: The initramfs mounts the boot image's modules on `/lib/modules/<release>` of whichever root it starts, so a new kernel needs nothing installed in any root (NixOS holds a kernel stub). A root that opts out with `own` in `/etc/tb323fu/modules` needs its own copy of the modules of every kernel it boots.
[^steps-8]: The development tablet's layout was made this way; the commands below were written down afterwards and were not run again as written, because that wipes Android's data.
[^built-3b]: Followed as written on Ubuntu 26.04 (x86-64, WSL2) by someone new to it; the kernel it built started on the tablet through `tb323fu-ctl kernel install-local` with all modules, and `tb323fu-check` passed.
