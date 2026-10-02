# Initramfs

The kernel Image carries its initramfs built in (`CONFIG_INITRAMFS_SOURCE`, see [../README.md](../README.md)),
together with its device tree and a fixed command line (`CONFIG_CMDLINE_FORCE`). So the boot image in `boot_a`
is self-contained: the bootloader starts the kernel, the kernel runs [`init`](init) from this directory, and
`init` switches to a root filesystem on a partition you created.

| File | What |
|---|---|
| [`init`](init) | `/init`: USB way in, emergency chord, the modules image, boot summary, root selection (MIT) |
| [`build.sh`](build.sh) | assembles the cpio from this directory, a kernel build and its installed modules, a static busybox and your firmware |
| [`spec.list`](spec.list) | annotated layout: every file in the image, where it comes from, why it is there |
| [`gpu-probe.c`](gpu-probe.c) | optional helper for the summary (is the Adreno alive), GPL-2.0 |

`keyhold` (the volume up+down chord) is built from [`../../userspace/platform/src/keyhold.c`](../../userspace/platform/src/keyhold.c),
`back-to-android` comes from [`../../android/`](../../android/).

## Building

```sh
make O=out modules headers_install        # the kernel build (see ../README.md)
make O=out INSTALL_MOD_PATH=mods INSTALL_MOD_STRIP=1 modules_install   # + aw882xx into extra/, then depmod -b mods <release>
kernel/initramfs/build.sh -k out -b /path/to/busybox-static -m mods/lib/modules/<release> -f /path/to/firmware-root \
    -a android-boot.sha256 initramfs.cpio.gz
# then CONFIG_INITRAMFS_SOURCE="…/initramfs.cpio.gz" in out/.config and build the Image,
# or: -l initramfs.list and pass that to tools/build-boot.sh -i
```

Nothing third-party is stored here:

- **busybox**: a static aarch64 build, e.g. Debian's `busybox-static` (`/bin/busybox`). The tested one is BusyBox 1.36.1.
- **console font**: console-setup's `Lat15-Terminus28x14.psf.gz`.
- **firmware** (`-f`, optional): extracted from your own tablet with [`../../firmware/extract-on-device.sh`](../../firmware/);
  the Adreno, touch, Bluetooth and Wi-Fi drivers probe before a root is mounted, so with `-f` their firmware is
  in the initramfs as well as in the root filesystem. Without `-f` the image carries no vendor firmware (release
  images are built that way).
- **regulatory.db**: from wireless-regdb.
- **mksquashfs** (squashfs-tools) on the build machine, for `-m`.

## What `init` does

1. **USB way in**, first: a configfs gadget with a serial shell on `ttyGS0` and a network interface
   (`usb0`, the tablet is `192.168.7.2`, telnet). A boot that goes wrong later stays reachable from a PC.
2. **Emergency way back to Android**: volume up + down held for 10 s runs `back-to-android`, which copies the
   Android boot image from `boot_b` back into `boot_a` and reboots — only if `boot_b` matches the expected hash.
   The hash comes from the image (`/etc/android-boot.sha256`, `build.sh -a`) or, for images built without one
   (release images), from the root: `init` copies `/etc/tb323fu/android-boot.sha256` of the state root (below;
   if it does not mount, of the next root that does) while it reads the boot selection. With neither, the chord does nothing in the initramfs
   and the panel says so.
3. **Modules**: mounts the image's modules squashfs (below), then loads the remoteproc PAS driver (it attaches to
   the charger/Type-C firmware the bootloader already runs) and the touch driver from it, after the USB shell is up,
   so a bad attach still leaves a way in. The touch driver only when its firmware is in the image — otherwise the
   root's udev loads it.
4. **Boot summary** on the panel and in the kernel log (so it also lands in ramoops): kernel, command line,
   CPUs, thermal, block devices, SD card, USB, battery, DRM, then the latest kernel warnings.
5. **Root selection** (with `baldur.end=hold`, the normal command line), the modules mount moved into the chosen
   root, and `switch_root`.

### Kernel modules (shared)

The image carries **all** modules of its kernel: `build.sh -m` puts the installed tree (`make modules_install
INSTALL_MOD_STRIP=1`, the out-of-tree aw882xx driver in `extra/`, `depmod` run at build time, no `updates/`) into the
cpio as one squashfs, `/lib/modules/<release>.sqfs` (xz, about 6 MB for some 750 modules). `init` mounts it on
`/lib/modules/<release>` (loop device, read-only), loads the early modules from it by `modules.dep`, and before
`switch_root` moves the mount to `lib/modules/<release>` **inside the chosen root** — resolved there, so a merged-`/usr`
root gets it on `/usr/lib/modules/<release>` and NixOS (no `/lib`) on a new `/lib/modules/<release>`. After boot:

```
$ findmnt /lib/modules/$(uname -r)
TARGET                                 SOURCE     FSTYPE   OPTIONS
/usr/lib/modules/7.3.0-rc4-tb323fu-t28 /dev/loop0 squashfs ro,relatime,errors=continue
```

So every root runs exactly the modules of the kernel that booted, nothing is installed into the roots, and a kernel
update, a rollback or Android's Switch to Linux only move whole boot images. Every build has its own release name
(`CONFIG_LOCALVERSION="-tb323fu-<tag>"`), so old trees in the roots never collide. Background and decisions:
[`docs/notes/kernel-updates-design.md`](../../docs/notes/kernel-updates-design.md).

- **Read-only.** `depmod -a` fails harmlessly (the image has the complete depmod output); kmod, udev and
  `systemd-modules-load` only read. DKMS or a module replaced for a test need a root of its own: put `own` in that
  root's `/etc/tb323fu/modules` — `init` then leaves it alone and the root needs its own `/lib/modules/<release>`
  (the release's `modules-*.tar.gz`). For a one-off test without that: copy the tree to a tmpfs, `mount --bind` it over
  the mount, change it, `depmod` (gone at the next boot). `overlay` (an overlayfs with a per-root upper layer) is
  reserved and not implemented; such a root gets the shared tree.
- **A read-only root** (the mount point cannot be created) boots without the mount; the panel says so.
- **NixOS**: its kmod tries `/run/booted-system/kernel-modules/lib/modules/<release>` first and falls through to
  `/lib/modules/<release>` when that does not exist; the system holds a kernel stub
  ([`packaging/nix/prebuilt-kernel.nix`](../../packaging/nix/prebuilt-kernel.nix)), so it needs no rebuild per kernel.
- The image stays in the initramfs' memory (about 6 MB, not swappable).
- An image built without `-m` carries only the early modules, flat in `/lib/modules`; every root then needs its own
  `/lib/modules/<release>` (the layout before 2026-10-02).

### Without firmware (release images)

Measured on the tablet on 2026-10-02 with a firmware-free initramfs (same kernel otherwise), Debian root:

| Function | In the initramfs | After `switch_root` |
|---|---|---|
| Panel, console, boot summary | works (the display controller needs no firmware) | works |
| GPU (Adreno) | `gen80200_sqe.fw` not found | **recovers by itself**: the driver loads its firmware on the first open, from the root (GNOME starts on the GPU) |
| Bluetooth | `qca/brhbtfw20.mbn` not found | **recovers by itself**: setup runs again when userspace powers the controller |
| Touch | firmware download fails, the driver does not retry | `init` no longer loads it without firmware; the root's udev loads it and it downloads the root's firmware |
| Wi-Fi (ath12k) | built in (development kernel): probe fails (`amss.bin` not found, -110); as a module (release kernels): not loaded here | built in: **does not recover by itself** (a manual bind of `0000:01:00.0` to `ath12k_wifi7_pci` brings it up). **As a module** (`CONFIG_ATH12K=m`, in `kernel/config/baldur.fragment` since 2026-10-02): udev loads it from the root with the root's firmware and it connects by itself (checked on the release build rc1) |
| Audio, DSPs | not started here anyway | the root starts the DSPs (firmware from the root) |

That is why the public configuration builds ath12k as a module (open question Q1 in
[`docs/notes/kernel-updates-design.md`](../../docs/notes/kernel-updates-design.md)). A release build from the public
series (rc1, 2026-10-02) came up with Wi-Fi, GPU, Bluetooth, touch, sound and the emergency key's hash from the root.

### Root partitions and multiboot

Roots are found by **GPT partition name** — names you give the partitions when you create them:

| Name | Usually on | Role |
|---|---|---|
| `baldur-root` | UFS (a partition after `userdata`) | the default root when present |
| `baldur-root-sd` | microSD | fallback |
| `tb323fu-*` (e.g. `tb323fu-ubuntu`, `tb323fu-arch`) | microSD | more systems, one per partition |

No name is required. The **state root** is the first present of `baldur-root`, `baldur-root-sd`, then the
`tb323fu-*` partitions in sorted order (an SD card with only `tb323fu-arch`, `tb323fu-fedora`, `tb323fu-ubuntu`:
`tb323fu-arch`). The helper and Android's Switch to Linux pick it by the same rule. The selection lives on it:

- `/etc/tb323fu/boot-next`: one partition name, used **once**. `init` deletes it (and syncs) before trying that
  root, so a system that hangs later boots the default next time.
- `/etc/tb323fu/boot-default`: persistent default (otherwise the state root).
- `/etc/tb323fu/boot-menu`: if present, a menu on the panel: volume-up moves to the next root, 5 s without a
  press boots the one shown.

Order tried: boot-next, boot-default, the state root, then every other candidate in the same order. A root
that is missing, does not mount or has no init is skipped, and the screen says why. The state root also keeps
`/var/lib/tb323fu/linux-current.img`, the image `back-to-android` saves for Android's Switch to Linux.
`test-root-selection.sh` runs this part of `init` offline against fake partitions (busybox sh, dash). The init of a root is `/sbin/init` (Debian, Ubuntu, Arch,
Fedora; an absolute symlink is resolved inside the root) or NixOS's `/nix/var/nix/profiles/system/init`.
The kernel and its modules are shared by all of them: `init` mounts the image's modules into whichever root it
starts ([above](#kernel-modules-shared)).

<details>
<summary>How the switch differs for NixOS</summary>

For `/sbin/init`, `/proc`, `/sys` and `/dev` are moved into the new root. For NixOS they are unmounted
instead: NixOS stage 2 (started without a NixOS stage 1) mounts `/proc`, `/sys`, `/dev`, `/dev/shm`,
`/dev/pts`, `/run` and `/run/keys` itself, but only when it finds `/proc` unmounted.

</details>

The helper sets these files for you: `tb323fu-ctl boot list|next NAME|default NAME|reboot NAME`, or the
"Systems" page of Tablet Settings (see [../../docs/helper.md](../../docs/helper.md)).

**Kernel trial.** A kernel the helper installed is on trial until a system confirms it (90 s after it came up, or
Keep for the testing channel; [docs/helper.md](../../docs/helper.md#kernel-updates)). While reading the selection,
`init` compares `/var/lib/tb323fu/kernel-state` on the state root with `/proc/version`: a start of the trial kernel
is counted (`tries=`); its third start (after `max` = 2 starts that never got confirmed) writes `linux-good.img` back into `boot_a`
— only when it matches its recorded SHA-256, read back after the write — records `failed=` and restarts. If the
write does not read back, `init` stays in the initramfs. Volume-up held: the start is not counted.
`test-root-selection.sh` covers these cases too.

Holding **volume-up** while the summary is shown keeps the initramfs (a shell on the USB serial port).
If no root is usable, `init` also stays in the initramfs.

### Command-line knobs

The normal command line uses `baldur.end=hold`: summary, then root selection. The other knobs are for bring-up.

<details>
<summary>All knobs</summary>

| Knob | Effect |
|---|---|
| `baldur.end=hold` | normal boot: summary, then root selection (the tested command line) |
| `baldur.end=panic` / `baldur.end=reboot` | bring-up endings for a kernel started by kexec from Android: countdown, then panic or reboot back to Android with the log in ramoops |
| (none of the above) | if crash dump mode is armed, countdown and panic on purpose (the log stays in ramoops); otherwise hold |
| `baldur.wait=N` | countdown length for the panic/reboot endings (default 120 s) |
| `baldur.ui=splash` | quiet panel: summary lines go to the log at KERN_INFO, the screen is not cleared (bootloader picture stays) |
| `baldur.diag=0` | obsolete, ignored (the SD-card diagnostics it skipped are gone) |

</details>
