"""Publishes the files built by `scripts/build_local.py` as a GitHub release.

    python scripts/publish.py             # publish dist-local/v<version>/
    python scripts/publish.py --offline   # also build + upload the offline installer

Uploads the installer, its updater signature, the portable zip and the
`latest.json` feed the in-app updater reads; release notes come from
CHANGELOG.md. Creates the git tag if it does not exist yet. Requires `gh`.
"""
import glob
import json
import os
import subprocess
import sys
from datetime import datetime, timezone

REPO = "reffer-191/bandwidth-limiter"
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(root)
version = json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["version"]
tag = f"v{version}"
out = os.path.join(root, "dist-local", tag)
offline = "--offline" in sys.argv


def run(cmd, **kw):
    print("+", " ".join(cmd))
    return subprocess.run(cmd, check=True, **kw)


def find(pattern):
    files = glob.glob(os.path.join(out, pattern))
    if not files:
        sys.exit(f"missing {pattern} in {out} — run `npm run dist` first")
    return files[0]


installer = find("*_x64-setup.exe")
sig = find("*_x64-setup.exe.sig")
portable = find("*-portable.zip")

if offline:
    print("building the offline installer (WebView2 runtime embedded)…")
    run(["npm", "run", "tauri", "build", "--", "--bundles", "nsis", "--config",
         json.dumps({"bundle": {"createUpdaterArtifacts": False, "windows": {"webviewInstallMode": {"type": "offlineInstaller", "silent": True}}}})],
        shell=True)
    built = glob.glob("src-tauri/target/release/bundle/nsis/*_x64-setup.exe")[0]
    offline_file = os.path.join(out, os.path.basename(built).replace("-setup.exe", "_offline-setup.exe"))
    os.replace(built, offline_file)

# GitHub turns spaces in asset names into dots.
asset_name = os.path.basename(installer).replace(" ", ".")
url = f"https://github.com/{REPO}/releases/download/{tag}/{asset_name}"
notes = subprocess.run(["python", "scripts/release_notes.py", tag] + (["--offline"] if offline else []),
                       capture_output=True, text=True, check=True, encoding="utf-8").stdout
feed = {
    "version": version,
    "notes": notes.split("\n---")[0].strip(),
    "pub_date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "platforms": {k: {"signature": open(sig, encoding="utf-8").read().strip(), "url": url} for k in ("windows-x86_64", "windows-x86_64-nsis")},
}
latest = os.path.join(out, "latest.json")
json.dump(feed, open(latest, "w", encoding="utf-8"), indent=2)
notes_file = os.path.join(out, "release-notes.md")
open(notes_file, "w", encoding="utf-8").write(notes)

# Tag (local + remote) if needed.
if subprocess.run(["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"], capture_output=True).returncode != 0:
    run(["git", "tag", "-a", tag, "-m", f"Bandwidth Limiter {version}"])
run(["git", "push", "origin", tag])

assets = [installer, sig, portable, latest] + ([offline_file] if offline else [])
exists = subprocess.run(["gh", "release", "view", tag, "-R", REPO], capture_output=True).returncode == 0
if exists:
    run(["gh", "release", "upload", tag, "-R", REPO, "--clobber"] + assets)
    run(["gh", "release", "edit", tag, "-R", REPO, "--notes-file", notes_file])
else:
    run(["gh", "release", "create", tag, "-R", REPO, "--title", f"Bandwidth Limiter {version}", "--notes-file", notes_file] + assets)
print(f"\npublished https://github.com/{REPO}/releases/tag/{tag}")
