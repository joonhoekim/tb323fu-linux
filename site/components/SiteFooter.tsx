// SPDX-License-Identifier: MIT
import { REPO_URL, githubUrl } from "@/lib/config";

export default function SiteFooter() {
  return (
    <footer className="site-footer">
      <p>
        Code and documentation are <a href={githubUrl("LICENSE")}>MIT-licensed</a> unless a file says otherwise; kernel
        patches are GPL-2.0-only. Firmware is not redistributed. Source on <a href={REPO_URL}>GitHub</a>.
      </p>
      <p>
        An independent community project, not affiliated with Lenovo or Qualcomm. Product names are trademarks of their
        owners.
      </p>
    </footer>
  );
}
