// SPDX-License-Identifier: MIT
"use client";

import { useEffect, useState } from "react";

// Show/hide the docs navigation (left) and the "On this page" contents (right)
// for a wider reading column. The choice is kept in localStorage and applied
// before the first paint by the script in app/layout.tsx (data-nav-hidden /
// data-toc-hidden on <html>).
type Panel = "nav" | "toc";
const ATTR: Record<Panel, string> = { nav: "navHidden", toc: "tocHidden" };
const KEY: Record<Panel, string> = { nav: "hide-nav", toc: "hide-toc" };

function setHidden(p: Panel, hidden: boolean) {
  const root = document.documentElement;
  if (hidden) root.dataset[ATTR[p]] = "";
  else delete root.dataset[ATTR[p]];
  try {
    if (hidden) localStorage.setItem(KEY[p], "1");
    else localStorage.removeItem(KEY[p]);
  } catch {
    /* storage unavailable: the choice lasts for this page only */
  }
}

function Toggle({ panel, label, show }: { panel: Panel; label: string; show: boolean }) {
  const [hidden, setState] = useState(false);
  useEffect(() => {
    setState(ATTR[panel] in document.documentElement.dataset);
  }, [panel]);
  if (!show) return null;
  return (
    <button
      type="button"
      className={`panel-toggle panel-toggle-${panel}`}
      aria-pressed={!hidden}
      onClick={() => {
        setState(!hidden);
        setHidden(panel, !hidden);
      }}
      title={hidden ? `Show ${label.toLowerCase()}` : `Hide ${label.toLowerCase()}`}
    >
      <svg aria-hidden="true" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <rect x="3" y="4" width="18" height="16" rx="2" />
        {panel === "nav" ? <path d="M9 4v16" /> : <path d="M15 4v16" />}
      </svg>
      <span>{label}</span>
    </button>
  );
}

export default function PanelToggles({ hasToc }: { hasToc: boolean }) {
  return (
    <div className="panel-toggles" role="group" aria-label="Page layout">
      <Toggle panel="nav" label="Pages" show />
      <Toggle panel="toc" label="On this page" show={hasToc} />
    </div>
  );
}
