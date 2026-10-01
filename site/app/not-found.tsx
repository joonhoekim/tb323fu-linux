// SPDX-License-Identifier: MIT
import Link from "next/link";

export default function NotFound() {
  return (
    <main id="main" className="narrow-page">
      <h1>Page not found</h1>
      <p>
        That page does not exist (or moved). Try the <Link href="/docs/">documentation</Link> or the{" "}
        <Link href="/">home page</Link>.
      </p>
    </main>
  );
}
