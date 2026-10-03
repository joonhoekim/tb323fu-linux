# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub's
[private vulnerability reporting](https://github.com/joonhoekim/tb323fu-linux/security/advisories/new)
(Security → Report a vulnerability), not in a public issue. Say what is affected (file, release tag or commit),
how to reproduce it, and what an attacker gains. This is a one-person community project: expect an answer within
about two weeks; a fix ships with the next kernel or helper release.

## Scope

In scope:

- **Open Device Helper** (`helper/`): the `tb323fu-helperd` system D-Bus service, its D-Bus policy and polkit
  actions (`io.github.joonhoekim.opendevicehelper.*`), and what it writes to sysfs, `/etc/tb323fu` and the boot
  partitions — for example a local user getting a privileged action without the authentication the policy asks for.
- **Kernel updates**: `tb323fu-kernel-fetch`, the checks before `boot_a` is written, and the trial and rollback
  logic. Note the design limit: `SHA256SUMS` comes from the same GitHub release as the kernel, so it detects damaged
  downloads, not a replaced release; signatures are optional (`require_signature`, see [docs/helper.md](docs/helper.md)).
- **The built-in initramfs** (`kernel/initramfs/`): its USB console (serial `ttyGS0` and telnet on `usb0`) takes no
  password while it runs; which builds enable it is described in
  [kernel/initramfs/README.md](kernel/initramfs/README.md). Someone with the tablet and a USB cable during boot is
  outside what this project can protect against (there is no disk encryption), but a way to reach that console
  without physical access is in scope.
- Platform files (`userspace/platform/`): udev rules and units that grant device access.

Out of scope: the upstream Linux kernel, Lenovo's or Qualcomm's firmware and bootloader, the rooting tools named in
the documentation (LTBox, KernelSU), and distributions' own packages. Please report those to their maintainers.
