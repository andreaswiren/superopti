# Initial native Windows preview

- Rust/windows-rs tray app, idle until requested; 2/5/15-minute captures with 2/5-second intervals. Native core metrics remain responsive when optional counter providers stall; Stop and deadlines close every capture helper.
- CPU, RAM, GPU-engine, disk, commit and pagefile graphs; top-10 heuristic process rankings with CPU, working set, I/O, threads and handles.
- On-demand thread snapshot, system checks, local JSON export, two reversible fixes and Undo.
- Current-user install/uninstall and optional idle tray autostart.

Download and extract **SuperOpti-windows-x64.zip**. Run the executable directly or use **Install for me**. Read the bundled README for metric definitions and limitations. Preview binaries are unsigned. GPU counters depend on the driver. Rankings suggest contributors; they do not establish causation.

