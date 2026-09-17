"""Builds the signed installer and the portable zip on this machine and drops
both in dist-local/<version>/ for a quick test, without going through CI.

    python scripts/build_local.py

Needs the same prerequisites as `npm run tauri build`, plus the updater key in
%USERPROFILE%\\.bandwidth-limiter-signing (optional: without it the build
still succeeds, only the update signature is skipped).
"""
import glob
import json
import os
import shutil
import subprocess

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
os.chdir(root)
version = json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["version"]

env = dict(os.environ)
keys = os.path.join(os.path.expanduser("~"), ".bandwidth-limiter-signing")
key_file = os.path.join(keys, "updater.key")
if os.path.exists(key_file):
    env["TAURI_SIGNING_PRIVATE_KEY"] = open(key_file, encoding="utf-8").read()
    pw = os.path.join(keys, "updater-password.txt")
    if os.path.exists(pw):
        env["TAURI_SIGNING_PRIVATE_KEY_PASSWORD"] = open(pw, encoding="utf-8").read().strip()
else:
    print("(updater key not found: the .sig file will not be produced)")

subprocess.run("npm run tauri build -- --bundles nsis", shell=True, check=True, env=env)
subprocess.run(["python", "scripts/portable.py"], check=True)

out = os.path.join(root, "dist-local", f"v{version}")
os.makedirs(out, exist_ok=True)
for f in glob.glob("src-tauri/target/release/bundle/nsis/*setup.exe*") + glob.glob(f"dist-portable/BandwidthLimiter-{version}-portable.zip"):
    shutil.copy2(f, out)
print(f"\nv{version} ->", out)
for f in sorted(os.listdir(out)):
    print(f"  {f}  {os.path.getsize(os.path.join(out, f)) / 1048576:.1f} MB")
