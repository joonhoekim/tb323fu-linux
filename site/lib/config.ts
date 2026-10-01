// SPDX-License-Identifier: MIT
// Build-time configuration. Everything here is read while pages are rendered
// (server components / static export), never in the browser.
import path from "node:path";

/** The repository checkout the site is built from (site/ lives inside it). */
export const REPO_ROOT = process.env.REPO_ROOT
  ? path.resolve(process.env.REPO_ROOT)
  : path.resolve(process.cwd(), "..");

/** The site/ directory. */
export const SITE_DIR = path.join(REPO_ROOT, "site");

/** "/tb323fu-linux" on GitHub Pages, "" locally (see next.config.ts). */
export const BASE_PATH = (process.env.PAGES_BASE_PATH ?? "").replace(/\/+$/, "");

export const REPO_URL = (process.env.REPO_URL ?? "https://github.com/joonhoekim/tb323fu-linux").replace(/\/+$/, "");
export const REPO_BRANCH = process.env.REPO_BRANCH ?? "main";

export const SITE_NAME = "tb323fu-linux";
export const SITE_DESCRIPTION =
  "Mainline Linux on the Lenovo Legion Tab Gen 5 / Legion Y700 5th Gen (TB323FU, Snapdragon 8 Elite Gen 5).";

/** Prefix a site-absolute path ("/docs/") with the base path, for raw HTML and <img>. */
export function withBase(p: string): string {
  return BASE_PATH + p;
}

/** GitHub URL of a repository path (file → blob, directory → tree). */
export function githubUrl(repoPath: string, isDir = false): string {
  const clean = repoPath.replace(/^\/+/, "").replace(/\/+$/, "");
  if (!clean) return REPO_URL;
  return `${REPO_URL}/${isDir ? "tree" : "blob"}/${REPO_BRANCH}/${clean.split("/").map(encodeURIComponent).join("/")}`;
}
