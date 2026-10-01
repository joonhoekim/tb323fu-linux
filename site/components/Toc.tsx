// SPDX-License-Identifier: MIT
import type { TocItem } from "@/lib/markdown";

function List({ items }: { items: TocItem[] }) {
  return (
    <ul>
      {items.map((t) => (
        <li key={t.id} className={t.depth === 3 ? "toc-sub" : undefined}>
          <a href={`#${t.id}`}>{t.text}</a>
        </li>
      ))}
    </ul>
  );
}

export function TocAside({ items }: { items: TocItem[] }) {
  if (items.length < 2) return null;
  return (
    <aside className="toc-aside">
      <nav aria-label="On this page" className="toc">
        <h2 className="toc-title">On this page</h2>
        <List items={items} />
      </nav>
    </aside>
  );
}

export function TocInline({ items }: { items: TocItem[] }) {
  if (items.length < 2) return null;
  return (
    <details className="toc-inline">
      <summary>On this page</summary>
      <nav aria-label="On this page (inline)" className="toc">
        <List items={items.filter((t) => t.depth === 2)} />
      </nav>
    </details>
  );
}
