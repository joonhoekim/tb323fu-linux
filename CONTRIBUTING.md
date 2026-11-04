# Contributing

Issues are for bugs you can reproduce; questions, ideas and hardware reports go to [Discussions](https://github.com/joonhoekim/tb323fu-linux/discussions).
Hardware reports ("this works / does not work on my tablet", in [Device reports](https://github.com/joonhoekim/tb323fu-linux/discussions/categories/device-reports))
are as useful as code; say which release or commit you ran, your firmware region and what you saw. Pull requests are welcome.

## Licensing of contributions (inbound = outbound)

A contribution is licensed under the license of the file it changes, as given by that file's SPDX line
(or by [NOTICE](NOTICE) and `REUSE.toml` for files without one). A new file takes the license of the component
it belongs to (for example GPL-3.0-or-later under `helper/`, MIT for platform files and tools, CC-BY-SA-4.0 for
documentation prose) and starts with an `SPDX-License-Identifier` line. There is no contributor license agreement.

## Developer Certificate of Origin

Issues, discussions and hardware reports need nothing of this; it is only for commits.

Some of the kernel patches here also go to the upstream Linux kernel, so this repository takes its sign-off rule
from the kernel's [submitting-patches](https://docs.kernel.org/process/submitting-patches.html#sign-your-work-the-developer-s-certificate-of-origin),
which upstream-bound patches follow in full.

Please add a `Signed-off-by:` line to every commit in a pull request, with the same name and e-mail as the commit's
author. It states that you may contribute the change under the file's license, as set out in the
[Developer Certificate of Origin 1.1](https://developercertificate.org/):

    Signed-off-by: Your Name <you@example.org>

We need a name you are known by, such as the one on your GitHub profile; it does not have to be your legal name.
`git commit -s` adds the line.

If the DCO check on your pull request fails, there is no need to open a new one. Add the line to the commits you have
already made and push them to the same branch:

    git rebase --signoff BASE        # BASE: the branch your pull request targets, for example origin/main
    git push --force-with-lease

Dependency updates opened by Dependabot are exempt: they change version numbers only.

## Names

Forks and modified builds have to use their own name, icons, IDs, update source and any signing key;
see [TRADEMARKS.md](TRADEMARKS.md).
