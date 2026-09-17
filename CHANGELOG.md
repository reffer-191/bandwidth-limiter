# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
