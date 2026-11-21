# Building and installing your own kernel

For contributors: build a kernel from this repository, install it with the helper (the same trial and automatic
rollback as an official update), and publish it on your fork's GitHub Releases so others can get it through their
helper.

Background: [helper.md "Kernel updates"](helper.md#kernel-updates) (what the helper checks and does),
[the manual install, step 3](install-manual.md#3-the-linux-boot-image) (the boot image), and the design note
[kernel-updates-design.md](notes/kernel-updates-design.md).

> **Status.** The helper side (`install-local`, the GitHub Releases channel, trial boot, Keep and rollback) is
> **verified** ([labels](hardware-status.md#evidence-labels)). The build commands are the ones in
> [the manual install, 3b](install-manual.md#3b-build-it-yourself): **verified**.

## 1. Build

Follow [the manual install, 3b](install-manual.md#3b-build-it-yourself), with two things that matter for the helper:

- **A release name of your own, unique per build:** `CONFIG_LOCALVERSION="-tb323fu-<something>"` and
  `make LOCALVERSION=` (no `+`). The release string tells modules and kernels apart. To publish through GitHub
  Releases (step 3) it must end in `-tb323fu-tNN`; `NN` is the serial the helper compares. A release is offered only
  when its serial is higher than the running kernel's, so a fork's serials must exceed those of the kernels its
  users run. For a kernel you only install yourself any name works, e.g. `-tb323fu-tNN-speakerfix`.
- **Shared modules:** build the initramfs with `kernel/initramfs/build.sh -m mods/lib/modules/<release>`. All modules
  go into the boot image as `/lib/modules/<release>.sqfs`, and the initramfs mounts them on every root it boots.
  Without `-m` each root needs its own copy of exactly these modules (`own` mode in `/etc/tb323fu/modules`); the
  helper warns about such a kernel before installing it.

What you install is the kernel **`Image`** (`out/arch/arm64/boot/Image`, initramfs built in) or a gzip of it
(`gzip -9 -n`); the bootloader decompresses gzip, not LZ4. A boot image (`build-boot.sh` output) works too: the
helper takes only its kernel.

## 2. Install it on your tablet

Copy the `Image` (or `Image.gz`) to the tablet, then:

```sh
tb323fu-ctl kernel inspect Image.gz          # release, banner, shared modules or not, warnings
sudo tb323fu-ctl kernel install-local Image.gz --name "speaker fix" --reboot
```

or in Open Device Helper: **About → Kernel Updates → Install Kernel from File…** (same information, asks before
installing). It is installed like an official update (repacked into your stock boot image, trial, automatic rollback;
[helper.md → Kernel updates](helper.md#kernel-updates)), with these differences:

- By default (`--trial`) only **Keep** confirms the kernel: `tb323fu-ctl kernel keep`, or the banner in the app. With
  `--keep` it is kept by itself once a system has run 90 s with it.
- `tb323fu-ctl kernel` shows the trial (`trial … "speaker fix" (a local file, start 1 of 2)`), and
  `tb323fu-ctl kernel rollback --reboot` goes back at any time, before or after confirming.
- Installing from a file asks for the administrator's password **every time**: nothing but you vouches for the file.

A kernel that stops **before** its initramfs runs (no boot log on the panel at all) is not caught by the trial; the way
back is EDL ([recovery.md](recovery.md#linux-does-not-boot)). Do not force-restart such a kernel over and over: repeated
failures can make the bootloader switch to slot `_b`, which does not boot.[^why-nmi]

## 3. Publish it on your fork

A kernel release on GitHub Releases has this asset set (the helper ignores releases without it):

| Asset | |
|---|---|
| `Image-tb323fu-tNN` | the raw `Image`, release string ending in `-tb323fu-tNN` |
| `Image-tb323fu-tNN.gz` | optional, `gzip -9 -n` of it; the helper downloads this one when it is there |
| `SHA256SUMS` | `sha256sum` of every file of the release |
| `SHA256SUMS.minisig` | optional, for helpers with `require_signature = true` |

[`tools/kernel-release.py`](../tools/kernel-release.py) (Python 3, standard library) prepares and checks the set:

```sh
python3 tools/kernel-release.py assets out-tNN linux-tb323fu/out/arch/arm64/boot/Image --gzip
python3 tools/kernel-release.py check out-tNN
gh release create kernel-tNN out-tNN/* --repo YOU/tb323fu-linux --title "tb323fu-linux tNN" \
    --notes-file notes.md [--prerelease]
```

- The release notes are the release body (Markdown) and appear in the app. A line
  `<!-- tb323fu: min_helper=X.Y.Z min_platform=X.Y.Z -->` makes older helpers refuse the kernel with a clear message.
- A **pre-release** is offered only on the `testing` channel and always waits for Keep; turning it into a release
  offers it on `stable` too.
- Ship the GPL sources with it, as the project's releases do: the patch series and configuration (`config-…`) the
  kernel was built from, and the out-of-tree module sources. The public release script also adds the initramfs file
  list and the BusyBox configuration; the helper ignores assets it does not know.

Users of your fork set, in `/etc/tb323fu/helper.toml`:

```toml
[kernel]
source = "github:YOU/tb323fu-linux"
channel = "testing"        # if you publish pre-releases
```

then `sudo tb323fu-ctl reload` and `tb323fu-ctl kernel check`. The helper reads public releases without a token. A
**private** fork needs a fine-grained token with read-only access to that repository's contents, in
`/etc/credstore/tb323fu-github-token` (root, mode 600); only the download unit reads it.

**What users of your fork trust:** `SHA256SUMS` only protects against damaged downloads. Whoever can publish releases
in your repository decides what their tablets run: use two-factor authentication and keep tokens that can write
releases out of CI and off shared machines. If you sign `SHA256SUMS` with minisign (`kernel-release.py sign out-tNN
--key your.key` before uploading), users can set `require_signature = true` and `public_keys = ["RW…your public key…"]`.

## 4. Testing a release without publishing

`kernel-release.py fake-api` builds a local stand-in of the GitHub API from the same asset directory, so the whole
check → download → install path can be tried before anything is public:

```sh
python3 tools/kernel-release.py fake-api /srv/api --repo YOU/tb323fu-linux --dir out-tNN --tag kernel-tNN \
    --prerelease --base http://127.0.0.1:8000
(cd /srv/api && python3 -m http.server -b 127.0.0.1 8000) &
# helper.toml: [kernel] source = "github:YOU/tb323fu-linux"  api_url = "http://127.0.0.1:8000"  channel = "testing"
sudo tb323fu-ctl reload && tb323fu-ctl kernel update
```

`file:///srv/api` works as `api_url` too, with `--base file:///srv/api`; the download unit then needs to read that
directory. Set `api_url` back to `https://api.github.com` afterwards.

[^why-nmi]: Seen on the development tablet: a test kernel with pseudo-NMI enabled hung before its initramfs on every start.
