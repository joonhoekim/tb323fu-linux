# Mesa through the helper — design

> **Design note, not implemented yet (2026-10-06).** Decisions taken with the maintainer on 2026-10-06 are in
> section 2; the phases are in section 6. `docs/notes/` is not rendered on the project site.

## 1. The problem

The port's Mesa changes (patches being prepared for upstream: OpenCL and Vulkan compute, ir3 compiler) are worth
a lot on this GPU — Geekbench 6 Vulkan about 26.9k against 24.5k with Mesa main, Geekbench 7 OpenCL about 17.7k
against 14.7k ([performance](../performance.md)). Distributions will not carry them for a long time: Debian 13
gets no Mesa feature updates (backports 26.1.x), and even after the patches are merged upstream it takes months
to reach a release. The helper already delivers kernels to every root; it should deliver this Mesa too.

What makes Mesa different from a kernel or the helper itself:

- **The distribution owns Mesa's files** (`dpkg`/`pacman`/`rpm`); the channel must never overwrite them.
- **Installing a second Vulkan driver is not enough to use it.** Measured on Debian 13: an extra ICD manifest in
  `/etc/vulkan/icd.d`, under the distribution's file name or another, makes the loader enumerate the Adreno twice.
  Vulkan loader 1.4.309 has no driver selection in its settings file (`additional_drivers` is newer). The
  portable switch is the environment: `VK_DRIVER_FILES`, and for OpenCL `OCL_ICD_VENDORS` plus
  `RUSTICL_ENABLE=freedreno`.
- **ABI across roots.** Turnip needs only stable libraries (glibc, libdrm, xcb, wayland, zstd, expat, libstdc++).
  rusticl links LLVM 19, libclang-cpp 19 and the SPIR-V/LLVM translator, whose SONAMEs differ between Debian,
  Ubuntu, Arch and Fedora.
- **No boot counter covers it**, and a broken GL driver would take the desktop down with it.

## 2. Decisions (2026-10-06)

| Question | Decision |
|---|---|
| Which drivers | Phase 1: Vulkan (Turnip) and OpenCL (rusticl). Phase 2: GL (freedreno gallium: EGL vendor, GBM backend), once the trial and revert path of phase 1 has been exercised |
| How it is switched on | Both: a session-wide toggle (environment variables for desktop sessions and login shells) and a per-application wrapper `tb323fu-mesa run CMD` (testing, per-application comparisons) |
| ABI | One self-contained build, made on Debian 13 (the oldest glibc of the supported roots), LLVM, clang and the SPIR-V translator linked statically, libclc embedded (`static-libclc=all`); NixOS uses its flake |
| Where releases live | GitHub Releases of `tb323fu-linux`, tags `mesa-…`; the release list request asks for 100 entries so kernel releases do not fall out of the window |

## 3. Layout on the device

Everything the channel writes is under `/var/lib/tb323fu/mesa/` — writable on every root (SteamOS and Armada
included) and inside the daemon's `ReadWritePaths`, so no transient unit is needed to install:

```
/var/lib/tb323fu/mesa/
  versions/<version>/        unpacked release tree: lib/…, share/…, MANIFEST
  current -> versions/<v>    the version in use
  previous -> versions/<v>   kept for Rollback (one older version)
  icd/turnip.json            Vulkan ICD manifest pointing into current/
  icd/opencl/rusticl.icd     OpenCL vendor file pointing into current/
  env.conf                   KEY=VALUE lines, empty when switched off
  state                      key=value: enabled, trial, tries, max, keep, failed, good
  staged/<tag>/              download area (tarball, SHA256SUMS, verified)
```

`helper/install.sh` (and the packages) link `/etc/environment.d/60-tb323fu-mesa.conf` and
`/etc/profile.d/tb323fu-mesa.sh` to files under `/var/lib/tb323fu/mesa/`, once; afterwards the daemon only writes
under `/var/lib`. Switching off empties `env.conf`; the next login uses the distribution's drivers.

## 4. Trial, Keep and automatic revert

Mirrors the kernel trial, with the environment as the thing that is reverted:

1. Enabling a version (or installing a new one while enabled) writes `trial=<version> tries=0 max=2`.
2. `tb323fu-mesa-trial.service` runs once per boot before the display manager: with a trial pending it counts
   the start; when `tries` exceeds `max` it empties `env.conf`, records `failed=<version>` and leaves the
   previous state (`good`) in place.
3. **Keep** (settings app, `tb323fu-ctl mesa keep`) clears the trial. Phase 1 could confirm automatically after
   a health check (`vulkaninfo`/`clinfo` against the new driver); phase 2 (GL) needs the explicit Keep, because
   a broken compositor cannot press anything.
4. **Rollback** swaps `current` and `previous`.

## 5. Releases

`tools/mesa-release.py` (like `helper-release.py`): stages a `DESTDIR` install of the Mesa build, writes
`tb323fu-mesa-<version>-aarch64.tar.gz` with a `MANIFEST` (`format=1`, `version=`, `mesa_commit=`, `base=`
(the upstream Mesa commit), `min_glibc=`, `drivers=vulkan,opencl`, then `file SHA MODE SIZE PATH` lines) and
`SHA256SUMS`. The release body carries the patch list (with links to the upstream merge requests) and
`<!-- tb323fu: min_helper=… -->`. Built on the Debian 13 tablet; no CI yet.

## 6. Phases

1. Self-contained build (static LLVM/clang/translator, embedded libclc), checked on Debian, Ubuntu, Arch, Fedora.
2. `mesa-release.py`, a first `mesa-…` pre-release.
3. Helper: core (`mesa.rs`: parse, pick, download, verify, install, enable, trial state), daemon D-Bus object
   `…OpenDeviceHelper1.Mesa`, polkit actions, `tb323fu-ctl mesa …`, `tb323fu-mesa run`, the trial unit,
   `install.sh` links, tests with the fake API.
4. Settings app: a "Graphics drivers" group (version, toggle, Keep banner, Rollback).
5. GL (phase 2 above).
