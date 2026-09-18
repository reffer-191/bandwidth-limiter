# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.8.0] - 2026-09-18

### Added
- **Diagnostics** in *Settings*: capture engine state (threads, packets, queued/dropped, send errors, driver restarts, adapters, metered), the last warnings, a rotating log file (`%LOCALAPPDATA%\Bandwidth Limiter\logs\app.log`, 1 MB × 3) and a *Copy report* button that gathers everything for an issue.
- **Watchdog**: if a capture thread dies (BFE restarted, driver unloaded by another tool) or the driver could not be opened at start, the engine re-opens WinDivert automatically every 15 s until it works.
- **Retroactive attribution**: bytes of a brand-new connection that arrive before Windows reports its owner are parked for up to 10 s and moved from "Unknown" to the right application (live counters, statistics and quotas) as soon as the socket event lands.
- *Settings → Engine → Count TCP/IP headers in limits* (off by default): limits and speeds are now measured on the payload, which is what download managers display, so a 1 Mbit/s limit reads as 1 Mbit/s; switch it on to shape and measure the wire length instead.
- Unit tests for the packet parser, the flow table and the shaper chain (20 in total).

### Changed
- Packets are received and re-injected in batches (`WinDivertRecvEx`/`SendEx`, up to 64 per call) with a reusable buffer: fewer kernel round-trips and copies under load.
- Errors that were silently ignored (driver parameters, re-injection failures, tray, window, usage database) are now counted or logged; the log replaces the console-only output.

## [0.7.2] - 2026-09-18

### Changed
- Traffic is now counted when the limiter delivers a packet, not when it arrives from the network, and dropped packets are not counted. The chart, the per-app speeds and the quotas therefore show what actually gets through: with a 2 Mbit/s computer limit the curve no longer spikes above 2 Mbit/s while TCP adapts to the limit.

## [0.7.1] - 2026-09-17

### Fixed
- Toast notifications did not appear: Windows only shows toasts from unpackaged apps that have a Start Menu shortcut carrying their AppUserModelID. The app now sets that property on the installer's shortcut (or creates a per-user one for the portable edition) the first time it needs to notify, and *Settings → Notifications → Test* shows the error when a toast cannot be sent.

## [0.7.0] - 2026-09-17

### Added
- **Priorities** (high / normal / low) per application or device. They only matter when a general limit (computer, hotspot or adapter) is saturated: high-priority apps get most of it, low-priority ones the leftovers (weighted fair queuing, 16:4:1).
- **Schedules**: any rule can apply only on selected weekdays and within a time window (overnight windows such as 22:00 → 06:00 work); outside it the rule is simply not enforced.
- **Data quotas** per rule: bytes per day / week / month (download + upload); when reached the rule blocks the traffic or only notifies. Usage and percentage are shown in the editor and as badges in the lists.
- **Windows notifications** (toasts): first time an application or device uses the network, quota reached, rule activated/deactivated by its schedule. Each one can be switched off in *Settings → Notifications*; a *Test* button shows a sample.
- **Connection rules**: limit or block traffic with a remote IP, network (CIDR) or host name, on given ports and protocol (TCP/UDP), for every application or just one. Host names are resolved every 5 minutes.
- **Adapter rules**: replace the whole-computer limit for traffic on Wi-Fi, Ethernet, a specific adapter or whenever Windows reports the connection as metered.
- **Statistics** view (Ctrl+2): usage by hour (today) or by day (7 / 30 days), totals and per-application breakdown with share bars; click an application to chart only its usage.
- **Profiles**: named rule sets (Home / Work / Travel…) switchable from the Rules view or the tray icon menu; **export / import** of all rules and profiles as a JSON file.
- Usage history moved from `usage.json` to a small SQLite database (`usage.db`) with hourly buckets for the last 30 days; the old file is imported once.

### Changed
- Views are now Activity, Statistics, Rules, Network, Settings (Ctrl+1…5).
- The capture handle uses a higher WinDivert priority so other WinDivert-based tools (or an older copy of the app) only see what the limiter lets through, and the driver is no longer stopped on exit while another copy of the app is running.
- *Settings → Engine* reports the number of captured packets and the last capture error.

## [0.6.2] - 2026-09-17

### Fixed
- The update banner showed raw Markdown from the release notes ("### Fixed"); it now shows the first sentence in plain text.
- The unit dropdown (kbit/s / Mbit/s) was unreadable in the dark theme: native controls now follow the app theme.

## [0.6.1] - 2026-09-17

### Fixed
- Enabling the general limits could leave the PC without Internet: the upload row defaulted to kbit/s while the download row defaulted to Mbit/s, so typing the same number in both (e.g. "2") stored 2 kbit/s of upload and starved TCP acknowledgements. Both rows now start in the same unit (1 Mbit/s), the editor shows the effective limit in plain words and warns in red when a value is too low, and the engine floors general limits at 128 kbit/s and per-app limits at 8 kbit/s (blocking remains an explicit switch). Existing configurations are corrected on load.

## [0.6.0] - 2026-09-16

### Added
- English interface (in addition to Spanish); follows the Windows language by default, selectable in *Settings → Appearance*. Tray menu and engine messages are localized too.
- Keyboard: Ctrl+F focuses the search, Ctrl+1…4 switch views, ↑/↓ move through the application list, Enter opens/closes the detail panel, Esc closes it. Visible focus rings and accessible names on switches and buttons.
- First-run introduction (three steps, explains the UAC prompt); can be replayed from *Settings → About*.
- Per-application chart of the selected app in the detail panel.
- Tray icon shows a green badge while limits are being applied; the tooltip says so.
- *About* section with a link to the project and the list of third-party licenses.

### Changed
- The detail panel slides in; the tooltip of the chart uses the interface language.

## [0.5.0] - 2026-09-16

### Added
- In-app updates: the app checks the GitHub releases at startup (configurable) and from *Settings → Updates*, and installs signed updates with one click.
- Continuous integration: every push and pull request is type-checked, built and tested. (Releases were published by CI from 0.5.0 to 0.6.1; since 0.6.2 they are built and published from the maintainer's machine with `npm run release`.)
- This changelog.

### Changed
- The default installer is now lightweight (~2 MB): it embeds only the WebView2 bootstrapper and downloads the runtime if the PC lacks it. An `*_offline-setup.exe` (~210 MB, runtime included) is still published for machines without Internet.
- Executable and installers are code-signed (self-signed certificate for now; see README).

## [0.4.0] - 2026-09-15

### Added
- System tray icon: minimise/close to tray, limiter toggle and quit from the tray menu, live speeds in the tooltip.
- Start with Windows (scheduled task with elevated privileges) and start minimised.
- Persistent 30-day usage history per application; applications seen in that window stay in the list.
- Stable application list ordering (name or accumulated usage, reversible), pinnable history chart with zoom and pan.
- Resizable sidebar and detail panel. Classic Windows window controls.
- Single-instance guard, portable edition, offline NSIS installer with uninstall hooks (removes the startup task, unloads the driver).

### Changed
- Renamed from "Cauce" to "Bandwidth Limiter". Data lives in `%APPDATA%\Bandwidth Limiter` for both editions.

## [0.1.0] - 2026-09-14

### Added
- First working version: live per-process traffic, per-app / global / hotspot limits and blocking, history chart with per-app breakdown, connections and adapters views, light/dark theme.

[Unreleased]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.6.2...HEAD
[0.6.2]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/reffer-191/bandwidth-limiter/releases/tag/v0.4.0
[0.1.0]: https://github.com/reffer-191/bandwidth-limiter/releases/tag/v0.4.0
