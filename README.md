# Bandwidth Limiter

**English** · [Español](README.es.md)

A lightweight bandwidth limiter and network traffic monitor for Windows, in the spirit of NetLimiter, with a clean macOS-inspired interface. See which programs are using your connection right now, limit the download/upload speed of any of them, cap your whole PC, and keep the devices connected to your mobile hotspot under the same limit.

![Activity view](docs/screenshot-activity.png)

## Features

- **Live per-application traffic** – download and upload speed of every process, with its icon and description, plus the devices connected to your Windows mobile hotspot.
- **Limits** – per application (download and/or upload), for the whole computer (optionally only Internet traffic, ignoring the local network) and for the hotspot. Hotspot traffic also counts against the global limit, so connected devices can never exceed it.
- **Blocking** – cut incoming and/or outgoing traffic of an application with one switch.
- **History chart (1–60 min)** – hover to see who was using the network at any moment; click a point to freeze the app table at that instant and analyse a peak; mouse-wheel to zoom into a specific second (Shift+wheel pans, double-click resets).
- **Stable application list** – sorted by name or by accumulated 30-day usage, never by the live speed, so rows don't jump around while you click. Applications that used the network in the last 30 days stay in the list even when they are closed.
- **Connections and network** – active connections per application, network adapters, hotspot detection.
- **System tray** – minimise/close to the tray, toggle the limiter from the tray menu, live speeds in the tooltip.
- **Start with Windows**, start minimised, light/dark theme, bits or bytes.
- Tiny footprint: ~45 MB of RAM and well under 1 % CPU while shaping.

## Download

| | Installer | Portable |
|---|---|---|
| File | `Bandwidth Limiter_<version>_x64-setup.exe` (~210 MB) | `BandwidthLimiter-<version>-portable.zip` (~3 MB) |
| Needs Internet? | **No.** The WebView2 runtime is embedded and installed only if the PC lacks it. | No, but the PC must already have the WebView2 runtime (built into Windows 11; Windows 10 usually has it through Microsoft Edge). |
| Install location | `C:\Program Files\Bandwidth Limiter` (all users) | Any folder you like; nothing is installed |
| Uninstall | *Settings → Apps* – also removes the "start with Windows" task and unloads the driver | Delete the folder |

Grab either one from the [Releases](../../releases) page.

**Requirements:** Windows 10/11, 64-bit. Administrator rights are required every time the app starts (the capture driver demands it); you'll get the usual UAC prompt.

> **Antivirus note.** The app ships `WinDivert.dll` and `WinDivert64.sys`, the open-source packet capture driver (LGPL) used by many networking tools. Some antivirus products flag *any* network driver they don't know as suspicious. The files are unmodified copies of the official [WinDivert 2.2.2](https://github.com/basil00/WinDivert/releases) release.

## Using it

### Activity
Speeds update every second. The four tiles show total download/upload, local-network traffic and hotspot traffic. The application list is stable: sort by **Name**, **Download** or **Upload** (accumulated over the last 30 days); click the same header again to reverse the order. **Active** hides idle applications; the search box filters by name, description or path.

Click an application to open its detail panel: current speed, 30-day totals, PID, path, active connections and the **rule editor**.

### Rules
- In the detail panel, switch on **Download** and/or **Upload**, type a value and pick the unit (kbit/s, Mbit/s or KB/s, MB/s depending on *Settings → Units*). The rule applies immediately and survives restarts — it is bound to the executable path, not to the process ID, so it keeps working after the program is restarted.
- **Block** incoming or outgoing traffic with the two small switches.
- The **Rules** view lists everything in one place: the **whole computer** limit (optionally counting only Internet traffic), the **hotspot** limit and every application/device rule. Devices connected to the hotspot appear as `hotspot:<ip>` and can be limited individually.
- The **Limiter** switch in the sidebar (and in the tray menu) pauses all rules without deleting them.

![Rules view](docs/screenshot-rules.png)

### History chart
- Hover to see the totals and the top applications at that second.
- **Click** to pin an instant: the table below switches to the speeds of that moment, sorted by usage, with a blue banner and a *Back to live* button. Click the chart again (or the button) to return.
- **Mouse wheel** zooms around the cursor (up to ×60) without changing the selected history range; **Shift + wheel** pans; **double-click** resets. A pinned point stays pinned while you zoom.
- The range selector (1–60 min) and the series selector (All / Internet / Local / Hotspot) are in the card header.

### Tray and startup
- The **minimise** button hides the window to the tray (configurable). Left-click the tray icon to bring it back; right-click for *Show*, *Limiter active* and *Quit*.
- *Settings → Close to tray* makes the **X** hide the window instead of quitting, keeping the limits active.
- *Settings → Start with Windows* creates a scheduled task that launches the app (minimised) at logon with the required privileges. The uninstaller removes it.

## Where your data lives

Everything is stored per Windows user in `%APPDATA%\Bandwidth Limiter\`:

| File | Content |
|---|---|
| `config.json` | rules, limits, theme, units, tray/startup options |
| `usage.json` | applications seen in the last 30 days with daily download/upload totals |

*Settings → Data* shows the exact path. Delete the folder to reset the app. This applies to both the installer and the portable edition. (Advanced: create an empty file named `portable` next to the portable `.exe` to keep the data in that folder instead.)

## How it works

| Layer | Technology |
|---|---|
| UI | React + TypeScript + Vite; hand-written CSS, frameless window, Mica/Acrylic backdrop. |
| Shell | [Tauri 2](https://tauri.app) on top of the system WebView2 — no bundled browser. |
| Engine | Rust. Packets are captured with **WinDivert** on the `NETWORK` layer (local traffic), `NETWORK_FORWARD` (traffic you forward to hotspot clients), and the `FLOW`/`SOCKET` layers to map each connection to its process. Shaping uses deficit token buckets with one queue per class (application → hotspot → global); packets that don't fit in ~1 s of queue are dropped and TCP adapts. |

```
src/                  frontend (React)
src-tauri/src/
  windivert.rs        dynamic binding to WinDivert.dll
  engine/mod.rs       capture threads, application table, 1 s ticker → "tick" event
  engine/shaper.rs    token buckets, queues, scheduler
  engine/flows.rs     5-tuple → PID (WinDivert events + iphlpapi fallback)
  engine/usage.rs     30-day usage store
  engine/procinfo.rs  path, description and icon of a process
  autostart.rs        "start with Windows" scheduled task
  tray.rs             system tray
  config.rs           config.json
```

## Building from source

Prerequisites: [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs) (stable, MSVC toolchain) and the *Desktop development with C++* workload of Visual Studio Build Tools.

```bash
npm install
npm run tauri dev          # development build (self-elevates through UAC)
npm run tauri build        # release build + NSIS installer (downloads the WebView2 offline installer once)
python scripts/portable.py # portable zip from the release build
```

Handy while developing:

- `npm run dev` and open http://localhost:1420 in a browser: the UI runs against a simulated backend (`src/lib/mock.ts`), no driver or admin rights needed. Add `#rules` or `#settings` to the URL to open a specific view.
- Debug builds expose the WebView2 DevTools protocol when `BWL_DEVTOOLS_PORT=9223` is set (or a `devtools-port.txt` file sits next to the exe).
- If the app is force-killed the driver may stay loaded and lock `WinDivert64.sys`; run `sc stop WinDivert` as administrator.

## FAQ

**Why does it need administrator rights?** Capturing and delaying packets requires a kernel driver, and loading it needs elevation. The driver is unloaded when you quit.

**Some traffic shows as "Unknown".** The first packets of a brand-new connection can arrive before Windows reports which process owns it. The amount is usually tiny.

**Limits look ~5 % lower than configured.** Limits are enforced on the wire, including TCP/IP headers, while download managers report payload only.

**Does it limit my hotspot clients?** Yes: traffic forwarded to devices connected to the Windows mobile hotspot is captured too, appears as a device row, and is subject to the hotspot limit and the global limit.

## License

MIT — see [LICENSE](LICENSE). WinDivert is © Basil and distributed under the LGPL v3 (`src-tauri/windivert/LICENSE`).
