// SPDX-License-Identifier: MIT
import type { Metadata, Viewport } from "next";
import SiteHeader from "@/components/SiteHeader";
import SiteFooter from "@/components/SiteFooter";
import { SITE_DESCRIPTION, SITE_NAME } from "@/lib/config";
import "./globals.css";

export const metadata: Metadata = {
  title: { default: `${SITE_NAME} — mainline Linux on the Legion Tab Gen 5`, template: `%s · ${SITE_NAME}` },
  description: SITE_DESCRIPTION,
  robots: { index: true, follow: true },
};

export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#ffffff" },
    { media: "(prefers-color-scheme: dark)", color: "#0f1115" },
  ],
};

// Applies a saved light/dark choice before the first paint (no flash).
// Also the docs panel choices (components/PanelToggles.tsx).
const themeScript = `try{var d=document.documentElement,t=localStorage.getItem("theme");if(t==="light"||t==="dark")d.dataset.theme=t;if(localStorage.getItem("hide-nav"))d.dataset.navHidden="";if(localStorage.getItem("hide-toc"))d.dataset.tocHidden=""}catch(e){}`;

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" data-scroll-behavior="smooth" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: themeScript }} />
      </head>
      <body>
        <a href="#main" className="skip-link">
          Skip to content
        </a>
        <SiteHeader />
        {children}
        <SiteFooter />
      </body>
    </html>
  );
}
