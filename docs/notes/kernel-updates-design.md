# Kernel updates through the helper — design

> **Design note, partly implemented.** Section 2 (shared modules) is implemented and verified on the device
> (2026-10-02, see 2.6). Sections 3 and 4 (update flow, trial boot and rollback, helper notification) are
> implemented and checked on the device with development kernels (3.7: M4 trial and rollback, M5 helper flow).
> **Channel changed 2026-10-03 (3.8):** releases come from GitHub Releases with `SHA256SUMS`; the signed index and
> manifests of 3.1, 3.2 and 3.7 are replaced (minisign stays as an option, off by default); kernels can also be
> installed from a local file. Plan 2 (CI builds with artifact attestations) is 3.9.
> `docs/notes/` is not rendered on the project site (the site picks up `docs/*.md` only).

## 1. The problem

The boot image in `boot_a` carries the kernel, its device tree and its built-in initramfs
([kernel/initramfs](../../kernel/initramfs/README.md)). That one kernel boots every root the initramfs can
select — today six of them: the UFS root `baldur-root` (Debian) and `tb323fu-{ubuntu,arch,fedora,nixos,spare}`
on the SD card. The kernel's **modules, however, live in each root**:

| Root | Where the modules come from | What a kernel change costs |
|---|---|---|
| Debian, Ubuntu, Arch, Fedora, SteamOS | a copy in `/lib/modules/<release>` (or `/usr/lib/…`, merged `/usr`) | unpack, keep the root's own out-of-tree modules, `depmod` — once per root, with that root mounted |
| NixOS | its system generation: [`prebuilt-kernel.nix`](../../packaging/nix/prebuilt-kernel.nix) copies a modules tree into the store, NixOS's `system.modulesTree` aggregates it into `/run/booted-system/kernel-modules/lib/modules/<ver>` | copy the tree into the flake, rebuild the system on the tablet, copy the closure to the partition, switch the profile |

So every kernel change is **N module installs plus a NixOS rebuild**, and every step can go wrong
independently. The manual roll-outs of the last two development kernels showed each failure mode:

- **An out-of-tree module was dropped.** The aw882xx speaker-amplifier driver lives in `extra/`; an install
  that carried over only `updates/` left the sound card unbound (`Speaker Playback: codec dai not found`)
  on every root it touched, and a second run also removed it from the backup copy.
- **Stale `updates/` copies shadowed fixed in-tree modules.** Fixes tested as `updates/*.ko` (camss, iris,
  q6apm) were later merged into the tree; a root that kept its `updates/` copy would load the old module.
  The install tool now needs a "supersedes" list shipped with each build.
- **The release string never changes** (`7.3.0-rc4-oneplus-infiniti+` for every build), so a new tree has to
  overwrite the old one in place; rollback means swapping `<release>` and `<release>.prev` in every root.
- **Order matters**: modules installed first and a failed flash leaves the old kernel with new modules.
- **NixOS needs a full rebuild on the device** for a kernel change, and the rebuild once left the
  partition's `/` mode 777 (sshd refused all keys).
- **The Android way back can boot a mismatched pair**: `back-to-android` saves the running image as
  `linux-current.img`, and the KernelSU action writes it back — but the modules in the roots may have moved
  on since.
- **Roots that are not mounted** (SD card out, a root added later) silently fall behind; the helper's
  `RootHealth` check exists only to catch that.

A user-facing update ("a new kernel is available — install") cannot be built on top of this. The fix is to
make the kernel and its modules **one artefact**.

## 2. Shared modules

### 2.1 Options

| | (a) Modules image on shared storage | (b) Modules image inside the boot image | (c) Per-root copies, automated |
|---|---|---|---|
| What | one read-only squashfs/erofs per kernel release, on the UFS root (`/var/lib/tb323fu/modules/<release>.sqfs`) or a small extra partition; the initramfs loop-mounts it onto `<root>/lib/modules/<release>` before `switch_root` | the same image file inside the built-in initramfs; the initramfs loop-mounts it the same way | the helper (or a tool) mounts every root and installs the tree there; NixOS rebuilds |
| Kernel and modules always match | only if the image for that release is still on disk (retention policy needed) | **yes — one file, one hash, one write** | no (the failure modes above) |
| Android fallback (`linux-current.img`) | needs the matching modules image kept | **automatically right** | can be wrong |
| Works without a UFS root (SD-only setup) | needs another place (or a new partition) | yes | yes |
| Rollback | flash the old image; old modules image must still exist | **flash the old image** | swap `.prev` in every root, NixOS rollback |
| Extra partition / repartitioning | (partition variant) yes | no | no |
| `boot_a` size budget | unaffected | modules add ≈ 10 MiB compressed (below) | unaffected |
| RAM | page cache only | the image file stays in initramfs memory (≈ 10 MiB, unswappable) | none |
| NixOS | no rebuild per kernel (2.5) | no rebuild per kernel (2.5) | rebuild per kernel, on the device |
| Per-root out-of-tree modules (DKMS etc.) | need an overlay (2.4) | need an overlay (2.4) | natural |
| Module-only hotfix | ship a new modules image | needs a new boot image (same release pipeline anyway) | natural |

**Size check for (b).** `boot_a` is 96 MiB (100 663 296 bytes; the images are padded to it). The current
development image's kernel field is 57.2 MB — the uncompressed arm64 `Image` with its gzip'd initramfs,
which today also carries the device's firmware (Adreno, Wi-Fi, Bluetooth, touch). The full stripped modules
tree of the current build is 740 modules, 34.5 MiB uncompressed and 8.1 MiB as `tar.gz`; as squashfs-xz
it should land at ≈ 9–11 MiB. A release image without firmware plus the modules image comes to roughly
60–65 MiB, leaving ≈ 30 MiB of headroom. (To be measured in step M0, section 6.)

### 2.2 Decision: (b) — the modules image travels inside the boot image

The kernel, its device tree, its initramfs and **all its modules** (including out-of-tree ones in `extra/`)
become one file whose SHA-256 identifies the whole kernel. Writing `boot_a` is the only install step; every
root, NixOS included, sees exactly the modules of the kernel that booted; the Android round trip and
rollback are correct by construction because they already move whole boot images.

(a) is the fallback if `boot_a` runs out of space: the mount code in the initramfs is identical, only the
image's location changes (and a retention rule — keep the modules image of every boot image that is saved
somewhere: `linux-current`, `linux-good`, the staged one). (c) is kept only for roots that opt out (2.4).

### 2.3 Layout and mount

Build side (the release build, replacing today's tarball):

- **Unique release string per build**: `CONFIG_LOCALVERSION="-tb323fu-tNN"` (and a clean tree, so no `+`),
  e.g. `7.3.0-rc4-tb323fu-t28`. Old and new trees can then coexist anywhere, nothing is overwritten in
  place, and "which modules belong to which kernel" is answered by `uname -r` alone.
- `make modules_install INSTALL_MOD_STRIP=1`, out-of-tree modules into `extra/` (aw882xx, built in the same
  run against the same tree), `depmod -b` **at build time** — `modules.dep`, `.alias`, `.devname`,
  `.builtin*`, `.softdep` are part of the image. No `updates/` directory: a fix is a new release.
- `mksquashfs <tree> modules.sqfs -comp xz -all-root`. Squashfs with xz and lz4 and the loop driver are
  already built into the kernel (`CONFIG_SQUASHFS=y`, `CONFIG_SQUASHFS_XZ=y`, `CONFIG_BLK_DEV_LOOP=y` in
  [reference.config](../../kernel/config/reference.config)); erofs would need a config change for no clear gain.
- The initramfs gets `/lib/modules/<release>.sqfs` (`build.sh -m`). The flat `/lib/modules/*.ko` copies of
  the early modules (remoteproc PAS, touch) go away: init mounts the image first and uses `modprobe`.

Boot side (`kernel/initramfs/init`):

1. Early, before loading the PAS/touch modules: `mount -t squashfs -o ro,loop /lib/modules/$rel.sqfs
   /lib/modules/$rel` (inside the initramfs), then `modprobe qcom_q6v5_pas nt36536_ts`.
2. After a root is chosen and mounted at `/newroot`: resolve `lib/modules` **inside** the root (merged-`/usr`
   roots have an absolute `/lib → usr/lib` link; resolve it the way `inroot()` already does), `mkdir -p` the
   `<release>` directory there, then **`mount --move /lib/modules/$rel /newroot/<resolved>/$rel`**. The loop
   device keeps the image file alive after `switch_root` deletes the initramfs' files.
3. For NixOS, `/dev`, `/proc`, `/sys` are unmounted before `switch_root` (as today); the module mount is
   under `/newroot` and survives. NixOS has no `/lib`; creating `/lib/modules/<release>` as an empty mount
   point is harmless (2.5).
4. Opt-out: if `<root>/etc/tb323fu/modules` contains `own`, init does not mount and the root uses its own
   tree (developers, roots with DKMS, the transition). If `mkdir` fails (a read-only root), init says so on the
   panel and boots without the mount; the root must then carry its own tree.

After boot: `findmnt /lib/modules/$(uname -r)` shows `squashfs /dev/loop0 ro`. The helper's `Kernel` object
reports it; `RootHealth` no longer has to check `modules.dep` / `extra/` for roots that use the shared tree.

### 2.4 Distribution tooling with a read-only `/lib/modules/<release>`

| Tool | What happens | Handling |
|---|---|---|
| `depmod -a` (manual, or package scripts that run it for `uname -r`) | fails with EROFS, nothing changes | harmless — the image ships complete depmod output |
| kmod, `systemd-modules-load`, udev `kmod` builtin, `kmod-static-nodes` (reads `modules.devname`) | read only | work unchanged |
| initramfs-tools `update-initramfs`, dracut, mkinitcpio, `kernel-install` | run for **distribution kernels** (`/boot/vmlinuz-*`, `/usr/lib/modules/*/vmlinuz`, kernel packages) | the rootfs builders install no distribution kernel; if one gets installed it uses its own release directory and never touches ours. Builders pin it out: Fedora `exclude=kernel*` in dnf.conf, Arch `IgnorePkg = linux*`, Debian/Ubuntu no `linux-image-*` (an apt preference with `Pin-Priority: -1` if a metapackage pulls one) |
| DKMS / out-of-tree packages writing `/lib/modules/<release>/updates` | EROFS; also no `build/` headers | not supported in the shared mode. Opt-in per root: `/etc/tb323fu/modules = overlay` → init mounts an overlayfs (lower: the image, upper: `<root>/var/lib/tb323fu/modules-local/<release>/`), and a layer-1 unit reruns `depmod` when the upper holds `.ko` files. Keyed by release, so a new kernel starts with an empty upper — nothing stale can shadow it. Needs `CONFIG_OVERLAY_FS=y` (now `m`). Later, only if someone asks |
| Fedora SELinux | the squashfs has no labels | `-o context=system_u:object_r:modules_object_t:s0` on the mount for roots with SELinux enforcing (Fedora runs permissive today) — check in M2 |
| Developers replacing one module for a test | EROFS | `own` mode, or at run time: copy the tree to tmpfs, `mount --bind` over the mount, edit, `depmod` — gone on reboot, which is what a test wants |

### 2.5 NixOS

NixOS looks for modules through nixpkgs' kmod, which is built with **several default module
directories** (`--with-modulesdirs`, `module-dir.patch`): `/run/booted-system/kernel-modules`,
`/run/current-system/kernel-modules`, then `""` — i.e. `<prefix>/lib/modules/<release>` for each prefix, the
**first directory that exists wins**, and the empty prefix is plain `/lib/modules/<release>`.

So the shared mount works for NixOS without touching the store, provided the generation does **not**
contain a directory for the running release:

- `prebuilt-kernel.nix` gets a **stub mode**: `modDirVersion = "0-tb323fu-shared"`, an empty
  `lib/modules/0-tb323fu-shared` (NixOS's `aggregateModules` runs `depmod` on it at build time; an empty tree
  is fine), and the kernel `config` of the current release series for NixOS modules that query
  `boot.kernelPackages.kernel.config` (sysctl, …). `system.modulesTree` then aggregates to that stub;
  `/run/booted-system/kernel-modules/lib/modules/<running release>` does not exist; kmod falls through to
  `/lib/modules/<running release>` = the initramfs' mount.
- `rootfs/nixos` stops copying `/lib/modules` into the flake (`MODULES_FROM` becomes optional, only for an
  `own`-mode NixOS). A kernel update then needs **no NixOS rebuild at all**; generations change only for
  NixOS configuration.
- Not supported in stub mode: `boot.extraModulePackages` (they would be built against the stub). They were
  never usable with the prebuilt kernel either.
- To verify on the device (M2): `modprobe -v`/`modinfo -n` paths, systemd-udevd autoloading, and whether
  the `kernel.modprobe` usermode helper or any unit sets `MODULE_DIR` (that would bypass the search and
  must point at `/lib/modules`). If the fall-through ever stops working, the alternative is a stage-2 hook:
  the initramfs passes the mount at `/lib/modules/<release>` and a NixOS activation snippet symlinks
  `kernel-modules` there — more moving parts, so not the first choice.

### 2.6 Implementation (2026-10-02)

Implemented as designed in 2.2–2.5; the details that were left open or came out differently:

- **Build** (`kernel/initramfs/build.sh -m MODULES_DIR`): `MODULES_DIR` is the installed, depmod'ed
  `<INSTALL_MOD_PATH>/lib/modules/<release>`; build.sh refuses a tree without `modules.dep`, with `build`/`source`
  links or without the early modules, and runs `mksquashfs -comp xz -all-root -no-xattrs -mkfs-time 0 -all-time 0`.
  The cpio gets `/lib/modules/<release>.sqfs` and the empty mount point `/lib/modules/<release>`. Without `-m` the
  old layout (flat early modules) is built. **No build-identity file inside the image:** the banner (`#N`, date) is
  only known after the final link, which already contains the image; the tree is the kernel's by construction. The
  own-mode tarball (`modules-tb323fu-<tag>.tar.gz`, same tree) carries `tb323fu-build` = the banner.
- **Early modules**: loaded by a small shell `modload` that reads `modules.dep` (dependencies first) and `insmod`s
  from the mount — busybox's small `modprobe` would scan all modules.
- **Move**: after the root's init is found, before the processes of the initramfs are stopped; only the root that
  boots gets it. Panel/kmsg line: `baldur:  modules: shared image on <resolved path>`. The resolved path is what
  `/proc/self/mountinfo` shows (`/usr/lib/modules/<release>` on merged-`/usr` roots); `findmnt -T
  /lib/modules/$(uname -r)` finds it from any root. The loop device shows `BACK-FILE /lib/modules/<release>.sqfs
  (deleted)`, autoclear set.
- **`/etc/tb323fu/modules`**: `own` = no move, the image is unmounted, the panel says whether the root has a
  `modules.dep` for the release; empty or `shared` = default; `overlay` (Q5) is **not implemented** — such a root
  gets the shared tree and the panel says so. A failed `mkdir`/move boots without the mount (said on the panel).
- **SharedModules for the helper**: true when `/lib/modules/$(uname -r)` resolves to a `squashfs` mount from a
  `/dev/loop*` device (`findmnt -n -o FSTYPE,SOURCE -T /lib/modules/$(uname -r)`).
- **NixOS**: `prebuilt-kernel.nix` with `modules = null` (now the default of `tb323fu.rootfs.kernel.modules`) builds
  `linux-tb323fu-shared-modules`: `lib/modules/0-tb323fu-shared` with empty `modules.order`/`modules.builtin`,
  version from the config header. Checked on the device: nixpkgs' kmod (31, `--with-modulesdirs`) falls through, no
  unit sets `MODULE_DIR`, `kernel.modprobe` is the store's kmod. The fall-through also worked with the **old**
  generation (store modules for `7.3.0-rc4-oneplus-infiniti+`, booting `…-tb323fu-t28`): an existing NixOS root keeps
  working before its stub rebuild, and keeps its old kernel's modules for a rollback until then.
- **Fedora SELinux** (2.4): the device's Fedora root runs with SELinux **disabled**, so no mount context was needed;
  revisit if a root enforces.
- **Sizes (M0)**, squashfs xz of the stripped tree: development kernel t28 (740 modules, 35.1 MB tree) **6.1 MB**
  (lz4hc 10.2 MB, tar.gz 8.4 MB); release rc2 (782 modules) 6.2 MB. Images: t28 (development, with firmware) raw
  62.6 MB / gzip 26.5 MB; release rc2 (no firmware) raw **36.2 MB** / gzip **20.0 MB** — `boot_a` (96 MiB, 100.7 MB)
  headroom 64 MB raw, 81 MB gzip.
- **Device checks** (t28 = development tree + shared modules, rc2 = public release build): M1 Debian, M2 Ubuntu, Arch,
  Fedora, NixOS (old generation and stub generation), baldur-root-sd; M3 Android round trip (`back-to-android` →
  Switch to Linux → the same image). On 2026-10-02 every root mounted the shared modules on the development tablet.

## 3. Update flow through the helper

Firmware is **never** shipped: release images carry no vendor firmware; users extract it from their own
tablet ([firmware/](../../firmware/)). The release image must therefore boot with the firmware that is in the
root only — see open question Q1.

### 3.1 Release artefacts

> Superseded by 3.8 (GitHub Releases; no manifest, no index). Kept as the record of the first design.

One GitHub Release per kernel build, tag `kernel-tNN`:

| Asset | What |
|---|---|
| `Image-tb323fu-tNN` (+ its modules) | the kernel `Image` (DT, command line, initramfs and — with this design — the modules image built in), **not a boot image**: a `boot.img` built here would carry the builder's stock header and Lenovo's GKI signature/vbmeta blobs, which are not ours to redistribute and must match the user's firmware. The helper packs the kernel into the user's own stock image — the copy in `boot_b` — with `tools/boot-repack-kernel.py` (decided 2026-10-02) |
| `tb323fu-kernel-tNN.json` | manifest: `release` (`uname -r`), `build` (the `/proc/version` line), `serial` (monotonic integer), `channel`, `sha256` of the uncompressed image and of the compressed file, `size` (= `boot_a`), `min_platform` (layer-1 package version the kernel needs, e.g. new UCM or udev rules), `min_helper`, source tag and commit, links to the GPL sources (kernel tree, aw882xx, busybox) |
| `tb323fu-kernel-tNN.json.minisig` | signature over the manifest |
| release notes (the release body) | what changed, which firmware files it expects (by name, from the manifest in `firmware/`), known problems |

A signed **index** on the project site, `https://joonhoekim.github.io/tb323fu-linux/kernel/index.json` (+
`.minisig`), lists per channel the current manifest URL and an `expires` date. Static files avoid the GitHub
API's unauthenticated rate limit (60 requests/hour) and let a channel move without editing releases;
the Releases API is the fallback when the site is down.

Channels: **`stable`** (default) and **`testing`** (every build that passed the device checks of the release
procedure). The channel is a helper setting (`kernel.channel` in `helper.toml`).

### 3.2 Verification: minisign

> Superseded by 3.8: integrity through `SHA256SUMS` and GitHub's asset digest; minisign over `SHA256SUMS` only
> when `require_signature` is set (no key built in). Plan 2 (3.9) replaces keys with build attestations.

- **minisign** (Ed25519): one small public key, offline verification, a pure-Rust verifier
  (`minisign-verify`, no C dependencies) inside the helper. Releases are built on the maintainer's machine,
  not in CI, so cosign's keyless mode (GitHub OIDC identity + transparency log) would add a network
  dependency and an identity that does not match where builds happen.
- **What is signed:** the index and each manifest; files are checked by the SHA-256 in the signed manifest.
  The helper checks `serial` > installed serial (downgrades only through an explicit "install older"
  with admin authentication) and the index's `expires` (a frozen index is reported, not trusted forever).
- **Keys:** the public key is compiled into the helper and shipped as
  `/usr/share/tb323fu/keys/kernel-<keyid>.pub`; the helper accepts a short list, so a rotation is: publish
  the new key in a helper release, sign with both for one cycle, then drop the old one. The secret key stays
  offline with the maintainer (password-encrypted minisign key, plus an offline backup), never in a
  repository or on the tablet. A lost key means a helper release with a new key.
- Distribution packages (section 4) keep their own OpenPGP signing; minisign is only for the kernel channel.

### 3.3 Download and staging

- The helper daemon never talks to the network itself. A oneshot unit `tb323fu-kernel-fetch.service`
  (`DynamicUser=yes`, `CacheDirectory=tb323fu/kernel`, network allowed, nothing else writable) fetches the
  index, manifest, signature and image into `/var/cache/tb323fu/kernel/<tNN>/`, started by the daemon on
  request (`Check`, `Download`) and by a daily timer (check only, never download on a metered connection —
  NetworkManager's `Metered` property).
- The daemon (root) then verifies: signature, image SHA-256 after decompression, size equal to `boot_a`,
  `ANDROID!` header v4, the `release` string inside the image matching the manifest. Only a verified image
  is offered for installation.

### 3.4 Install (writing `boot_a`)

Done by the daemon, method `Kernel.Install(tag, reboot)`, polkit `kernel-install` (`auth_admin_keep`):

1. Preconditions — the same checks `back-to-android` and the KernelSU action make: `boot_a`/`boot_b` found by
   GPT name and unique; **`boot_b` matches the recorded Android hash** (the way back exists); battery ≥ 30 %
   or external power; no trial already running (3.5).
2. **Keep the image that works**: if the running kernel is confirmed (3.5), copy `boot_a` to
   `/var/lib/tb323fu/linux-good.img` (+ `.sha256`) on the state root (normally `baldur-root`, mounted under
   `/run/tb323fu/state` when running from another root, as the `Boot` object already does), via a `.tmp` file,
   hash check, `rename`, `sync`.
3. Write the trial record (3.5) on the same root, `sync`.
4. `dd` the verified image to `boot_a` (`conv=fsync`), drop caches, read `boot_a` back and compare the
   SHA-256. On a mismatch: write `linux-good.img` back, verify, delete the trial record, report the error.
   A 96 MiB write takes about a second on UFS; there is no atomic way to replace a partition, so the
   power check in step 1 and the read-back are the protection (a half-written `boot_a` still leaves
   EDL; fastboot is read-only on this bootloader).
5. Reboot now or later (the user's choice). Until the reboot, `Kernel.Pending` shows the installed tag.

### 3.5 Boot-success marker and automatic fallback

State lives in `/var/lib/tb323fu/kernel-state` on the state root (normally the UFS root; the root Android reads, Q6):

```
good=7.3.0-rc4-tb323fu-t27      # release of linux-good.img
good_sha256=…
trial=7.3.0-rc4-tb323fu-t28     # set by Install, cleared by the confirm unit
tries=0
max=2
```

- **Initramfs (every release has this code):** after reading the boot selection, if `trial` equals
  `uname -r`: increment `tries`, `sync`. If `tries > max`: check `linux-good.img` against `good_sha256`,
  write it to `boot_a`, read back, record `failed=<release>`, clear `trial`, reboot. The panel says what it
  does; holding volume-up still keeps the initramfs.
- **Confirm unit** (layer 1, in the platform package and the NixOS module, so it never depends on the
  helper): `tb323fu-kernel-confirm.timer`, `OnBootSec=90s`, wanted by `graphical.target` (or
  `multi-user.target` on roots without a desktop). Its script clears `trial`, sets `good=` to the running
  release, and copies `boot_a` to `linux-good.img` (same tmp/hash/rename steps). A root that reaches its
  target and stays up for 90 s confirms the kernel, whichever root it is.
- **Kernel panics before or after init** reboot (`panic=10 oops=panic` are in the built-in command line),
  so a crash during boot comes back to the initramfs and counts as a try. The watchdog that the initramfs
  feeds covers hangs after init has started, as long as the bootloader leaves it armed (Q3).
- **What it cannot catch:** a kernel that hangs or panics **before** `/init` every time. Nothing of ours runs
  then. The emergency chord needs the initramfs too, and fastboot is read-only on this bootloader, so the way
  out is EDL, writing `boot_a` from a PC ([recovery](../recovery.md#linux-does-not-boot)). The `testing` channel
  exists so that this is caught on the maintainer's tablet first.
- **Through Android:** the KernelSU action today writes `linux-current.img` (the image that ran last). It
  gains one rule: if `kernel-state` has an unconfirmed `trial` or a `failed=` equal to the saved image's
  release, it writes `linux-good.img` instead and says so. `back-to-android` is unchanged (it still saves
  the running image as `linux-current.img`). The emergency chord stays as is.

### 3.6 D-Bus, polkit, CLI, UI

New object `/io/github/joonhoekim/OpenDeviceHelper1/Kernel`, interface `…Helper.Kernel`:

| | |
|---|---|
| Properties | `Running` (s: `uname -r`), `RunningBuild` (s), `SharedModules` (b: `/lib/modules/<release>` is the image mount), `Channel` (s), `Available` (a(sssu): tag, release, notes URL, serial — newer than running, in the channel), `Downloaded` (as), `State` (s: `idle` / `checking` / `downloading` / `verifying` / `ready` / `installing` / `pending-reboot` / `trial` / `rolled-back`), `Progress` (u, %), `Trial` (s: release on trial, ""), `Good` (s: release of `linux-good.img`), `LastFailed` (s), `LastCheck` (t), `IndexExpired` (b) |
| Methods | `Check()`, `Download(s tag)`, `Install(s tag, b reboot)`, `Rollback(b reboot)` (write `linux-good.img`), `SetChannel(s)`, `Notes(s tag) → s` |
| Signals | `Finished(s operation, b ok, s message)` |

polkit actions (defaults for the active local user): `kernel-check` — yes; `kernel-download` — yes;
`kernel-install` — `auth_admin_keep`; `kernel-rollback` — `auth_admin_keep`; `kernel-channel` —
`auth_admin_keep`.

The daemon unit needs the network-free parts only (it already may write `/var/lib`, mount roots under
`/run/tb323fu`, and open block devices); downloading is the separate unit above.

CLI: `tb323fu-ctl kernel status | check | list | notes TAG | download TAG | install TAG [--reboot] |
rollback [--reboot] | channel stable|testing`.

Settings app:

- **About**: the Kernel row (release, build, "shared modules"), and when `Available` is not empty a row
  "Kernel tNN available" with the release notes (rendered Markdown from the release body), **Download** →
  **Install** (authentication) → **Restart now / later**. After a rollback: a banner "tNN did not start
  twice; back on tMM" with a link to Diagnostics export.
- **Systems**: a line on top while a trial runs ("Trying kernel tNN — confirmed 90 s after a system has
  started"). Rows keep their health subtitles; the modules problems disappear for roots using the shared
  tree, an `own`-mode root shows "own modules — not updated with the kernel".
- The GNOME tile stays as is; a desktop notification (from the app's background check, or the CLI) is
  enough for "update available".

### 3.7 Implementation (2026-10-02)

> The release files, keys, index fetch and `helper.latest` below were replaced on 2026-10-03 (3.8); the trial,
> confirm, install and Android parts stand.

Implemented as designed in 3.1–3.6, tested without the device and then on it (M4, M5 below); what
was left open or came out differently:

- **Release files** (`tools/kernel-channel.py` `add`/`index`/`sign`/`verify`, since removed; replaced by
  [`tools/kernel-release.py`](../../tools/kernel-release.py)): a
  channel directory with `index.json` and per release `<tag>/tb323fu-<tag>.json` + the kernel file; every `.json`
  has a `.minisig`. Index: `format`, `generated`, `expires`, `channels` (`<name>` → `tag`, `serial`, `manifest`
  URL, relative to the index), `helper` (`latest`, `notes`). Manifest: `format`, `tag`, `release`, `build` (the
  banner), `serial`, `channel`, `kernel` (`file`, optional `url`, `sha256`, `size`), `image_sha256` (uncompressed),
  `min_helper`, `min_platform`, `notes` (Markdown, inside the signed manifest — the daemon has no network for a
  separate notes file), `notes_url`, `source_tag`, `source_commit`, `gpl_sources`. The tool reads release and
  banner from the Image itself. **An Image carries the banner twice**: a placeholder from `init/version.o` with an
  empty build number (`# SMP PREEMPT `) and the real one (`#7 SMP PREEMPT <date>`); the first match was the
  placeholder, which would never equal `/proc/version` (found by the real-image test below) — both the tool and
  the helper take the one with a build number.
- **Keys**: project key id `A5D2DA7287637413` compiled into the daemon (`kernel::KEYS`) and installed as
  `/usr/share/tb323fu/keys/kernel-A5D2DA7287637413.pub`; `/etc/tb323fu/keys/kernel-*.pub` adds keys (a local test
  channel uses a separate test key). The secret keys are not in any repository. Verifier: `minisign-verify` 0.2.
- **Fetch**: the daemon writes `NAME MAXBYTES URL` lines to `/run/tb323fu/kernel-fetch.list` and runs `systemctl
  start tb323fu-kernel-fetch.service`; the unit runs `tb323fu-kernel-fetch` (curl; https, http, file) as a dynamic
  user into `/var/cache/tb323fu-kernel`. The daemon re-reads everything with size limits and verifies it, and keeps
  verified files in `/var/lib/tb323fu/kernel/` (`index.json`, `manifests/`, `staged/<tag>/`) on the running root.
  The **daily check runs in the daemon** (once a day, five minutes after boot at the earliest, `kernel.auto_check`)
  instead of a separate timer; it checks only, so there is no metered-connection test (a download is always asked
  for). Download progress comes from the size of the partial file.
- **Install**: the repack is a Rust port of `boot-repack-kernel.py` (`bootimg.rs`); on the development tablet's real
  stock image with the rc2 Image, raw and gzip, it gives the Python tool's image byte for byte
  (`cargo test -- --ignored real_images`). The running kernel is "in boot_a" when the kernel in `boot_a` carries the
  exact `/proc/version` line; then `boot_a` becomes `linux-good.img` (unless it already is). Otherwise (a pending
  install, a hand-written `boot_a`) a verified `linux-good.img` is required. Refused: a running kernel that is still
  on trial, a release not newer by serial (downgrades are not offered at all yet), `min_helper` (and `min_platform`
  when the platform files record their version in `share/tb323fu/platform-version`).
- **`kernel-state` keys** beyond 3.5: `good_sha256`, `good_version`, `good_serial`, `trial_sha256` (the full boot
  image written), `trial_version` (the banner = `/proc/version` of the trial kernel: matches builds with the same
  release string too), `trial_serial`, `trial_channel`, `failed_sha256`, `failed_seen` (the notice was dismissed).
  Writers: the daemon (tmp + rename), the initramfs and the confirm script (`grep -v` + append, `mv`).
- **Initramfs**: in the state-root block of the root selection (`ktrial`); counted only when the trial matches
  `/proc/version`; volume-up held: not counted; the third start writes `linux-good.img` back (only if it matches
  `good_sha256`), reads back, records `failed=`/`failed_sha256=`, clears the trial and restarts; a read-back mismatch
  stays in the initramfs. No `linux-good.img`: the trial kernel boots on (and says so).
- **Confirm** (`tb323fu-kernel-confirm.service`, platform files): a service ordered after `graphical.target` and
  `multi-user.target`, wanted by `multi-user.target`, that sleeps 90 s and confirms — a timer would need to be
  ordered after `graphical.target`, which cycles with `timers.target` unless default dependencies are dropped. The
  script also records a running kernel that is not in `kernel-state` at all (written by hand) as good once boot_a
  carries its banner, and drops a stale trial record whose image is no longer in `boot_a` (e.g. Android wrote
  `linux-good.img` back). It tells the daemon (`Kernel.Refresh()`, optional).
- **Android**: the KernelSU action compares the saved image's hash with `trial_sha256` and `failed_sha256`.
- **D-Bus** beyond 3.6: methods `Keep()` (Q7), `Dismiss()` (hides the rollback notice; the record stays),
  `Refresh()`, `SetAutoCheck(b)`, `SetHelperNotify(b)`; properties `TrialChannel`, `Tries`, `MaxTries`,
  `KeepPending`, `AutoCheck`, `HelperLatest`, `HelperUpdateCommand`, `Message`; polkit `kernel-keep` (allowed). The
  long methods return their message when done (no client timeout in zbus) besides `Finished`. CLI as in 3.6 plus
  `update`, `keep`, `dismiss`, `auto-check`, `helper-notify`.
- **RootHealth**: with shared modules the modules checks apply only to `own`-mode roots (with a tree: "own modules
  (not updated with the kernel)").
- **Helper self-update** (section 4): notification only, from the signed index's `helper.latest` (not the GitHub
  Releases API: the index is already fetched and signed); the command comes from `os-release` (apt, pacman, dnf,
  the NixOS flake; SteamOS: not available yet). Opt-out `kernel.helper_notify`.
- **Not done**: "install older" (admin downgrade), a desktop notification for "update available", the apt/pacman
  repositories and COPR of section 4, `tb323fu-ctl self-update`.
- **Tests** (no device): `cargo test` (library: signatures, expiry, tampering, install/confirm/rollback on files, a
  read-back mismatch restoring `linux-good.img`), `helper/tests/kernel-update-test.sh` (daemon + CLI end to end
  against a `file://` channel signed with a throw-away key), `kernel/initramfs/test-root-selection.sh` (trial
  cases), `userspace/platform/test-kernel-confirm.sh`, `android/test-state-root.sh`.
- **Device checks (2026-10-02, development kernels t29/t30 = t28 + this initramfs, local channel over HTTP on the
  tablet, signed with the test key):** M4 — stable channel, confirm unit masked: starts 1 and 2 counted, the third
  start wrote `linux-good.img` (t28) back and restarted, `failed=t29`, State `rolled-back`. M5 — `tb323fu-ctl kernel
  update --reboot`: check and download through `tb323fu-kernel-fetch.service`, the installed `boot_a` equal to
  `boot-repack-kernel.py`'s image of the tablet's own `boot_b` byte for byte, confirmed 96 s after boot
  (`graphical.target` at 6 s); testing channel: installed from the settings app by a person, the confirm unit left
  t30 alone, Keep pressed in the app. Android round trip with the new KernelSU module and with the helper's Android
  switch. Found there: the trial must follow the channel installed from, not the manifest's (fixed);
  `back-to-android` run by the helper did not reboot (systemd's `reboot -f` cannot write `/run` in the sandbox;
  falls back to `systemctl reboot --force` now). The tablet's clock starts in 1970 until NTP: `LastCheck` and the
  index expiry use whatever the clock says (no trusted time).

### 3.8 Channel: GitHub Releases (2026-10-03)

**Decided (maintainer, 2026-10-03):** the channel is the project's **GitHub Releases**, read over HTTPS through the
GitHub REST API; the signed index and manifests of 3.1–3.2 and 3.7 are gone. Reasons: one place to publish (the
release is also where the GPL sources and notes live), no second hosting path to keep in sync, no signing key whose
loss or leak would need a helper release (3.2, section 7), and forks work the same way. The cost: authenticity rests
on the GitHub account that publishes, not on an offline key (below).

- **Source**: `[kernel] source = "github:OWNER/REPO"` (default `github:joonhoekim/tb323fu-linux`), `api_url`
  (default `https://api.github.com`; `http://`/`file://` only for a local test stand-in). The fetch unit reads
  `GET /repos/OWNER/REPO/releases` (the 30 newest; `Accept: application/vnd.github+json`, API version 2022-11-28).
- **Release = a GitHub Release** with `Image-tb323fu-tNN` (raw), optionally `Image-tb323fu-tNN.gz`, and
  `SHA256SUMS`; the release body is the notes. `NN` from the asset name is the serial. Releases without exactly one
  kernel serial and a `SHA256SUMS`, drafts, and assets whose URL is not under this repository's
  `/releases/assets/` are ignored. The helper downloads the `.gz` when there is one (20 MB instead of 36 MB for rc2;
  Q2: the bootloader decompresses gzip) and installs it as it is.
- **Channels**: `stable` = the newest (by serial) release that is not a pre-release; `testing` = the newest of all,
  pre-releases included (so a pre-release promoted to a release is still the newest for testing users). A
  pre-release always waits for Keep, also when someone installs it with the channel set to stable.
- **Integrity, not authenticity**: the kernel file must match its line in `SHA256SUMS`, the `digest` GitHub reports
  for the asset (`sha256:…`, computed by GitHub at upload; checked when the API gives it), its size, a bootable
  format, and carry a banner whose release ends in `-tb323fu-tNN`. `SHA256SUMS` comes over the same channel, so this
  catches transfer errors and a mixed-up upload — **not** a changed release. Whoever can publish releases in the
  repository decides what the helpers install.
- **Requirements** (`min_helper`, `min_platform` of the old manifest): a line `<!-- tb323fu: min_helper=X
  min_platform=Y -->` in the release body (invisible in the rendered notes).
- **minisign stays as an option, off by default**: `require_signature = true` makes the helper require
  `SHA256SUMS.minisig` from a configured key (`public_keys` in `helper.toml`, or `/etc/tb323fu/keys/kernel-*.pub`);
  no key is built in (`kernel::KEYS` is empty), the packaged project key file is no longer installed and
  `install.sh` removes it. The verifier and its tests stay. For whoever runs their own channel and wants a key.
- **Rate limit and caching**: unauthenticated API calls are limited to 60 an hour per address. The daily check makes
  one; the release list is cached with its `ETag` and asked for with `If-None-Match` — a `304 Not Modified` reuses
  the cached copy (and does not count against the limit). An exhausted limit is reported with the reset time
  (`x-ratelimit-reset`). Downloads go through the API asset URL (`Accept: application/octet-stream`; GitHub
  redirects to its download host), two calls per download.
- **Token, optional**: for a private repository (or many tablets behind one address) a token can be given as the
  systemd credential `tb323fu-github-token` (`ImportCredential=` in the fetch unit; the file
  `/etc/credstore/tb323fu-github-token`). Only the fetch script reads it; it sends it only to
  `https://api.github.com/` (curl does not forward it to the redirect host). A fine-grained token with read-only
  `contents` access to that one repository is enough.
- **Helper notice** (section 4): a release tagged `helper-vX.Y.Z` (not a pre-release) newer than the running helper.
- **D-Bus**: unchanged methods; new properties `Source`, `RequireSignature`, `TrialLabel`, `GoodLabel`;
  `IndexExpired` stays and is always false; `Available` carries the release title and the release page URL.

**Account hardening** (what the trust above rests on; the maintainer's checklist):

- two-factor authentication on the publishing account — **on**; recovery codes stored offline, not on the PC that
  builds;
- tokens least-privilege and short-lived: a fine-grained token per use (`gh` on the build PC: this repository only,
  `contents: write` for releases; no classic `repo`-scope tokens), none stored on tablets or in CI;
- protected release tags: a tag ruleset for `kernel-*` and `helper-v*` (only the maintainer may create, nobody may
  update or delete) — set when the repository becomes public, together with "immutable releases" if GitHub offers it
  for the repository, so a published release's assets cannot be swapped afterwards;
- releases are made from a clean public commit (the release script refuses a dirty tree) and say which commit.

**Local kernel install** (same date, maintainer's request): `tb323fu-ctl kernel install-local PATH
[--trial|--keep] [--name NAME]` and **Install Kernel from File…** in the app. The file (`Image`, `Image.gz`, or a
boot image — only its kernel field is used) is opened by the caller and passed as a file descriptor
(`InspectLocal(h)`, `InstallLocal(h, s name, b auto_confirm, b reboot)`), so the daemon (with `ProtectHome=yes`)
reads exactly what the caller could read. `InspectLocal` shows release, banner, format, whether the initramfs carries
`/lib/modules/<release>.sqfs` (the cpio file name searched in the Image and in each gzip stream inside it — the
initramfs is gzip; about 60 ms on the t30 Image) and warnings (no shared modules: roots need `own` modules; the
running build; a release without `-tNN`; a boot image). Install is the release path from the repack on: stock image
from `boot_b` after the Android hash check, `linux-good.img`, read-back, trial, automatic rollback, the Android rule
(trial or failed image → `linux-good.img`). `trial_channel=local`.

- **Q7 for local files — decided:** a kernel from a file waits for **Keep** by default (`--trial`, `trial_keep=1`,
  as testing): nobody but the person installing it has seen it run, and the 90 s rule would confirm a kernel that
  boots but breaks sound or Wi-Fi. `--keep` (app: "Keep it by itself once a system has run 90 seconds") lets the
  confirm unit keep it like a stable release — for builds the developer already ran.
- `kernel-state` gains `trial_keep=1` (the trial waits for Keep, whatever its channel — the confirm script and the
  initramfs check it besides `trial_channel=testing`), `trial_label`/`good_label` (the `--name`).
- **polkit**: `kernel-install-local` is `auth_admin` — the administrator's password **every time**, never kept —
  where a release install is `auth_admin_keep`: a release went through the project's publishing and the helper's
  checks against its own release data; a file went through nothing but the decision of the person at the tablet,
  and it becomes the code that runs with access to everything. `InspectLocal` only reads (`kernel-check`, allowed).
- Not done: a size or signature policy for local files (nothing to check them against), installing older official
  releases (downgrades; a local install of an old `Image` is the way meanwhile).

**Tests** (no device): `cargo test` 28 + 2 ignored (release list parsing with drafts, mixed serials, foreign asset
URLs, helper releases; channel pick; SHA256SUMS/digest/banner checks; optional signatures — no key, no signature,
another key, tampered; local files raw/gzip/boot image, with and without the modules image; install/confirm with
labels); `real_kernel_inspect` on the t30 Image (shared) and the t27 Image (no modules image);
`tests/kernel-update-test.sh` against a `file://` stand-in made with `tools/kernel-release.py fake-api` (all
passed; it also checks that the polkit policy is well-formed XML); initramfs 164/164, confirm 42/42, Android 30/30.

**Device check (2026-10-03, development kernels t30 and t31 = t28 tree + the `trial_keep` initramfs; about 12
minutes, 6 restarts, no crash):**

- **GitHub channel**: a test pre-release `kernel-t31` (`Image-tb323fu-t31`, `.gz`, `SHA256SUMS`) in a private
  repository, a token as the credential for the test only (in `/run`, gone with the next restart). `check` on the
  testing channel offered it; the second check was answered `304 Not Modified` from the cached copy; with the token
  moved away the check failed with "not found (HTTP 404) … a private repository needs a token". `download` (26 MB `.gz`, 5 s): `SHA256SUMS`
  and GitHub's `digest` (present on every asset) matched. `install`: `boot_a` = `boot-repack-kernel.py`'s image of
  the tablet's own `boot_b` byte for byte; the initramfs said "start 1 of 2 (kept when you press Keep)"; `keep` →
  good t31. The unauthenticated path against a public repository (no kernel releases) worked; the test release and
  its tag were deleted.
- **Local file, `--keep`**: the t30 **boot image** (only its kernel used), `--name "t30 back"`: the confirm unit kept
  it 96 s after boot, `good_label` carried over.
- **Local file, `--trial`**: the raw t31 `Image`: the confirm unit said "waits for Keep … not confirming it here" at
  96 s; starts 1 and 2 counted; the third start wrote `linux-good.img` (the kept t30) back and restarted: t30 runs,
  `failed=t31`, `trial_keep`/`trial_label` cleared by the initramfs, State `rolled-back`.
- **Found**: the polkit policy had `--` inside an XML comment (the new `kernel-install-local` comment) — polkit then
  registers **none** of the file's actions, so every helper call that checks one failed ("Action … is not
  registered"; seen after the restart, the install before it still worked). Fixed, and the end-to-end test now parses
  the file. `install.sh` from `git archive` is not executable (`sh install.sh`).
- Not repeated today: the Android rule (unchanged code: it compares image hashes), the settings app's file chooser
  (built and installed, not looked at by a person).

### 3.9 Plan 2: releases built by GitHub Actions, with artifact attestations

Not started; the next step for authenticity once the repository is public.

**What changes.** Release kernels are built by a GitHub Actions workflow instead of the maintainer's build machine:
a tag push (`kernel-tNN`, protected) starts a job on an x86-64 runner that cross-compiles the kernel (clang/LLVM,
`ARCH=arm64`, the public patch series and configuration), the aw882xx module, BusyBox (pinned upstream tarball) and
the firmware-free initramfs with `build.sh -m` (needs `mksquashfs`, `gen_init_cpio` from the kernel build), runs
`kernel-release.py assets` and `check`, and publishes the release. The job then calls
`actions/attest-build-provenance` on the assets: GitHub signs a SLSA provenance statement ("this file, this
SHA-256, was built by workflow W at commit C of repository R, triggered by tag T") with a short-lived Sigstore
certificate issued to the workflow's OIDC identity (keyless — no key is stored anywhere), recorded in Sigstore's
public transparency log (Rekor; GitHub's own instance for private repositories).

**Helper side.** Download the attestation bundle for the kernel file's digest
(`GET /repos/OWNER/REPO/attestations/sha256:<digest>`) through the fetch unit, and verify it in the daemon: the
Sigstore bundle's certificate chain against the Sigstore/GitHub trust root (shipped with the helper, updated with
it), the Rekor inclusion proof and signed timestamp, then the certificate's identity — `repository` =
`kernel.source`, workflow file = the project's release workflow, ref = `refs/tags/kernel-tNN` — and the subject
digest = the downloaded file. A new setting `require_attestation` (`true` by default for the official source once
releases carry attestations; `false` for forks until they set the workflow up) and `attestation_workflow`. A Rust
implementation would use the `sigstore` crate (verification only) or a small verifier for the bundle format; the
`gh attestation verify FILE --repo OWNER/REPO` command does the same on a PC and is what the docs tell people to run
by hand.

**Benefits.** No key management at all (nothing to lose, leak, rotate or protect with a password); every release
says which commit and workflow built it, publicly logged, so anyone can check that a published kernel was built
from the published sources — build provenance doubles as GPL transparency ("corresponding source" = the commit the
attestation names); a stolen account password alone cannot produce a valid attestation for a kernel built
elsewhere, only for one built by the workflow from a pushed commit (which is visible in the repository); forks get
the same scheme by copying the workflow.

**Costs and limits.** CI build time (a full kernel build is about 30–60 min on a 4-core hosted runner; caching the
build directory between runs helps; public repositories get the runners free); the workflow has to reproduce the
local build exactly (toolchain pinned by container image digest, `KBUILD_BUILD_TIMESTAMP`/`USER`/`HOST` fixed so the
banner is reproducible); development kernels for the maintainer's tablet stay local (with dev firmware, never
published). The trust moves from "the maintainer's account" to "the repository's workflow and its protected
branches/tags": someone with push access can still change the workflow — branch protection and required reviews
matter more. The verifier adds code and a trust root that needs updates (Sigstore root rotation) to the helper; an
outdated helper then fails closed (and says so). Verification needs the attestation API (one more call, rate
limit) — the bundle can instead be attached to the release as an asset (`*.sigstore.json`) to keep it one place.

## 4. Helper self-update

**Decided (maintainer, 2026-10-03):** the helper updates **itself through GitHub Releases only** — no apt or
pacman repositories, no COPR. The earlier plan of this section (the helper stays a package everywhere; apt and
pacman repositories with an OpenPGP key, COPR, a minisign-signed tarball for SteamOS only) is dropped: none of
those channels existed, so the update commands the helper showed did not work anywhere, and the repositories
would have brought back a signing key that 3.8 removed.

As built (0.3.0; the user-facing description is [docs/helper.md "Helper updates"](../helper.md#helper-updates)):

- A `helper-vX.Y.Z` release carries `tb323fu-helper-X.Y.Z-aarch64.tar.gz` (`MANIFEST` + `root/<path>`, the
  `PREFIX=/usr/local` tree of `install.sh`, the settings app and the GNOME extension) and `SHA256SUMS`; other
  assets (Debian packages) are ignored. `tools/helper-release.py` makes and checks the set; it is built on the
  Debian 13 root (the oldest glibc), and `MANIFEST` records `min_glibc`.
- Who owns the running daemon decides (`dpkg-query -S`, `pacman -Qo`, `rpm -qf`, `/etc/NIXOS`, `/nix/store`), not
  `os-release`: owned → notify only, with that system's way; nobody → self-update.
- Same channel and checks as kernels (fetch unit, `SHA256SUMS`, GitHub digest, optional minisign), plus a fixed
  allow-list of install paths, so a release can never write anything else (layer 1 never).
- The swap runs outside the daemon's sandbox in a transient unit (the daemon binary with `--apply`): snapshot of
  every touched path, per-file atomic renames, daemon-reload, D-Bus reload, restart, health check (new `Version`
  on the bus within 30 s, polkit knows the actions), automatic restore of the snapshot otherwise; `Rollback`
  re-applies the kept snapshot. polkit `helper-update` (`auth_admin_keep`, as a kernel install).
- Not an overlay (versions under `/var/lib` with unit drop-ins, or a systemd-sysext image): the files go
  where `install.sh` puts them, so a self-updated system looks exactly like a manual install of that version, and
  a later `install.sh` or package simply replaces them.
- D-Bus rule: additive changes only within `OpenDeviceHelper1`; a breaking change gets `OpenDeviceHelper2`.
- Open: attestation-based verification (3.9) should be required for helper updates before kernels; releases built
  by CI; a `min_updater` line if the tarball format ever changes; the helper of another root in a multiboot setup is
  not updated with this one.

`min_helper` / `min_platform` in a kernel manifest let the helper refuse (with a clear message) a kernel that
needs newer layer-1 files than the root has — e.g. a kernel that renames a sysfs knob the UCM or udev rules
use.

## 5. Migration from today's layout

1. **Build pipeline** (no device): unique `LOCALVERSION`, modules squashfs with build-time `depmod` and
   `extra/` aw882xx, `kernel/initramfs/build.sh -m`; keep producing the plain modules tarball for `own`-mode
   roots. Measure the image size (M0).
2. **Initramfs**: mount + move of the modules image, `modules=own|overlay` opt-out, trial counter and
   rollback, `modprobe` for the early modules. Old per-root trees keep working: an old image without a
   modules file boots as today, and the new release string never collides with the old tree.
3. **Platform package**: `tb323fu-kernel-confirm.{service,timer}` and its script; the KernelSU action's
   `linux-good` rule; NixOS module gets the same unit.
4. **NixOS**: `prebuilt-kernel.nix` stub mode, `rootfs/nixos` without the modules copy; one last rebuild of
   the existing NixOS root to switch to the stub.
5. **Helper**: `Kernel` object, fetch unit and timer, polkit actions, CLI, app pages; `RootHealth` checks
   `SharedModules` for the current root and `own` mode for others instead of `modules.dep`/`extra/`.
6. **Release procedure** (3.8): `release-public.sh` builds the asset set (`Image-tb323fu-tNN`, `.gz`,
   `SHA256SUMS`, notes from the template), `gh release create kernel-tNN … --prerelease` publishes it for the testing
   channel (checked on the maintainer's tablet), then the pre-release is turned into a release for `stable`. The
   repository has to be public for downloads without a token; until then a token credential or a local stand-in
   (`kernel-release.py fake-api`, `kernel.api_url`) is used for testing.
7. **Cleanup**: once no saved image (`linux-current`, `linux-good`, Android's staged copy) uses the old
   release, the helper offers to delete the roots' old `/lib/modules/<old release>` trees (it shows the size).
   The manual roll-out scripts become rollback helpers for old kernels only.

## 6. Testing plan

Short device steps, each with a time estimate; everything else is done without the tablet.

| Step | Where | What | Time |
|---|---|---|---|
| M0 | build machine | build tNN with the modules image; sizes of `Image`, initramfs, squashfs (xz and lz4), total vs 96 MiB; `unsquashfs -l` matches the tree; `depmod` output present | agent, ~1 h |
| T1 | PC/container | the init mount/move logic against fake roots (Debian merged-`/usr`, Arch, Fedora, NixOS without `/lib`, read-only root, `own` and `overlay` markers) in a chroot with busybox; trial counter transitions (0 → 1 → 2 → rollback; confirm clears) on a fake `boot_a` file | agent, ~2 h |
| T2 | PC | helper: `Kernel` object against the fake sysfs root and a fake `boot_a`/`boot_b` (files): manifest/index signature with a test key, expired index, downgrade refusal, read-back mismatch → restore, polkit denials; CLI golden output | agent, ~3 h |
| M1 | tablet, Debian | flash tNN: boots, `findmnt` shows the squashfs, sound card + both amplifiers, Wi-Fi, touch, netfilter modules load, boot time vs previous kernel | **10 min**, 1 reboot |
| M2 | tablet, each root | `tb323fu-ctl boot reboot` through Ubuntu, Arch, Fedora, NixOS (stub generation), SteamOS: failed units, sound card, Wi-Fi, `modinfo -n snd-soc-aw882xx` path; NixOS: kmod fall-through, `MODULE_DIR`; Fedora: SELinux label question | **20 min**, 5 reboots |
| M3 | tablet | Android round trip: emergency chord → Android → KernelSU action → same image back, modules match | **5 min**, 2 reboots, a person holds the keys |
| M4 | tablet | trial and rollback: install tNN+1 with the confirm timer masked on the default root → boots twice unconfirmed → the initramfs restores `linux-good` → `LastFailed` shown; then install again normally → confirmed after 90 s | **15 min**, 4 reboots |
| M5 | tablet | helper end to end with a PC-served index: check → download → install → reboot → confirm; app pages (a person looks) | **10 min**, 1 reboot |

Not testable safely: a kernel that dies before `/init` (documented manual recovery only).

## 7. Risks

- **`boot_a` space.** 96 MiB is fixed. The kernel grows, more modules get enabled. Mitigations: measure each
  release (the build fails above, say, 88 MiB), xz instead of lz4, prune modules nobody on this SoC can load,
  and option (a) as the fallback with the same mount code.
- **Early-boot regressions are not caught automatically** (3.5). Mitigation: the testing channel, and a
  `stable` release only after M1–M2 on real hardware.
- **Power loss during the `boot_a` write.** About one second; battery check first; EDL remains (fastboot is read-only).
- **NixOS depends on nixpkgs' kmod search order.** If nixpkgs drops the `/lib/modules` fall-through, NixOS
  loses its modules after a NixOS update — the M2 check becomes a release check, and the activation-snippet
  alternative (2.5) is ready to implement.
- **Read-only modules surprise someone** (DKMS, manual `depmod`). `own`/`overlay` modes, documented.
- **Unswappable RAM** for the image in the initramfs (≈ 10 MiB) — accepted.
- **Publishing account compromise** (3.8): whoever controls the account or a token with release rights can publish a
  kernel that helpers install (stable after their 90 s, or with Keep). Mitigations: the account rules in 3.8, the
  trial and rollback (a kernel that does not boot goes back by itself — a malicious one that boots does not), and
  plan 2 (3.9). (Before 3.8: signing key loss or leak, rotation through helper releases.)

## 8. Open questions

- **Q1 — firmware for built-in drivers in a firmware-free release image.** Adreno, Wi-Fi (ath12k),
  Bluetooth and touch are built in and request firmware at probe, before any root is mounted; today their
  firmware sits in the initramfs. Options: build them as modules in release kernels (loaded after
  `switch_root`, firmware from the root — but the initramfs' panel summary then has no display), or keep
  them built in and re-probe after the root is mounted. This decides what a release kernel looks like and
  is a prerequisite for publishing images at all.
  *Measured 2026-10-02* (firmware-free initramfs, otherwise the development kernel, Debian; table in
  [kernel/initramfs/README.md](../../kernel/initramfs/README.md#without-firmware-release-images)): the GPU and
  Bluetooth fail in the initramfs but **recover on their own** after `switch_root` (the GPU driver loads its
  firmware on first open, Bluetooth re-runs setup at power-on); touch is a module and is now left to the root's
  udev when the image has no firmware; **only Wi-Fi stays down** (built-in ath12k, probe fails for good, a
  manual bind after boot brings it up). So the answer is per driver, not "everything as modules": either
  `CONFIG_ATH12K=m` in release kernels (untested; udev loads it after `switch_root`) or a late bind of the
  Wi-Fi device in the platform files (the manual bind is verified). Panel and boot summary are unaffected.
  **Decided and verified 2026-10-02:** `CONFIG_ATH12K=m` in `kernel/config/baldur.fragment` (and patch 0055's
  copy). On a release build from the public series (rc1) udev loaded `ath12k_wifi7` from the root at 4 s, the
  interface was up and connected by itself; GPU, Bluetooth, touch (taps seen), the sound card with both amplifiers
  and the emergency key's hash from the root all worked.
- **Q2 — `boot_a` size**: the M0 numbers; and whether the bootloader accepts a compressed kernel (`Image.gz`
  / `Image.lz4`) in the boot image, which would free 20+ MiB.
  **Answered 2026-10-02: gzip yes, LZ4 no.** The bootloader's LinuxLoader (decompressed from `abl_a`) has a gzip
  decompressor ("the input data is not a gzip package", "Decompressing kernel image …") and no LZ4 code or magic.
  A development kernel repacked as `Image.gz` (57.2 MB → 21.1 MB, `gzip -9`) booted normally, with no measurable
  delay (reboot to SSH 20 s against 22 s raw). LZ4 was not tried on the device (no decoder; a failed boot needs
  EDL). Headroom in the 96 MiB `boot_a`: the release kernel (30.0 MB raw, firmware-free initramfs) is 13.8 MB as
  gzip, leaving about 86 MB; the development kernel with firmware leaves about 79 MB. `boot-repack-kernel.py`
  accepts `Image.gz` and refuses LZ4.
- **Q3 — watchdog after a normal boot**: does the bootloader leave the hardware watchdog armed when it starts
  `boot_a` directly (it does on the kexec path)? If not, a hang after `/init` but before the initramfs feeds
  it is only a hang, not a counted try.
- **Q4 — the bootloader's A/B retry bits** (`tries_remaining`/`successful` in the GPT attributes of `boot_a`):
  could they give an automatic fallback for kernels that die before `/init`? Slot `_b` is not a working
  Android slot here, so this needs study before anyone touches it.
  **Decided (2026-10-02): not used.** The fallback is the initramfs' trial counter; a kernel that dies before
  `/init` is never counted and needs EDL (fastboot is read-only on this bootloader; 3.5). Seen 10-03: a dev kernel (t33, pseudo-NMI) hung before the initramfs on every start.
- **Q5 — `overlay` mode**: worth building now, or wait until someone needs DKMS?
- **Q6 — where `kernel-state` and the saved images live on SD-only setups** (no `baldur-root`): **decided
  (2026-10-02): on the state root** — the first present of `baldur-root`, `baldur-root-sd`, then the
  `tb323fu-*` partitions in sorted order. The initramfs, the helper, `back-to-android` and the KernelSU action
  already pick the boot selection and `linux-current.img` by that rule; `linux-good.img` and the trial record
  follow it (read "the UFS root" in this note as "the state root", mounted under `/run/tb323fu/state`).
- **Q7 — confirmation criterion**: 90 s after `graphical.target` on any root — or only on the default root, or
  with a user-visible "keep this kernel" prompt for the testing channel?
  **Decided (2026-10-02):** stable channel — confirmed once a system on **any** root reached its target
  (`graphical.target`, else `multi-user.target`) and stayed up 90 s more; testing channel — only when the user
  presses **Keep** (settings app, `tb323fu-ctl kernel keep`); without it the kernel goes back after its third start.
