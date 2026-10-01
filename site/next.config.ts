// SPDX-License-Identifier: MIT
import type { NextConfig } from "next";

// GitHub Pages serves a project site under /<repo>; the workflow passes the
// path from actions/configure-pages. Empty for local development.
const basePath = (process.env.PAGES_BASE_PATH ?? "").replace(/\/+$/, "");

const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: true,
  basePath: basePath || undefined,
  assetPrefix: basePath || undefined,
  images: { unoptimized: true },
  env: { PAGES_BASE_PATH: basePath },
  poweredByHeader: false,
  // Next 16 writes AGENTS.md/CLAUDE.md into the project on `next dev`; not wanted here.
  agentRules: false,
};

export default nextConfig;
