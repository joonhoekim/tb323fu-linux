// SPDX-License-Identifier: MIT
import type { Metadata } from "next";
import { notFound } from "next/navigation";
import DocPage from "@/components/DocPage";
import { findDocBySlug, getDocs } from "@/lib/docs";

export const dynamicParams = false;

export function generateStaticParams() {
  return getDocs()
    .filter((d) => d.slug.length > 0)
    .map((d) => ({ slug: d.slug }));
}

type Props = { params: Promise<{ slug: string[] }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { slug } = await params;
  const entry = findDocBySlug(slug);
  return entry ? { title: entry.title } : {};
}

export default async function Doc({ params }: Props) {
  const { slug } = await params;
  const entry = findDocBySlug(slug);
  if (!entry) notFound();
  return <DocPage entry={entry} />;
}
