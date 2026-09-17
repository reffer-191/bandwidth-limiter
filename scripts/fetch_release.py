"""Downloads the installer and the portable zip of a published release into
dist-local/<version>/ so they can be tested right away.

    python scripts/fetch_release.py            # latest release
    python scripts/fetch_release.py v0.6.2     # a specific tag
    python scripts/fetch_release.py --offline  # also the offline installer

Requires the GitHub CLI (`gh`) to be installed and authenticated.
"""
import os
import subprocess
import sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
args = [a for a in sys.argv[1:] if not a.startswith("--")]
offline = "--offline" in sys.argv
tag = args[0] if args else subprocess.run(
    ["gh", "release", "view", "--json", "tagName", "--jq", ".tagName"], capture_output=True, text=True, check=True
).stdout.strip()

out = os.path.join(root, "dist-local", tag)
os.makedirs(out, exist_ok=True)
patterns = ["*x64-setup.exe", "*-portable.zip"]
if not offline:
    patterns[0] = "*x64-setup.exe"  # gh matches "_offline-setup.exe" too; filtered below
cmd = ["gh", "release", "download", tag, "--dir", out, "--clobber"]
for p in patterns:
    cmd += ["--pattern", p]
subprocess.run(cmd, check=True)
if not offline:
    for f in os.listdir(out):
        if f.endswith("_offline-setup.exe"):
            os.remove(os.path.join(out, f))
print(f"\n{tag} ->", out)
for f in sorted(os.listdir(out)):
    print(f"  {f}  {os.path.getsize(os.path.join(out, f)) / 1048576:.1f} MB")
