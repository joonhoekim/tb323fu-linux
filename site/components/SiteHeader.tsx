// SPDX-License-Identifier: MIT
import Link from "next/link";
import ThemeToggle from "./ThemeToggle";
import { REPO_URL, SITE_NAME } from "@/lib/config";

export default function SiteHeader() {
  return (
    <header className="site-header">
      <div className="site-header-inner">
        <Link href="/" className="brand">
          <svg aria-hidden="true" viewBox="0 0 32 32" width="22" height="22">
            <rect x="5" y="3" width="22" height="26" rx="4" fill="none" stroke="currentColor" strokeWidth="2.5" />
            <circle cx="16" cy="24.5" r="1.6" fill="currentColor" />
          </svg>
          <span>{SITE_NAME}</span>
        </Link>
        <nav aria-label="Main" className="main-nav">
          <Link href="/docs/">Docs</Link>
          <Link href="/docs/hardware-status/">Hardware</Link>
          <Link href="/docs/distros/" className="nav-optional">
            Distros
          </Link>
          <a href={REPO_URL}>GitHub</a>
        </nav>
        <ThemeToggle />
      </div>
    </header>
  );
}
