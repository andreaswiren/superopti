# SuperOpti 0.3.0 preview

Signal brings a compact native dashboard, system light/dark themes, smaller Segoe UI typography, executable icons, and structured process tables. Antialiased graphs preserve measured peaks and missing-data gaps. Buttons, duration segments and panels now have smooth edges; window controls, pin/compact icons and colored capture status share the padded header.

- On-demand capture and CPU/core histories, with no continuous idle sampling.
- Click any table column to sort; click again to reverse. Pressure values now use
  bold accent text and magnitude bars. Sorting applies to displayed rows (the
  current page in paginated traffic history); contributor ranking selects its cohort.
- Capture updates repaint data without repeatedly resizing/resetting textboxes,
  buttons and dropdowns, preventing update-driven flicker and text shifts.
- Searchable contributors with resident RAM, private commit and pressure-score help; executable metadata tooltips, child processes, services and process/file folder actions.
- TCP/UDP traffic recording with UAC when needed, live flow rows, searchable time ranges, local history and IPv4/IPv6 endpoints. Browsing pauses visual follow without stopping recording.
- Pinned compact view and transparency; Signal app/tray icons and licensed Lucide assets.
- Fixed pagefile proposal: max(half installed RAM, 16 GiB), initial=max, disk guard and Undo.
- Regenerated CycloneDX inventory, license evidence, dependency audit and voluntary CRA documentation.

Extract **SuperOpti-windows-x64.zip**, then run portable or install for the current user. Process-core observation is an explicit five-second trace with UAC when needed. This unsigned preview retains explicit limitations: thread-to-file activity is unavailable; live CPU/GPU throttle overlays and full IPv6 scope/connection-instance validation remain unfinished. Captured traffic totals describe observed ETW payload events and can be incomplete. See README and VALIDATION for tested scope and remaining checks.
