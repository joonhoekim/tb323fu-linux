// SPDX-License-Identifier: MIT
import Link from "next/link";
import { GROUP_LABELS, getDocs, type DocGroup } from "@/lib/docs";

function NavList({ current }: { current: string }) {
  const docs = getDocs();
  const groups = (Object.keys(GROUP_LABELS) as DocGroup[]).filter((g) => docs.some((d) => d.group === g));
  return (
    <>
      {groups.map((g) => (
        <div className="nav-group" key={g}>
          <h2 className="nav-group-title">{GROUP_LABELS[g]}</h2>
          <ul>
            {docs
              .filter((d) => d.group === g)
              .map((d) => (
                <li key={d.route} className={d.parent ? "nav-child" : undefined}>
                  <Link href={d.route} aria-current={d.route === current ? "page" : undefined}>
                    {d.title}
                    {d.group === "source" && <span className="nav-path">{d.file.replace(/\/README\.md$/, "/")}</span>}
                  </Link>
                </li>
              ))}
          </ul>
        </div>
      ))}
    </>
  );
}

export default function DocsSidebar({ current }: { current: string }) {
  return (
    <aside className="docs-sidebar">
      {/* wide screens: always open; narrow screens: a disclosure */}
      <nav aria-label="Documentation" className="docs-nav docs-nav-wide">
        <NavList current={current} />
      </nav>
      <details className="docs-nav-narrow">
        <summary>Documentation pages</summary>
        <nav aria-label="Documentation (menu)" className="docs-nav">
          <NavList current={current} />
        </nav>
      </details>
    </aside>
  );
}
