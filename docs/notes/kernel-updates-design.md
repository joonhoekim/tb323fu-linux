# Kernel updates through the helper — design

> **Design note, not implemented.** Nothing here exists yet; it describes how kernel updates and kernel
> modules should work once the project publishes boot images. `docs/notes/` is not rendered on the
> project site (the site picks up `docs/*.md` only). Status: draft for review, 2026-10-02.

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

## 3. Update flow through the helper

Firmware is **never** shipped: release images carry no vendor firmware; users extract it from their own
tablet ([firmware/](../../firmware/)). The release image must therefore boot with the firmware that is in the
root only — see open question Q1.

### 3.1 Release artefacts

One GitHub Release per kernel build, tag `kernel-tNN`:

| Asset | What |
|---|---|
| `tb323fu-boot-tNN.img.zst` | the 96 MiB boot image (kernel + DT + initramfs + modules image), zstd (padding compresses away) |
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
   `/var/lib/tb323fu/linux-good.img` (+ `.sha256`) on the UFS root (`baldur-root`, mounted under
   `/run/tb323fu/ufs` when running from another root, as the `Boot` object already does), via a `.tmp` file,
   hash check, `rename`, `sync`.
3. Write the trial record (3.5) on the same root, `sync`.
4. `dd` the verified image to `boot_a` (`conv=fsync`), drop caches, read `boot_a` back and compare the
   SHA-256. On a mismatch: write `linux-good.img` back, verify, delete the trial record, report the error.
   A 96 MiB write takes about a second on UFS; there is no atomic way to replace a partition, so the
   power check in step 1 and the read-back are the protection (a half-written `boot_a` still leaves
   fastboot and EDL, section 3.6).
5. Reboot now or later (the user's choice). Until the reboot, `Kernel.Pending` shows the installed tag.

### 3.5 Boot-success marker and automatic fallback

State lives in `/var/lib/tb323fu/kernel-state` on the UFS root (the root Android can read):

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
  then. The ways out stay: the emergency chord needs the initramfs too, so for this case it is fastboot
  (volume down + power) with `fastboot flash boot_a linux-good.img` from a PC, or EDL. The `testing` channel
  exists so that this is caught on the maintainer's tablet first.
- **Through Android:** the KernelSU action today writes `linux-current.img` (the image that ran last). It
  gains one rule: if `kernel-state` has an unconfirmed `trial` or a `failed=` equal to the saved image's
  release, it writes `linux-good.img` instead and says so. `back-to-android` is unchanged (it still saves
  the running image as `linux-current.img`). The emergency chord stays as is.

### 3.6 D-Bus, polkit, CLI, UI

New object `/io/github/joonhoekim/tb323fu/Helper/Kernel`, interface `…Helper.Kernel`:

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

## 4. Helper self-update

The helper stays a normal package; it never replaces its own binaries on systems with a package manager.
It **notifies**: the same signed index carries `helper.latest` (version, notes URL); `Helper.Version` older
than that shows a row in About with the command for this system (chosen from `os-release` `ID`/`ID_LIKE`).

| System | Channel | Notes |
|---|---|---|
| Debian, Ubuntu | apt repository on GitHub Pages (`apt-ftparchive` or `reprepro`, arm64), `deb [signed-by=/usr/share/keyrings/tb323fu.gpg] https://joonhoekim.github.io/tb323fu-linux/apt stable main`; `.deb` files also attached to GitHub Releases | needs an OpenPGP key for the repository (apt cannot use minisign); `tb323fu-platform` installs the keyring |
| Arch Linux ARM | a pacman repository `[tb323fu]` (signed `repo-add` database, packages on Releases/Pages), plus the PKGBUILD in the AUR | same OpenPGP key |
| Fedora | COPR project (aarch64 chroots), built from a spec in `packaging/` | COPR signs with its own key |
| NixOS | the flake: `nix flake update tb323fu-linux` + `nixos-rebuild` | the notification shows that command |
| SteamOS and other image-based systems | no package path. `tb323fu-ctl self-update` (explicit, admin auth) downloads a minisign-signed tarball (same key as the kernel channel), installs into `/var/lib/tb323fu/helper/` with units in `/etc/systemd/system` | system image updates do not remove it (`/etc`, `/var` persist); documented as best effort |

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
6. **Release procedure**: signing key, index on the site, release-notes template, the testing channel on the
   maintainer's tablet before `stable`. The repository has to be public for unauthenticated downloads;
   until then the helper's index URL is configurable (`kernel.index_url` in `helper.toml`) so a PC-served
   index can be used for testing.
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
- **Power loss during the `boot_a` write.** About one second; battery check first; fastboot and EDL remain.
- **NixOS depends on nixpkgs' kmod search order.** If nixpkgs drops the `/lib/modules` fall-through, NixOS
  loses its modules after a NixOS update — the M2 check becomes a release check, and the activation-snippet
  alternative (2.5) is ready to implement.
- **Read-only modules surprise someone** (DKMS, manual `depmod`). `own`/`overlay` modes, documented.
- **Unswappable RAM** for the image in the initramfs (≈ 10 MiB) — accepted.
- **Signing key loss or leak.** Rotation through helper releases (3.2); a leak would need a helper release
  that revokes the key, so the key never leaves the maintainer's offline storage.

## 8. Open questions

- **Q1 — firmware for built-in drivers in a firmware-free release image.** Adreno, Wi-Fi (ath12k),
  Bluetooth and touch are built in and request firmware at probe, before any root is mounted; today their
  firmware sits in the initramfs. Options: build them as modules in release kernels (loaded after
  `switch_root`, firmware from the root — but the initramfs' panel summary then has no display), or keep
  them built in and re-probe after the root is mounted. This decides what a release kernel looks like and
  is a prerequisite for publishing images at all.
- **Q2 — `boot_a` size**: the M0 numbers; and whether the bootloader accepts a compressed kernel (`Image.gz`
  / `Image.lz4`) in the boot image, which would free 20+ MiB.
- **Q3 — watchdog after a normal boot**: does the bootloader leave the hardware watchdog armed when it starts
  `boot_a` directly (it does on the kexec path)? If not, a hang after `/init` but before the initramfs feeds
  it is only a hang, not a counted try.
- **Q4 — the bootloader's A/B retry bits** (`tries_remaining`/`successful` in the GPT attributes of `boot_a`):
  could they give an automatic fallback for kernels that die before `/init`? Slot `_b` is not a working
  Android slot here, so this needs study before anyone touches it.
- **Q5 — `overlay` mode**: worth building now, or wait until someone needs DKMS?
- **Q6 — where `kernel-state` and the saved images live on SD-only setups** (no `baldur-root`): use
  `baldur-root-sd` the same way, or require a UFS root for updates through the helper?
- **Q7 — confirmation criterion**: 90 s after `graphical.target` on any root — or only on the default root, or
  with a user-visible "keep this kernel" prompt for the testing channel?
