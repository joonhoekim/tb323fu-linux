# Contributing

Issues and pull requests are welcome. Hardware reports ("this works / does not work on my tablet") are as useful as code;
say which release or commit you ran and what you saw.

## Licensing of contributions (inbound = outbound)

A contribution is licensed under the license of the file it changes, as given by that file's SPDX line
(or by [NOTICE](NOTICE) and `REUSE.toml` for files without one). A new file takes the license of the component
it belongs to (for example GPL-3.0-or-later under `helper/`, MIT for platform files and tools, CC-BY-SA-4.0 for
documentation prose) and starts with an `SPDX-License-Identifier` line. There is no contributor license agreement.

## Developer Certificate of Origin

Every commit must carry a `Signed-off-by:` line with your real name, certifying the
[Developer Certificate of Origin 1.1](https://developercertificate.org/), as in the Linux kernel:

    Signed-off-by: Your Name <you@example.org>

`git commit -s` adds it. Kernel patches meant for upstream follow the kernel's
[submitting-patches](https://docs.kernel.org/process/submitting-patches.html) rules in addition.

## Names

Forks and modified builds have to use their own name, icons, IDs, update source and any signing key;
see [TRADEMARKS.md](TRADEMARKS.md).
