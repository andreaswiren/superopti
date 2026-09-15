# Changelog

Application versions follow Semantic Versioning; preview tags append CI run/attempt. Dates use YYYY-MM-DD.

## [0.3.0] - 2026-09-15

- Add explicit five-second ETW process-to-logical-CPU observations with a limited
  UAC helper, identity checks, lost-event rejection, and deadline watchdog.
- Display observed indexes in contributors and process details and include their
  observation timestamp/report in capture exports.
- Show system commit/pagefile percentages in the Memory card; remove the
  unsupported per-process swap placeholder and explain the limitation in details.
- Align contributor toolbar controls to a shared 28-pixel height, center search
  placeholder/input text using font metrics, and keep minimum-width spacing.
- Add explicit floor/ceiling labels and guides to overview resource sparklines
  and network bandwidth, plus clearer percentage bounds on resource history.

- Add UTC graph labels and capture start/end times, resource-card sparklines,
  and separate receive/send interface-bandwidth curves.
- Add an on-demand process picker table with PID, memory, threads and handles.
- Export reports completion through a notification bubble without overlaying charts.

- Repair opacity popup sizing and combo notification handling; choosing transparency enables always-on-top without requiring a separate Pin click.

- Private commit collection and contributor-table column; pressure ranking now
  considers each process's share of the system commit limit.
- Clickable question-mark help explains the pressure formula, missing evidence,
  and the difference between resident RAM, private commit and actual swap.
- On-demand system checks report CPU firmware-limiting Event 37 history over the
  last 24 hours, with explicit unavailable/no-report states rather than claiming
  that throttling is absent.
- Signal native dashboard with compact Segoe UI typography, system themes,
  antialiased measured-data curves, smooth controls and structured tables.
- Padded Workspace header, horizontal SuperOpti rail branding, colored capture
  status, pin/compact icon controls and native minimize/maximize/close controls.
- Capture outcome distinguishes automatic completion, manual stop and failure.
- System health separates deviations, blocked actions, unavailable evidence and
  optional preferences, and sorts actionable findings first. Disk-space warning
  boundaries are tested without changing system settings.
- Signal waveform app/tray/README icons, with pinned Lucide source and license
  included in the dependency inventory.
- On-demand executable metadata tooltips (manufacturer, version, file size and
  location), with bounded helper execution and cached process icons.
- Live TCP/UDP history rows refresh every two seconds during recording; the
  visible latest 500 flow buckets do not limit the full local archive. Browsing
  history pauses visual follow, with an explicit Follow live action to resume.
- Native pinned compact view, opacity controls and process/file folder actions
  are implemented in the candidate. Throttle graph overlays and supported GPU
  throttle telemetry remain outstanding.

Validation scope and unfinished diagnostics are documented in VALIDATION.md.
This release is a preview; scoped visual passes do not constitute full
accessibility or hardware-matrix validation.

## [0.2.0] - 2026-09-14

### Added
- Professional native Overview, System checks, Settings and Process details pages, responsive system-DPI layout, labeled fixed-scale graphs and native selectable process table.
- Original app/tray icons, active capture badge, executable resources and manifest.
- Double-click/Enter process drill-down with creation-time identity checks, CPU split, memory, handles, threads and separate read/write I/O totals and rates.
- IPv4/IPv6 TCP endpoints and UDP local sockets; explicit 10-second TCP per-connection incoming/outgoing byte measurement through EStats.
- Fixed pagefile policy: initial and maximum equal max(half installed RAM,16 GiB), rounded up to MiB. Dedicated fix and Fix all, disk-space guard, administrator requirement, backup, Undo and restart notice. Multiple/custom layouts require manual review.
- CRA scope assessment, technical file, requirements mapping, risk register and security policy for individual non-commercial MIT distribution.
- Complete resolved Cargo CycloneDX 1.5 graph, verified archive hashes/license texts, Rust/runtime inventory, schema validation and dependency audit in release packaging.

### Changed
- Installer registration reads the embedded product version and uses the absolute system PowerShell path for uninstall.
- Capture deadline includes helper startup time; Stop terminates all helper threads without blocking on Windows driver I/O cancellation.
- Table selection tracks process instances across refreshes. Detail requests are serialized and identities checked before inspection.
- PowerShell launches from the Windows system directory with concurrently drained, bounded captured output.
- Pinned Rust/release tools; release packages include SBOM and security documentation.

### Known limitations
- TCP polling misses short flows and closing bytes; totals are lower bounds. UDP/QUIC remote peers and per-peer bytes are unavailable.
- Unsigned preview; administrator/reboot and accessibility validation remain incomplete. See VALIDATION.md.

## [0.1.0] - 2026-09-14

### Added
- Initial native Rust/windows-rs tray app, on-demand capture, isolated native/PDH collectors, bounded history and deadline/Stop cleanup.
- CPU, RAM, GPU, disk, commit, pagefile and contributor metrics, thread snapshot and local JSON export.
- Current-user install/uninstall, idle autostart, system checks, animation/Balanced fixes and Undo.
- Main-branch build, tests and automatic preview releases.

[0.2.0]: https://github.com/andreaswiren/superopti/compare/45b4296...main
[0.1.0]: https://github.com/andreaswiren/superopti/releases/tag/v0.1.0-build.1.1
