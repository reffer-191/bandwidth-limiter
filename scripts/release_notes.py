"""Prints the CHANGELOG section for a version (e.g. `v0.5.0` or `0.5.0`),
followed by the bilingual download guide, for use as GitHub release notes."""
import re
import sys

sys.stdout.reconfigure(encoding="utf-8")

args = [a for a in sys.argv[1:] if not a.startswith("--")]
offline = "--offline" in sys.argv
version = args[0].lstrip("v") if args else None
text = open("CHANGELOG.md", encoding="utf-8").read()
m = re.search(rf"^## \[{re.escape(version)}\][^\n]*\n(.*?)(?=^## \[|\Z)", text, re.S | re.M) if version else None
section = m.group(1).strip() if m else "_No changelog entry for this version._"

offline_row = f"| `Bandwidth.Limiter_{version}_x64_offline-setup.exe` | Installer with the WebView2 runtime embedded (~210 MB) for PCs without Internet. | Instalador con el runtime WebView2 embebido (~210 MB) para equipos sin Internet. |\n" if offline else ""

print(f"""{section}

---

**Downloads / Descargas**

| File | English | Español |
|---|---|---|
| `Bandwidth.Limiter_{version}_x64-setup.exe` | Installer (~2 MB). Downloads the WebView2 runtime only if the PC lacks it. | Instalador (~2 MB). Descarga el runtime WebView2 solo si el PC no lo tiene. |
{offline_row}| `BandwidthLimiter-{version}-portable.zip` | Portable: unzip and run; needs WebView2 already installed. | Portable: descomprimir y ejecutar; requiere WebView2 ya instalado. |

Windows 10/11 x64. Administrator rights are requested at every start (capture driver). Data is stored per user in `%APPDATA%\\Bandwidth Limiter`. The binaries are signed with the project's self-signed certificate — SmartScreen may still warn on first run; see the README.
""")
