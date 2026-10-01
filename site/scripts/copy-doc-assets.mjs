// SPDX-License-Identifier: MIT
// Copy images that the repository's Markdown references by relative path into
// public/_repo/<repo path>, so the static site can serve them (lib/markdown.ts
// rewrites those references to /_repo/...). Runs before `next dev` / `next build`.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repo = process.env.REPO_ROOT ? path.resolve(process.env.REPO_ROOT) : path.resolve(site, "..");
const dest = path.join(site, "public", "_repo");
const SKIP = new Set([".git", "node_modules", "site", "out", ".next", ".claude"]);
const IMAGE = /\.(png|jpe?g|webp|avif|gif|svg)$/i;

function* markdownFiles(dir, depth = 0) {
  if (depth > 4) return;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (SKIP.has(e.name)) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) yield* markdownFiles(p, depth + 1);
    else if (e.name.toLowerCase().endsWith(".md")) yield p;
  }
}

fs.rmSync(dest, { recursive: true, force: true });
let n = 0;
for (const md of markdownFiles(repo)) {
  const text = fs.readFileSync(md, "utf8");
  const refs = [
    ...text.matchAll(/!\[[^\]]*\]\(\s*<?([^)\s>]+)>?/g),
    ...text.matchAll(/<img\b[^>]*\bsrc=["']([^"']+)["']/gi),
    ...text.matchAll(/^\s*\[[^\]]+\]:\s*<?(\S+?)>?(?:\s|$)/gm),
  ].map((m) => m[1]);
  for (const ref of refs) {
    if (/^[a-z][a-z0-9+.-]*:|^\/\/|^#/i.test(ref)) continue;
    const clean = decodeURIComponent(ref.replace(/[?#].*$/, ""));
    if (!IMAGE.test(clean)) continue;
    const abs = clean.startsWith("/") ? path.join(repo, clean) : path.resolve(path.dirname(md), clean);
    const rel = path.relative(repo, abs);
    if (rel.startsWith("..") || rel.startsWith(`site${path.sep}public`) || !fs.existsSync(abs)) continue;
    const out = path.join(dest, rel);
    fs.mkdirSync(path.dirname(out), { recursive: true });
    fs.copyFileSync(abs, out);
    n++;
  }
}
console.log(`copy-doc-assets: ${n} image(s) -> public/_repo`);
