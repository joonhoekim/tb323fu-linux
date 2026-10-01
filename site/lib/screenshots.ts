// SPDX-License-Identifier: MIT
// Screenshot gallery source: site/public/screenshots/. An optional index.json
// ([{ "file", "title", "caption" }]) sets order, titles and captions (also the alt
// text); without it every image in the directory is listed by file name.
import fs from "node:fs";
import path from "node:path";
import { imageSize } from "image-size";
import { SITE_DIR } from "./config";

export interface Screenshot {
  /** path under public/, e.g. "/screenshots/gnome.png" (without base path) */
  src: string;
  title: string;
  caption: string;
  width?: number;
  height?: number;
}

const DIR = path.join(SITE_DIR, "public", "screenshots");
const IMAGE = /\.(png|jpe?g|webp|avif|gif)$/i;

function titleFromFile(file: string): string {
  const base = file.replace(/\.[^.]+$/, "").replace(/^\d+[-_ ]+/, "").replace(/[-_]+/g, " ").trim();
  return base ? base[0].toUpperCase() + base.slice(1) : file;
}

function dims(file: string): { width?: number; height?: number } {
  try {
    const { width, height } = imageSize(fs.readFileSync(path.join(DIR, file)));
    return { width, height };
  } catch {
    return {};
  }
}

interface ManifestEntry {
  file?: unknown;
  title?: unknown;
  caption?: unknown;
}

export function getScreenshots(): Screenshot[] {
  let files: string[];
  try {
    files = fs.readdirSync(DIR).filter((f) => IMAGE.test(f));
  } catch {
    return [];
  }
  const present = new Set(files);
  const make = (file: string, title?: string, caption?: string): Screenshot => ({
    src: `/screenshots/${file.split("/").map(encodeURIComponent).join("/")}`,
    title: title || titleFromFile(file),
    caption: caption || "",
    ...dims(file),
  });

  const manifestPath = path.join(DIR, "index.json");
  if (fs.existsSync(manifestPath)) {
    try {
      const data = JSON.parse(fs.readFileSync(manifestPath, "utf8")) as unknown;
      const list = Array.isArray(data) ? (data as ManifestEntry[]) : [];
      const shots = list
        .filter((e) => typeof e?.file === "string" && fs.existsSync(path.join(DIR, e.file as string)))
        .map((e) =>
          make(e.file as string, typeof e.title === "string" ? e.title : undefined, typeof e.caption === "string" ? e.caption : undefined),
        );
      if (shots.length) return shots;
    } catch (err) {
      console.warn(`screenshots/index.json ignored: ${(err as Error).message}`);
    }
  }
  return [...present].sort().map((f) => make(f));
}
