"""Builds dist-portable/BandwidthLimiter-<version>-portable/ (+ .zip) from the release binaries."""
import json, os, shutil, sys, zipfile
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
version = json.load(open(os.path.join(root, "src-tauri", "tauri.conf.json"), encoding="utf-8"))["version"]
rel = os.path.join(root, "src-tauri", "target", "release")
name = f"BandwidthLimiter-{version}-portable"
out = os.path.join(root, "dist-portable", name)
shutil.rmtree(out, ignore_errors=True)
os.makedirs(out)
exe = next(f for f in ["bandwidth-limiter.exe", "Bandwidth Limiter.exe"] if os.path.exists(os.path.join(rel, f)))
shutil.copy2(os.path.join(rel, exe), os.path.join(out, "BandwidthLimiter.exe"))
for f in ["WinDivert.dll", "WinDivert64.sys"]:
    shutil.copy2(os.path.join(rel, f), out)
shutil.copy2(os.path.join(root, "src-tauri", "windivert", "LICENSE"), os.path.join(out, "WinDivert-LICENSE.txt"))
open(os.path.join(out, "LEEME.txt"), "w", encoding="utf-8").write(f"""Bandwidth Limiter {version} - version portable
============================================

1. Descomprime la carpeta donde quieras y ejecuta BandwidthLimiter.exe.
   Pedira permisos de administrador: son necesarios para cargar el driver de
   captura WinDivert (WinDivert64.sys), incluido en esta carpeta.
2. No instala nada. El driver se carga al abrir la app y se descarga al cerrarla.
3. Tus reglas, ajustes e historial de 30 dias se guardan por usuario de Windows en
   %APPDATA%\\Bandwidth Limiter (config.json y usage.json). Puedes ver la ruta en
   Ajustes > Datos. Si borras esa carpeta, la app vuelve a los valores por defecto.
4. Opcional (uso avanzado): crea un fichero vacio llamado "portable" junto al exe para
   que los datos se guarden en esta misma carpeta en lugar de en AppData.

Requisitos: Windows 10/11 x64 con el runtime WebView2 (incluido en Windows 11;
en Windows 10 suele venir con Microsoft Edge). Si falta, usa el instalador, que lo trae.
WinDivert se distribuye bajo LGPL v3 (ver WinDivert-LICENSE.txt).
""")
open(os.path.join(out, "README.txt"), "w", encoding="utf-8").write(f"""Bandwidth Limiter {version} - portable edition
=========================================

1. Unzip the folder anywhere and run BandwidthLimiter.exe.
   It asks for administrator rights: they are required to load the WinDivert
   capture driver (WinDivert64.sys) shipped in this folder.
2. Nothing gets installed. The driver is loaded when the app starts and unloaded on exit.
3. Rules, settings and the 30-day history are stored per Windows user in
   %APPDATA%\\Bandwidth Limiter (config.json and usage.json). Settings > Data shows the path.
   Delete that folder to reset the app.
4. Optional (advanced): create an empty file named "portable" next to the exe to keep
   the data in this folder instead of AppData.

Requirements: Windows 10/11 x64 with the WebView2 runtime (built into Windows 11;
Windows 10 usually has it through Microsoft Edge). If it is missing, use the installer.
WinDivert is distributed under the LGPL v3 (see WinDivert-LICENSE.txt).
""")
zip_path = out + ".zip"
with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as z:
    for f in sorted(os.listdir(out)):
        z.write(os.path.join(out, f), f"{name}/{f}")
print(out); print(zip_path, os.path.getsize(zip_path))
