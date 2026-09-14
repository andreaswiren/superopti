# SuperOpti desktop experience

## Selected direction: Signal

The user selected Signal after review by a lead designer and two independent UX critics. The native implementation follows its table-first composition: a 140 logical-pixel labeled rail, compact resource cards, an enclosed contributor table, a substantial resource-history graph and a separate resource-peak insight panel. No illustrative telemetry is present in the native application.

The custom caption occupies the top 28 logical pixels. The padded content header shows Workspace, a page heading, a state dot and text, quiet Pin and Compact icons, and opacity. The rail places SuperOpti to the right of its transparent waveform mark. Minimize sends the window to the taskbar; Close hides it to the system tray; Exit terminates it. Maximize/restore and edge resizing retain native Windows behavior.

Normal layout targets a 1020 by 680 logical client minimum with at least four complete contributor rows and a visible plot. Larger layouts cap the table at ten rows instead of filling the window with empty rows. Tables scroll horizontally for endpoint, identity and timestamp columns. Compact mode has a 420 by 488 minimum, four metric cards, the top three contributors, and Stop all. Stop all covers both performance and network recording. Opacity ranges from 50% to 100%, applies only while pinned, returns to 100% when unpinned, and is disabled in high contrast.

## Visual and interaction system

Static Segoe UI is verified through actual GDI font resolution for regular and semibold weights. Controls and table text use an 11 logical-pixel base, section headings 12, page headings 20 and primary metric values 23. Process icons use 16 pixels within a fixed 20-pixel slot, including reserved space when an icon is unavailable.

Charcoal surfaces use restrained rose actions, cyan memory traces, violet GPU traces and amber disk traces. Light mode has its own contrast-tested palette. Buttons, duration segments, panels, combo borders and toggle shapes use antialiased GDI+ rounded geometry. Parent-matched backgrounds prevent rectangular corner artifacts. Disabled actions retain their shape and muted text. Keyboard focus respects Windows UI state; mouse selection does not force a dotted focus rectangle.

The native process/report ListViews provide selection, keyboard navigation and horizontal scrolling. Numeric columns are right-aligned. CPU and pressure-score cells contain quiet proportional bars. Selection is retained by PID and process creation identity during ranking changes, preventing silent substitution after PID reuse. Double-click or Enter opens the selected process. Search filters names and PIDs.

Pin and Compact have explicit accessible names and native tooltips. Sorting, duration, opacity, process search, protocol and grouping controls receive explicit accessibility-property annotations rather than borrowing names from neighboring labels. Executable metadata infotips show the available publisher, version, size and path. Complete screen-reader descriptions of custom graphs remain follow-up work.

## Data semantics and workflows

Idle, permission pending, recording, stopped and error states remain distinct. State labels reflect actual application state; retained history alone does not prove successful completion. N/A and Unavailable are distinct from zero. The pressure score is a ranking clue, not proof of causation.

Overview presents CPU utilization, physical memory in GiB with percentage/installed capacity, GPU utilization and disk busy, plus actual capture peaks. Logical-processor charts use sampled activity on a common 0–100% scale. Process observed-core cells remain Not captured until execution evidence exists; affinity is not presented as observed execution. Private process commit is not physical swap residency, and process Swap remains Unavailable rather than inventing paged-out byte counts.

Graphs retain a bounded 120-second window and explicit elapsed-time labels. Early captures leave the unobserved remainder empty. Missing or nonfinite measurements split traces into separate runs. Shape-preserving antialiased curves pass through observed samples without smoothing away peaks or overshooting adjacent values; they do not connect missing observations. CPU area shading covers only observed runs.

Process details expose structured metrics, threads, connections, files, children and associated services where supported. Executable and file-containing folders can be revealed. A file-folder action requires a selected eligible filesystem path and does not replace or reset the file table. File handles belong to the process; they do not imply ownership by an individual thread.

Traffic history distinguishes live following from archive browsing. Follow live restores the latest bounded rows. Browsing retains its filtered table and totals while a separate footer reports current live-session received/sent totals. Errors and export outcomes remain visible independently of following. Protocol and endpoint labels distinguish TCP/UDP, IPv4/IPv6, local/remote and received/sent data. Recording explicitly requests elevation.

System checks preview current/target values and offer explicit reversible settings. The fixed pagefile policy is max(50% of installed physical RAM, 16 GiB), with equal initial and maximum sizes. Fix all names its exact three-setting scope; startup, storage and update reviews remain manual. Administrator requirements, restart implications and Undo state are explained before applying a fix. Autostart launches quietly without beginning capture.

## Rendering and assets

Painting uses an offscreen GDI bitmap, GDI+ antialiased shapes/graphs and a single final blit; native tables use double buffering. Actual worker updates and user/window events trigger repainting. There is no idle animation or sampling timer. DPI changes update fonts, bounds and layout; system theme events update light/dark or high-contrast colors.

The app/tray use the Signal rose waveform on charcoal, and the rail uses a transparent waveform plus the pinned Lucide navigation family. Attribution, license and revision are recorded under assets/lucide. Real executable icons are cached by process identity with a bounded cache. Recording uses a static tray indicator rather than animation.

## Validation evidence and limits

This is scoped engineering and UX evidence, not a blanket accessibility certification:

- The independent visual critic passed the populated Signal 8 dark overview at 1380 by 981 after the latest header, branding, icon-action and antialiased-control changes. No overlap or clipping was observed in that view.
- Earlier dark and light overview iterations passed their scoped reviews. Signal 6 compact-to-full interaction also passed scoped usability review for three-row density, correct capture state and visible keyboard focus. Later header changes require renewed checks rather than inheriting every earlier result.
- The independent usability critic passed Signal 9 light branding/header, antialiased controls, colored capture state, icon Compact-to-full interaction and explicit control names. Its separate visual review and untested traffic interactions remain outside that scoped result.
- Frame 7 maximize/restore was observed on a 5120 by 2112 monitor, restoring the prior window dimensions. Minimize behavior is taskbar minimization; close-to-tray is separate.
- Formatting and strict all-target clippy checks passed. All 18 current tests passed, including actual font resolution, palette contrast, interpolation sample/peak bounds, PID identity and bounded live traffic evidence.
- The optimized smoke capture auto-stopped at approximately 10003 ms, manual stop completed promptly, and no idle sampling events were observed.

Remaining review scope includes current-build minimum-size and mixed-DPI behavior at 150%/200%, revised light/compact states, live/paused/error traffic interactions, every process-detail page, complete keyboard/Narrator coverage, taskbar icon sizes and Explorer tray recreation. Release notes identify missing diagnostics separately; a polished overview does not imply that unsupported telemetry exists.
