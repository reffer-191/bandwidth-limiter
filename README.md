# Bandwidth Limiter

**English** · [Español](README.es.md)

[![CI](https://github.com/reffer-191/bandwidth-limiter/actions/workflows/ci.yml/badge.svg)](https://github.com/reffer-191/bandwidth-limiter/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/reffer-191/bandwidth-limiter)](https://github.com/reffer-191/bandwidth-limiter/releases/latest)

A lightweight bandwidth limiter and network traffic monitor for Windows, in the spirit of NetLimiter, with a clean macOS-inspired interface. See which programs are using your connection right now, limit the download/upload speed of any of them, cap your whole PC, and keep the devices connected to your mobile hotspot under the same limit.

![Activity view](docs/screenshot-activity.png)

## Features

- **Live per-application traffic** – download and upload speed of every process, with its icon and description, plus the devices connected to your Windows mobile hotspot.
- **Limits** – per application (download and/or upload), for the whole computer (optionally only Internet traffic, ignoring the local network) and for the hotspot. Hotspot traffic also counts against the global limit, so connected devices can never exceed it.
- **Blocking** – cut incoming and/or outgoing traffic of an application with one switch.
- **Priorities** – high / normal / low per application: when a general limit is saturated, high-priority apps get most of it (video call) and low-priority ones the leftovers (downloads).
- **Schedules and data quotas** – any rule can apply only on certain weekdays/hours, and can carry a quota (bytes per day, week or month) that blocks the traffic or just warns you when it runs out.
- **Connection rules** – limit or block traffic with a remote host, IP range or port (per protocol, for all apps or one), e.g. throttle a CDN or block a tracker port.
- **Adapter rules** – a different whole-PC limit on Wi-Fi, Ethernet, a specific adapter or whenever Windows reports the connection as *metered*.
- **Statistics** – usage by hour or by day (today / 7 / 30 days) with a per-application breakdown, stored in a small SQLite database.
- **Profiles** – switch between complete rule sets (Home / Work / Travel) from the app or the tray icon; export/import rules as JSON.
- **Notifications** – Windows toasts when a new application uses the network, a quota runs out or a scheduled rule kicks in (each one optional).
- **History chart (1–60 min)** – hover to see who was using the network at any moment; click a point to freeze the app table at that instant and analyse a peak; mouse-wheel to zoom into a specific second (Shift+wheel pans, double-click resets).
- **Stable application list** – sorted by name or by accumulated 30-day usage, never by the live speed, so rows don't jump around while you click. Applications that used the network in the last 30 days stay in the list even when they are closed.
- **Connections and network** – active connections per application, network adapters, hotspot detection.
- **System tray** – minimise/close to the tray, toggle the limiter from the tray menu, live speeds in the tooltip.
- **Start with Windows**, start minimised, light/dark theme, bits or bytes, **English and Spanish** interface (follows the Windows language).
- **Keyboard friendly** – Ctrl+F search, Ctrl+1…5 views, ↑/↓ + Enter in the list, Esc to close.
- **Automatic updates** – new releases are offered in-app and installed with one click (signed update feed).
- Tiny footprint: ~45 MB of RAM and well under 1 % CPU while shaping.

## Download

| | Installer | Offline installer | Portable |
|---|---|---|---|
| File | `Bandwidth.Limiter_<version>_x64-setup.exe` (~5 MB) | `Bandwidth.Limiter_<version>_x64_offline-setup.exe` (~210 MB) | `BandwidthLimiter-<version>-portable.zip` (~4 MB) |
| WebView2 runtime | Downloaded only if the PC lacks it (built into Windows 11) | Embedded — works with no Internet at all | Must already be present |
| Install location | `C:\Program Files\Bandwidth Limiter` (all users) | same | Any folder; nothing is installed |
| Updates | In-app | In-app | Download the new zip |
| Uninstall | *Settings → Apps* – also removes the "start with Windows" task and unloads the driver | same | Delete the folder |

Grab either one from the [Releases](../../releases) page.

**Requirements:** Windows 10/11, 64-bit. Administrator rights are required every time the app starts (the capture driver demands it); you'll get the usual UAC prompt.

> **SmartScreen and antivirus.** The executable and the installers are code-signed, but with a certificate the project issued itself (a certificate from a public authority is not free). Windows may therefore show *"Windows protected your PC"* on first run: click **More info → Run anyway**. You can verify the signature in the file's *Properties → Digital Signatures* tab (signer "Bandwidth Limiter, reffer-191"). The app also ships `WinDivert.dll` and `WinDivert64.sys`, the open-source packet capture driver (LGPL) used by many networking tools; they are unmodified copies of the official [WinDivert 2.2.2](https://github.com/basil00/WinDivert/releases) release and keep Microsoft's driver signature.

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

#### Priority, schedule and quota
Every rule editor has a *Priority, schedule and quota* disclosure:

- **Priority** (applications and devices only): *High* / *Normal* / *Low*. It changes nothing on its own — it decides who gets the bandwidth when a general limit (computer, hotspot or adapter) is saturated. High-priority traffic is served about 16× more than low-priority traffic and 4× more than normal, so a video call marked *High* stays smooth while a download marked *Low* takes what is left.
- **Only on a schedule**: pick the weekdays and a *from → to* window (local time). A window that crosses midnight (22:00 → 06:00) belongs to the day it starts on; the same start and end time means the whole day. Outside the window the rule is not enforced at all, and the badges in the lists are dimmed.
- **Data quota**: an allowance in MB/GB per day, week (from Monday) or month, counting download plus upload. When it runs out the rule either **blocks** the traffic until the next period or **only notifies**. The editor shows *Used X of Y (%)*. The whole-computer quota counts every application; the hotspot quota counts the connected devices.

#### Connection rules
*Rules → Per connection* limits or blocks traffic with a remote endpoint instead of an application:

- **Host, IP or network**: `1.2.3.4`, `10.0.0.0/8`, `2a00::/16` or a host name such as `cdn.example.com` (resolved every 5 minutes — CDNs may answer with other addresses, so prefer IP ranges when you can). Empty = any host.
- **Ports**: `443`, `80,443`, `6881-6889`; empty = all. **Protocol**: any / TCP / UDP.
- **Application**: all, or one specific application.
- The rule has the same limit/block/schedule editor. Connection limits stack with the application and general limits (the strictest wins).

#### Adapter rules
*Rules → Per adapter* replaces the whole-computer limit for traffic that goes through a given adapter: **Wi-Fi**, **Ethernet**, a specific adapter by name, or **Metered connection** (whatever Windows currently flags as metered — mobile data, tethering, or a network you marked as metered in Windows settings). The first enabled rule that matches is used; when none matches, the normal whole-computer limit applies.

#### Profiles, export and import
The **Profiles** card at the top of *Rules* keeps named copies of the complete rule set (general limits, adapter and connection rules, every application rule). Create one from the current rules with **New profile**; switching profiles saves the edits into the one you leave and loads the other; the tray icon menu switches them too. **Export…** writes everything (live rules plus profiles) to a JSON file, **Import…** loads such a file (profiles with the same name are replaced).

### Statistics
*Statistics* (Ctrl+2) shows how much was transferred **today** (by hour), in the **last 7 days** or the **last 30 days** (by day), with download/upload totals and a per-application table with share bars. Click an application to chart only its usage; double-click to jump to it in *Activity*. The data comes from the hourly usage database (see *Where your data lives*).

### History chart
- Hover to see the totals and the top applications at that second.
- **Click** to pin an instant: the table below switches to the speeds of that moment, sorted by usage, with a blue banner and a *Back to live* button. Click the chart again (or the button) to return.
- **Mouse wheel** zooms around the cursor (up to ×60) without changing the selected history range; **Shift + wheel** pans; **double-click** resets. A pinned point stays pinned while you zoom.
- The range selector (1–60 min) and the series selector (All / Internet / Local / Hotspot) are in the card header.

### Keyboard shortcuts
| Keys | Action |
|---|---|
| Ctrl+F | Focus the search box |
| Ctrl+1 / 2 / 3 / 4 / 5 | Activity / Statistics / Rules / Network / Settings |
| ↑ ↓ | Move through the application list |
| Enter | Open / close the detail panel of the focused row |
| Esc | Close the detail panel, clear the search |
| Mouse wheel / Shift+wheel / double-click on the chart | Zoom / pan / reset |

### Updates
The app checks the project's releases a few seconds after starting (switch it off in *Settings → Updates*) and shows a banner when a newer version exists; *Settings → Updates → Check now* does it on demand. Updates are downloaded from GitHub, verified against the project's signing key and installed by the signed installer, after which the app restarts. The portable edition does not self-update: download the new zip.

### Tray and startup
- The **minimise** button hides the window to the tray (configurable). Left-click the tray icon to bring it back; right-click for *Show*, *Limiter active* and *Quit*.
- *Settings → Close to tray* makes the **X** hide the window instead of quitting, keeping the limits active.
- *Settings → Start with Windows* creates a scheduled task that launches the app (minimised) at logon with the required privileges. The uninstaller removes it.

### Notifications
Windows toast notifications, each switchable in *Settings → Notifications*: **New application** (first time a program or hotspot device uses the network — off by default), **Quota reached** and **Scheduled rules** (a rule was activated or deactivated by its schedule). *Test* shows a sample toast; clicking a toast brings the window back.

## Where your data lives

Everything is stored per Windows user in `%APPDATA%\Bandwidth Limiter\`:

| File | Content |
|---|---|
| `config.json` | rules, limits, profiles, theme, units, tray/startup/notification options |
| `usage.db` | SQLite database: applications seen in the last 30 days with hourly download/upload totals (feeds Statistics and the quotas). A `usage.json` from versions before 0.7 is imported once and renamed `usage.json.migrated`. |

*Settings → Data* shows the exact path. Delete the folder to reset the app. This applies to both the installer and the portable edition. (Advanced: create an empty file named `portable` next to the portable `.exe` to keep the data in that folder instead.)

## How it works

| Layer | Technology |
|---|---|
| UI | React + TypeScript + Vite; hand-written CSS, frameless window, Mica/Acrylic backdrop. |
| Shell | [Tauri 2](https://tauri.app) on top of the system WebView2 — no bundled browser. |
| Engine | Rust. Packets are captured with **WinDivert** on the `NETWORK` layer (local traffic), `NETWORK_FORWARD` (traffic you forward to hotspot clients), and the `FLOW`/`SOCKET` layers to map each connection to its process. Shaping uses deficit token buckets with one queue per class (application → connection rule → hotspot → computer/adapter); when several queues wait on the same bucket the scheduler picks packets by start-time fair queuing weighted by the application's priority. Packets that don't fit in ~1 s of queue are dropped and TCP adapts. Schedules and quotas are re-evaluated every second and only pushed to the shaper when something changes. |

```
src/                  frontend (React)
src-tauri/src/
  windivert.rs        dynamic binding to WinDivert.dll
  engine/mod.rs       capture threads, application table, 1 s ticker → "tick" event
  engine/shaper.rs    token buckets, queues, priority-weighted scheduler, connection/adapter matching
  engine/effective.rs schedules + quotas → what is enforced right now
  engine/flows.rs     5-tuple → PID (WinDivert events + iphlpapi fallback)
  engine/usage.rs     hourly usage store (SQLite) + statistics queries
  notify.rs           Windows toast notifications
  clock.rs            local time / calendar periods
  engine/procinfo.rs  path, description and icon of a process
  autostart.rs        "start with Windows" scheduled task
  tray.rs             system tray
  config.rs           config.json (rules, profiles, preferences)
  dialogs.rs          Win32 open/save dialogs for import/export
```

## Building from source

Prerequisites: [Node.js](https://nodejs.org) 20+, [Rust](https://rustup.rs) (stable, MSVC toolchain) and the *Desktop development with C++* workload of Visual Studio Build Tools.

```bash
npm install
npm run tauri dev          # development build (self-elevates through UAC)
npm run tauri build        # release build + NSIS installer
python scripts/portable.py # portable zip from the release build
```

Releases are built and published from the maintainer's machine (GitHub Actions only runs the checks on every push):

```bash
npm run dist             # build + sign the installer and the portable zip into dist-local/v<version>/
npm run publish          # create the GitHub release for that version: installer, .sig, portable, latest.json, notes from CHANGELOG.md
npm run release          # both steps in one go
npm run publish -- --offline   # additionally build and upload the offline installer (~210 MB)
npm run fetch -- v0.6.2  # download the published files of a version into dist-local/<tag>/
```

`npm run dist` signs with the certificate in the Windows certificate store (see `scripts/sign.ps1`) and, when present, the updater key in `%USERPROFILE%\.bandwidth-limiter-signing`.

Handy while developing:

- `npm run dev` and open http://localhost:1420 in a browser: the UI runs against a simulated backend (`src/lib/mock.ts`), no driver or admin rights needed. Add `#rules` or `#settings` to the URL to open a specific view.
- Debug builds expose the WebView2 DevTools protocol when `BWL_DEVTOOLS_PORT=9223` is set (or a `devtools-port.txt` file sits next to the exe).
- If the app is force-killed the driver may stay loaded and lock `WinDivert64.sys`; run `sc stop WinDivert` as administrator.

## FAQ

**Why does it need administrator rights?** Capturing and delaying packets requires a kernel driver, and loading it needs elevation. The driver is unloaded when you quit.

**Some traffic shows as "Unknown".** The first packets of a brand-new connection can arrive before Windows reports which process owns it. The amount is usually tiny.

**The chart shows less than what arrives at my network card.** Speeds and totals count the packets the limiter delivers; what it drops while TCP adapts to a limit is not counted, so the curve never exceeds an active limit (local traffic excluded by *Internet traffic only* is still shown, of course).

**Limits look ~5 % lower than configured.** Limits are enforced on the wire, including TCP/IP headers, while download managers report payload only.

**Why is the certificate self-signed?** Certificates trusted by Windows cost money every year. Projects that want to remove the SmartScreen warning can apply to [SignPath Foundation](https://signpath.org/) (free signing for open-source projects) or buy an OV/EV certificate; the build already supports any certificate through `scripts/sign.ps1`.

**Does it limit my hotspot clients?** Yes: traffic forwarded to devices connected to the Windows mobile hotspot is captured too, appears as a device row, and is subject to the hotspot limit and the global limit.

**A priority does not seem to do anything.** Priorities only matter when a general limit (computer, hotspot or adapter) is saturated: with plenty of bandwidth every app gets what it asks for. Set a computer limit and run two downloads with different priorities to see the difference.

**A host-name connection rule does not catch everything.** The rule matches the addresses the host name resolves to on this PC (refreshed every 5 minutes). Big services answer DNS with many different addresses; use an IP range or a port rule for those.

## License

MIT — see [LICENSE](LICENSE). WinDivert is © Basil and distributed under the LGPL v3 (`src-tauri/windivert/LICENSE`).
