// SPDX-License-Identifier: MIT
import DocsSidebar from "./DocsSidebar";
import { TocAside, TocInline } from "./Toc";
import PanelToggles from "./PanelToggles";
import { githubUrl } from "@/lib/config";
import { readDocSource, type DocEntry } from "@/lib/docs";
import { renderMarkdown } from "@/lib/markdown";

export async function renderDoc(entry: DocEntry) {
  return renderMarkdown(readDocSource(entry), entry.file, entry.title);
}

export default async function DocPage({ entry }: { entry: DocEntry }) {
  const doc = await renderDoc(entry);
  return (
    <div className="docs-layout">
      <DocsSidebar current={entry.route} />
      <main id="main" className="doc-main">
        <article className="doc">
          <PanelToggles hasToc={doc.toc.length >= 2} />
          <p className="doc-path">
            <a href={githubUrl(entry.file)}>{entry.file}</a>
          </p>
          <h1>{doc.title}</h1>
          <TocInline items={doc.toc} />
          <div className="prose" dangerouslySetInnerHTML={{ __html: doc.html }} />
          <p className="doc-source">
            <a href={githubUrl(entry.file)}>View this page&apos;s Markdown on GitHub</a>
          </p>
        </article>
      </main>
      <TocAside items={doc.toc} />
    </div>
  );
}
