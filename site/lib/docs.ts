// SPDX-License-Identifier: MIT
// The registry of Markdown files the site renders, and their routes.
//
//   README.md                      -> /docs/
//   docs/<name>.md                 -> /docs/<name>/   (every file, picked up automatically)
//   <dir>/README.md (list below)   -> /docs/repo/<dir>/
//
// Files in HIDDEN are internal and not rendered; links to them go to GitHub.
import fs from "node:fs";
import path from "node:path";
import { REPO_ROOT } from "./config";

export type DocGroup = "project" | "docs" | "source";

export interface DocEntry {
  /** repository-relative POSIX path, e.g. "docs/distros.md" */
  file: string;
  /** route segments under /docs */
  slug: string[];
  /** site route without base path, always with a trailing slash */
  route: string;
  title: string;
  group: DocGroup;
  /** shown indented under the doc of this name */
  parent?: string;
}

/** Internal working documents: not rendered, not in the navigation. */
const HIDDEN = new Set(["docs/photo-shotlist.md"]);

/** Preferred order of docs/*.md in the navigation; the rest follow alphabetically. */
const DOCS_ORDER = [
  "hardware-status",
  "distros",
  "rooting",
  "install",
  "install-windows",
  "install-linux",
  "install-macos",
  "install-manual",
  "after-install",
  "recovery",
  "helper",
  "helper-reference",
];

/** docs/<parent>-<name>.md shown indented under docs/<parent>.md in the navigation. */
const NAV_PARENTS = ["install", "helper"];

/** Directory READMEs worth reading on the site, in navigation order. */
const SOURCE_READMES = [
  "kernel/README.md",
  "kernel/initramfs/README.md",
  "firmware/README.md",
  "userspace/platform/README.md",
  "helper/README.md",
  "android/README.md",
  "tools/install/README.md",
  "tools/install/distros/README.md",
];

export const GROUP_LABELS: Record<DocGroup, string> = {
  project: "Project",
  docs: "Documentation",
  source: "In the source tree",
};

function readTitle(abs: string, fallback: string): string {
  try {
    const text = fs.readFileSync(abs, "utf8");
    const m = /^#[ \t]+(.+?)[ \t#]*$/m.exec(text);
    if (m) return m[1].replace(/[`*_]/g, "").trim();
  } catch {
    /* fall through */
  }
  return fallback;
}

function routeOf(slug: string[]): string {
  return slug.length ? `/docs/${slug.join("/")}/` : "/docs/";
}

let cache: DocEntry[] | null = null;

/** All rendered docs, in navigation order. Re-read on every call in development. */
export function getDocs(): DocEntry[] {
  if (cache && process.env.NODE_ENV === "production") return cache;
  const out: DocEntry[] = [];
  const add = (file: string, slug: string[], group: DocGroup, fallback: string, parent?: string) => {
    const abs = path.join(REPO_ROOT, file);
    if (!fs.existsSync(abs) || HIDDEN.has(file)) return;
    out.push({ file, slug, route: routeOf(slug), title: readTitle(abs, fallback), group, parent });
  };

  add("README.md", [], "project", "Overview");

  let names: string[] = [];
  try {
    names = fs
      .readdirSync(path.join(REPO_ROOT, "docs"))
      .filter((n) => n.toLowerCase().endsWith(".md"))
      .map((n) => n.slice(0, -3));
  } catch {
    /* no docs/ */
  }
  const rank = (n: string) => {
    const i = DOCS_ORDER.indexOf(n);
    return i === -1 ? DOCS_ORDER.length : i;
  };
  names.sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
  for (const n of names) {
    // "repo" is reserved for the source-tree READMEs below
    if (n === "repo") continue;
    add(`docs/${n}.md`, [n], "docs", n, NAV_PARENTS.find((p) => n.startsWith(`${p}-`)));
  }

  for (const file of SOURCE_READMES) {
    const dir = path.posix.dirname(file);
    add(file, ["repo", ...dir.split("/")], "source", dir);
  }

  cache = out;
  return out;
}

export function findDocBySlug(slug: string[]): DocEntry | undefined {
  const key = slug.join("/");
  return getDocs().find((d) => d.slug.join("/") === key);
}

export function findDocByFile(file: string): DocEntry | undefined {
  return getDocs().find((d) => d.file === file);
}

export function readDocSource(entry: DocEntry): string {
  return fs.readFileSync(path.join(REPO_ROOT, entry.file), "utf8");
}
