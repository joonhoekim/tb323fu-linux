// SPDX-License-Identifier: MIT
// Write public/third-party-licenses.txt: every production dependency of the site
// (`pnpm licenses list --prod`) with the license files of its package, identical
// texts once. Wider than what reaches the browser (most packages only run at build
// time), which is fine for a notice. Runs before `next dev` / `next build`.
import { execSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const site = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const out = path.join(site, "public", "third-party-licenses.txt");
const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright)([-._].*)?$/i;

// through the shell: on Windows pnpm is a .cmd shim, which Node does not start directly
const list = JSON.parse(
  execSync("pnpm licenses list --prod --json", { cwd: site, encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }),
);
const pkgs = [];
for (const [license, entries] of Object.entries(list)) {
  for (const e of entries) {
    e.versions.forEach((v, i) => pkgs.push({ name: e.name, version: v, license, dir: e.paths[i] }));
  }
}
pkgs.sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));

const seen = new Map();
const body = [];
for (const p of pkgs) {
  body.push(`== ${p.name} ${p.version} (${p.license})`);
  const files = fs
    .readdirSync(p.dir)
    .filter((n) => LICENSE_FILE.test(n) && fs.statSync(path.join(p.dir, n)).isFile())
    .sort();
  if (files.length === 0) {
    body.push("   no license file in the package; the license is the one named above\n");
    continue;
  }
  for (const n of files) {
    const text = fs.readFileSync(path.join(p.dir, n), "utf8").trim() + "\n";
    const id = crypto.createHash("sha256").update(text).digest("hex").slice(0, 12);
    if (seen.has(id)) {
      body.push(`-- ${n}: identical to [text ${id}] above\n`);
    } else {
      seen.set(id, true);
      body.push(`-- ${n} [text ${id}]\n\n${text}`);
    }
  }
}

fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(
  out,
  "Third-party packages used to build this website, with the license files of their packages.\n" +
    `${pkgs.length} packages.\n\n` +
    body.join("\n"),
);
console.log(`third-party-licenses: ${pkgs.length} packages, ${seen.size} distinct license texts`);
