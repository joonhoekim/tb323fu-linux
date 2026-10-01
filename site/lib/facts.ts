// SPDX-License-Identifier: MIT
// Facts for the landing page, read from the docs at build time (never copied by hand):
// the distributions table in docs/distros.md and the status columns in docs/hardware-status.md.
import fs from "node:fs";
import path from "node:path";
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import { toString } from "mdast-util-to-string";
import type { Root, Table, TableRow } from "mdast";
import { REPO_ROOT } from "./config";

function parse(file: string): Root | null {
  try {
    const text = fs.readFileSync(path.join(REPO_ROOT, file), "utf8");
    return unified().use(remarkParse).use(remarkGfm).parse(text) as Root;
  } catch {
    return null;
  }
}

const SUPERSCRIPTS = /[⁰¹²³⁴⁵⁶⁷⁸⁹]+/g;
const cells = (row: TableRow) => row.children.map((c) => toString(c).replace(SUPERSCRIPTS, "").trim());

export interface Distro {
  system: string;
  boot: string;
}

export function getDistros(): Distro[] {
  const tree = parse("docs/distros.md");
  if (!tree) return [];
  const table = tree.children.find((n): n is Table => n.type === "table");
  if (!table) return [];
  const head = cells(table.children[0]).map((h) => h.toLowerCase());
  const iSys = head.findIndex((h) => h.startsWith("system"));
  const iBoot = head.findIndex((h) => h.includes("boot"));
  if (iSys < 0) return [];
  return table.children.slice(1).map((r) => {
    const c = cells(r);
    return { system: c[iSys] ?? "", boot: iBoot >= 0 ? (c[iBoot] ?? "") : "" };
  });
}

export type StatusKey = "works" | "partial" | "broken" | "unverified" | "na";

export const STATUS_META: Record<StatusKey, { label: string; symbol: string }> = {
  works: { label: "works", symbol: "✅" },
  partial: { label: "partial", symbol: "🟡" },
  broken: { label: "does not work", symbol: "❌" },
  unverified: { label: "not verified", symbol: "❓" },
  na: { label: "n/a or not implemented", symbol: "—" },
};

export interface StatusSection {
  title: string;
  counts: Record<StatusKey, number>;
  total: number;
}

export interface HardwareSummary {
  kernel: string | null;
  updated: string | null;
  sections: StatusSection[];
  totals: Record<StatusKey, number>;
  total: number;
}

function classify(cell: string): StatusKey | null {
  for (const k of Object.keys(STATUS_META) as StatusKey[]) if (cell.includes(STATUS_META[k].symbol)) return k;
  return null;
}

const zero = (): Record<StatusKey, number> => ({ works: 0, partial: 0, broken: 0, unverified: 0, na: 0 });

export function getHardwareSummary(): HardwareSummary | null {
  const file = "docs/hardware-status.md";
  const tree = parse(file);
  if (!tree) return null;
  const text = fs.readFileSync(path.join(REPO_ROOT, file), "utf8");
  const kernel = /mainline Linux ([0-9][\w.-]*)/i.exec(text)?.[1] ?? null;
  const updated = /Last updated (\d{4}-\d{2}-\d{2})/i.exec(text)?.[1] ?? null;

  const sections: StatusSection[] = [];
  let current = "";
  for (const node of tree.children) {
    if (node.type === "heading" && node.depth === 2) current = toString(node);
    if (node.type !== "table") continue;
    const head = cells(node.children[0]).map((h) => h.toLowerCase());
    const iStatus = head.indexOf("status");
    if (iStatus < 0) continue;
    let sec = sections.find((s) => s.title === current);
    if (!sec) sections.push((sec = { title: current || "Status", counts: zero(), total: 0 }));
    for (const row of node.children.slice(1)) {
      const k = classify(cells(row)[iStatus] ?? "");
      if (!k) continue;
      sec.counts[k]++;
      sec.total++;
    }
  }
  const totals = zero();
  for (const s of sections) for (const k of Object.keys(totals) as StatusKey[]) totals[k] += s.counts[k];
  const total = sections.reduce((a, s) => a + s.total, 0);
  return { kernel, updated, sections, totals, total };
}
