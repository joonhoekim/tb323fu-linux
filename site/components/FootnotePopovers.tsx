// SPDX-License-Identifier: MIT
"use client";
// Places footnote popovers (lib/markdown.ts) in the viewport, so a scrolling table does not clip them.
import { useEffect } from "react";

export default function FootnotePopovers() {
  useEffect(() => {
    document.documentElement.classList.add("fn-js");
    const place = (e: Event) => {
      const fn = (e.target as Element | null)?.closest?.(".prose .fn");
      const pop = fn?.querySelector<HTMLElement>(".fn-pop");
      if (!fn || !pop) return;
      pop.style.visibility = "hidden";
      pop.style.display = "block";
      const r = fn.getBoundingClientRect();
      const w = pop.offsetWidth;
      const h = pop.offsetHeight;
      const left = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), window.innerWidth - w - 8);
      const top = r.top - h - 6 >= 8 ? r.top - h - 6 : r.bottom + 6;
      pop.style.left = `${left}px`;
      pop.style.top = `${top}px`;
      pop.style.removeProperty("display");
      pop.style.removeProperty("visibility");
    };
    document.addEventListener("mouseover", place);
    document.addEventListener("focusin", place);
    return () => {
      document.removeEventListener("mouseover", place);
      document.removeEventListener("focusin", place);
    };
  }, []);
  return null;
}
