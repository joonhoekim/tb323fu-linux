// SPDX-License-Identifier: MIT
import fs from "node:fs";
import path from "node:path";
import Link from "next/link";
import { REPO_ROOT, REPO_URL, githubUrl } from "@/lib/config";
import { STATUS_META, getDistros, getHardwareSummary, type StatusKey } from "@/lib/facts";
import { findDocByFile } from "@/lib/docs";

const ORDER: StatusKey[] = ["works", "partial", "broken", "unverified", "na"];

/** The README's status warning (the first "> **Status: ...**" blockquote), as plain text. */
function readmeStatus(): { head: string; body: string } {
  const fallback = {
    head: "Work in progress — not ready for everyday use.",
    body: "The install guide was followed end to end on the development tablet; the kernel and helper releases are on GitHub Releases.",
  };
  try {
    const text = fs.readFileSync(path.join(REPO_ROOT, "README.md"), "utf8");
    const m = /^>\s*\*\*Status:\s*(.+?)\*\*\s*(.*(?:\n>.*)*)/m.exec(text);
    if (!m) return fallback;
    const head = m[1].trim();
    const body = m[2]
      .split("\n")
      .map((l) => l.replace(/^>\s?/, ""))
      .join(" ")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/[*`]/g, "")
      .replace(/\s+/g, " ")
      .trim();
    return { head: head[0].toUpperCase() + head.slice(1), body };
  } catch {
    return fallback;
  }
}

function docRoute(file: string): string | null {
  return findDocByFile(file)?.route ?? null;
}

export default function Home() {
  const status = readmeStatus();
  const hw = getHardwareSummary();
  const distros = getDistros();

  const tree: { name: string; file?: string; text: string }[] = [
    { name: "kernel/", file: "kernel/README.md", text: "Patch series on a fixed upstream base, board device tree, config, and the built-in initramfs." },
    { name: "firmware/", file: "firmware/README.md", text: "Manifest and an extraction script — firmware comes from your own tablet, none is stored." },
    { name: "rootfs/", file: "docs/distros.md", text: "Optional builders for root filesystems on the multiboot partitions." },
    { name: "userspace/", file: "userspace/platform/README.md", text: "Distribution-neutral platform files: udev, systemd units, audio, sensors, emergency key." },
    { name: "helper/", file: "helper/README.md", text: "Open Device Helper: daemon, CLI and settings app for the tablet's own knobs." },
    { name: "packaging/", text: "Debian, Arch and Nix recipes." },
  ];

  return (
    <main id="main" className="home">
      <section className="hero" aria-labelledby="hero-title">
        <p className="eyebrow">Lenovo Legion Tab Gen 5 · Legion Y700 5th Gen · TB323FU</p>
        <h1 id="hero-title">Mainline Linux on the TB323FU</h1>
        <p className="lead">
          A port of the mainline Linux kernel to Lenovo&apos;s gaming tablet with the Snapdragon 8 Elite Gen 5
          (SM8850 &ldquo;kaanapali&rdquo;, board &ldquo;baldur&rdquo;). The <strong>kernel port is the core</strong>: a patch
          series, the board device tree and the config. Distributions and desktop integration are optional add-ons.
        </p>
        <div className="status-badge" role="note">
          <span className="status-dot" aria-hidden="true" />
          <div>
            <strong>{status.head}</strong> {status.body}
          </div>
        </div>
        <div className="cta">
          <Link className="button primary" href="/docs/">
            Read the docs
          </Link>
          <Link className="button" href="/docs/hardware-status/">
            Hardware status
          </Link>
          <a className="button" href={REPO_URL}>
            Source on GitHub
          </a>
        </div>
      </section>

      <section aria-labelledby="facts-title" className="section">
        <h2 id="facts-title" className="section-title">
          At a glance
        </h2>
        <dl className="facts">
          {hw?.kernel && (
            <div className="fact">
              <dt>Kernel</dt>
              <dd>
                mainline Linux <strong>{hw.kernel}</strong> + this project&apos;s patch series
              </dd>
            </div>
          )}
          {hw && hw.total > 0 && (
            <div className="fact">
              <dt>Hardware</dt>
              <dd>
                <strong>{hw.totals.works}</strong> of {hw.total} listed features work, checked on the device
              </dd>
            </div>
          )}
          {distros.length > 0 && (
            <div className="fact">
              <dt>Distributions</dt>
              <dd>
                <strong>{distros.length}</strong> booted to a desktop from their own partitions
              </dd>
            </div>
          )}
          <div className="fact">
            <dt>SoC</dt>
            <dd>
              Snapdragon 8 Elite Gen 5, <strong>SM8850</strong>
            </dd>
          </div>
        </dl>
      </section>

      {hw && hw.sections.length > 0 && (
        <section aria-labelledby="hw-title" className="section">
          <div className="section-head">
            <h2 id="hw-title" className="section-title">
              Hardware status
            </h2>
            <Link href="/docs/hardware-status/">Full table →</Link>
          </div>
          <p className="muted">
            A feature counts as working only when it was measured or seen on the device — a probing driver does not count.
            {hw.updated && <> Last updated {hw.updated}.</>}
          </p>
          <ul className="legend" aria-label="Legend">
            {ORDER.map((k) => (
              <li key={k}>
                <span className={`swatch s-${k}`} aria-hidden="true" />
                {STATUS_META[k].label}
              </li>
            ))}
          </ul>
          <div className="hw-sections">
            {hw.sections.map((s) => (
              <div className="hw-row" key={s.title}>
                <div className="hw-row-head">
                  <span className="hw-name">{s.title}</span>
                  <span className="hw-counts">
                    {ORDER.filter((k) => s.counts[k] > 0)
                      .map((k) => `${s.counts[k]} ${STATUS_META[k].label}`)
                      .join(" · ")}
                  </span>
                </div>
                <div className="bar" aria-hidden="true">
                  {ORDER.filter((k) => s.counts[k] > 0).map((k) => (
                    <span key={k} className={`seg s-${k}`} style={{ flexGrow: s.counts[k] }} />
                  ))}
                </div>
              </div>
            ))}
          </div>
        </section>
      )}

      {distros.length > 0 && (
        <section aria-labelledby="distros-title" className="section">
          <div className="section-head">
            <h2 id="distros-title" className="section-title">
              Distributions
            </h2>
            <Link href="/docs/distros/">Details →</Link>
          </div>
          <p className="muted">
            The kernel does not depend on a distribution. These have been built with the scripts in <code>rootfs/</code> and
            booted on the tablet, each from its own partition.
          </p>
          <ul className="distros">
            {distros.map((d) => (
              <li key={d.system}>
                <span className="distro-name">{d.system}</span>
                {d.boot && (
                  <span className="distro-boot" title="Boot to desktop (kernel + userspace, automatic login)">
                    {d.boot}
                    <span className="sr-only"> to the desktop</span>
                  </span>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      <section aria-labelledby="helper-title" className="section">
        <div className="section-head">
          <h2 id="helper-title" className="section-title">
            Open Device Helper
          </h2>
          <Link href="/docs/helper/">Details →</Link>
        </div>
        <p className="muted">
          The tablet&apos;s own settings, in a GNOME quick-settings tile, a settings app and a command line
          (<code>tb323fu-ctl</code>), over one D-Bus service:
        </p>
        <ul className="helper-list">
          <li>Charge limit, bypass charging and a &ldquo;full by&rdquo; schedule; battery health and the charger&apos;s power</li>
          <li>Refresh policy and panel heat protection</li>
          <li>Performance profiles with CPU and GPU limits, CPU boost and thermal profiles</li>
          <li>Torch, LED ring colors and effects, vibration strength</li>
          <li>USB-C port roles and wake sources, the emergency key</li>
          <li>Restart into Android, choose the system to boot</li>
          <li>Kernel updates from this project&apos;s releases, tried on the next start and rolled back if it fails</li>
        </ul>
      </section>

      <section aria-labelledby="tree-title" className="section">
        <h2 id="tree-title" className="section-title">
          In the repository
        </h2>
        <ul className="tree">
          {tree.map((t) => {
            const route = t.file ? docRoute(t.file) : null;
            const href = route ?? githubUrl(t.name, true);
            const inner = (
              <>
                <code>{t.name}</code>
                <span>{t.text}</span>
              </>
            );
            return (
              <li key={t.name}>
                {route ? <Link href={href}>{inner}</Link> : <a href={href}>{inner}</a>}
              </li>
            );
          })}
        </ul>
        <p className="muted">
          Licensing: <a href={githubUrl("LICENSE")}>LICENSE</a> and the README&apos;s{" "}
          <Link href="/docs/#license">license section</Link>.
        </p>
      </section>
    </main>
  );
}
