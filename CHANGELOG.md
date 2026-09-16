# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.0] - 2026-09-16

### Added
- In-app updates: the app checks the GitHub releases at startup (configurable) and from *Settings → Updates*, and installs signed updates with one click.
- Continuous integration: every push and pull request is type-checked, built and tested; every `v*` tag builds, signs and publishes the release automatically (installer, offline installer, portable zip and updater feed).
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

[Unreleased]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/reffer-191/bandwidth-limiter/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/reffer-191/bandwidth-limiter/releases/tag/v0.4.0
[0.1.0]: https://github.com/reffer-191/bandwidth-limiter/releases/tag/v0.4.0
