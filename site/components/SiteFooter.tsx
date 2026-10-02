// SPDX-License-Identifier: MIT
import { REPO_URL, githubUrl } from "@/lib/config";

export default function SiteFooter() {
  return (
    <footer className="site-footer">
      <p>
        Documentation text and screenshots are <a href={githubUrl("LICENSES/CC-BY-SA-4.0.txt")}>CC BY-SA 4.0</a>{" "}
        (code snippets in them also MIT). Code is <a href={githubUrl("LICENSE")}>MIT</a> unless a file says otherwise:
        the helper and Tablet Settings are GPL-3.0-or-later, kernel patches GPL-2.0-only, board device trees
        BSD-3-Clause; see <a href={githubUrl("NOTICE")}>NOTICE</a>. Firmware is not redistributed. Source on{" "}
        <a href={REPO_URL}>GitHub</a>.
      </p>
      <p>
        An independent community project, not affiliated with Lenovo, Qualcomm or Valve. Product names are trademarks of
        their owners; see the <a href={githubUrl("TRADEMARKS.md")}>name policy</a>.
      </p>
    </footer>
  );
}
