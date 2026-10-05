// SPDX-License-Identifier: MIT
import Link from "next/link";
import ThemeToggle from "./ThemeToggle";
import { REPO_URL, SITE_NAME } from "@/lib/config";

export default function SiteHeader() {
  return (
    <header className="site-header">
      <div className="site-header-inner">
        <Link href="/" className="brand">
          <svg aria-hidden="true" viewBox="0 0 12 12" width="24" height="24" shapeRendering="crispEdges">
            <path fill="currentColor" d="M0 2h12v1h-12zM0 3h3v1h-3zM11 3h1v1h-1zM0 4h3v1h-3zM4 4h1v1h-1zM11 4h1v1h-1zM0 5h3v1h-3zM5 5h1v1h-1zM11 5h1v1h-1zM0 6h1v1h-1zM2 6h1v1h-1zM6 6h1v1h-1zM11 6h1v1h-1zM0 7h3v1h-3zM5 7h1v1h-1zM11 7h1v1h-1zM0 8h3v1h-3zM4 8h1v1h-1zM7 8h3v1h-3zM11 8h1v1h-1zM0 9h3v1h-3zM11 9h1v1h-1zM0 10h12v1h-12z" />
          </svg>
          <span>{SITE_NAME}</span>
        </Link>
        <nav aria-label="Main" className="main-nav">
          <Link href="/docs/">Docs</Link>
          <Link href="/docs/install/">Install</Link>
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
