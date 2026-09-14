# SuperOpti

<img src="assets/superopti.png" alt="SuperOpti Signal waveform icon" width="96" height="96">

The Signal waveform uses Lucide's `audio-lines` icon, adapted to SuperOpti's rose and charcoal palette. [Source and license](assets/lucide/LICENSE).

A native Rust + windows-rs Windows tray app for short, on-demand performance investigations. No webview, browser engine, background service, scheduled sampler, telemetry or network upload. Windows 10/11 x64; GPU counters require a compatible WDDM driver. This is a preview. [Changelog](CHANGELOG.md) · [Security](SECURITY.md) · [SBOM](sbom/INVENTORY.md) · [CRA documentation](docs/cra/README.md).

## Run and install

Download `SuperOpti-windows-x64.zip` from the repository's Releases page and extract it. Run `superopti.exe` for portable use. Choose **Install for me** to install for the current user, or run `Install.ps1` in PowerShell. The installer copies to `%LOCALAPPDATA%\SuperOpti`, creates a Start-menu shortcut, and registers an uninstall entry in Windows Settings. No admin account is required for installation. Files are unsigned; managed-device policy may prohibit execution.

**Startup on/off** changes only SuperOpti's HKCU Run entry. Autostart launches `--tray` with monitoring OFF. Install before enabling autostart so the executable has a stable location. The optional installer `-AutoStart` switch enables the same behavior. Closing hides the app in the tray; minimizing uses the taskbar; use **Exit** or the tray menu to quit. A second launch reveals the existing app.

Exit the installed app before updating or uninstalling it. Uninstall using Windows Settings or the installed `Uninstall.ps1`. Capture files and fix backups are retained; use Undo before uninstalling if you want settings restored.

## Capture slowness

1. Open the tray icon when the machine feels slow.
2. Choose 2, 5 or 15 minutes. The default interval is 2 seconds; enable **5s interval** to sample less often. The first interval warms rate counters.
3. Follow CPU, RAM, GPU and aggregate disk busy in the Signal dashboard. The history graph spans two minutes on a 0–100% scale. Open CPU details for individual logical processor graphs. Pin the window, switch to compact view, or choose opacity while pinned.
4. Search contributors by process name/PID. Double-click a row (or select and press Enter) for Process details. Inspect threads, open files, descendants, associated services and network endpoints. Hover for executable manufacturer, version, size and location. Icons and metadata are cached; inaccessible fields remain unavailable.
5. Stop early or let the capture stop automatically. **Export JSON** saves the current session to `%LOCALAPPDATA%\SuperOpti\captures`. A new capture replaces the previous in-memory session after confirmation. Nothing is saved unless exported.

Sampling stops on completion, Stop, or Exit. While idle the worker blocks on a channel and the GUI blocks on the Win32 message loop: no monitoring timer, polling loop, periodic log writes, or continuous rendering. During monitoring, a native core collector and four optional provider collectors run as hidden child processes; GDI graphs repaint on samples and normal window events. At most 900 samples are retained. System checks run only when requested and may briefly add overhead through Windows CIM/PowerShell. Thread inspection enumerates threads once, opens only those owned by the chosen PID and measures their CPU times over two seconds. It runs in a separate helper with a 12-second timeout. Checks and fixes run serially on a separate action worker; PowerShell commands have a 30-second timeout. The capture coordinator remains responsive during these actions. A watchdog enforces the capture deadline even when a Windows counter provider stalls. Owned helper processes are terminated on Stop, completion and Exit; kill-on-close Windows Job Objects also enforce cleanup on application termination. The command coordinator does not wait for slow driver I/O cancellation. No active samplers remain while idle; terminated helper process objects may briefly remain while Windows cancels pending driver I/O. Optional metrics may take longer to initialize and become N/A when their last reading is older than max(15 seconds, 3 sample intervals).

## Interpreting the data

- **CPU:** elapsed-time processor usage through GetSystemTimes; on machines with more than one processor group, the system CPU graph is N/A to avoid reporting one group as the entire machine. Process CPU is normalized to whole-system capacity across all groups. This differs from Task Manager's frequency-adjusted utility metric. Thread snapshot CPU is percent of one logical processor.
- **RAM:** system physical memory load. Process RAM is working set, including shared pages; do not sum it as private memory. **Commit** is percentage of the system commit limit, not RAM or swap. **Pagefile** is allocated paging-file usage, not page-in traffic. Pages Input/sec also includes file-backed hard faults.
- **GPU:** system value is the busiest physical GPU engine after summing its process instances. Per-process values sum engines and cap at 100%, so parallel engines can overstate whole-device load; no GPU value means unavailable, not zero. There is no VRAM or temperature sensor support in this version.
- **Disk:** aggregate physical-disk busy = 100 - idle. Also reports throughput, queue length and average transfer latency. Aggregate values can hide one saturated device. Process **I/O** includes disk, network and device I/O; it is not a measurement of per-process disk throughput. Network totals may double-count virtual interfaces.
- **Top 10:** process instances, not a grouped application view. Score = maximum of CPU%, GPU%, memory share (weighted 1.0 at >=85% system RAM, otherwise 0.2), private commit as a share of the commit limit (weighted 1.0 at >=85% system commit, otherwise 0.2), and `min(I/O MiB/s * 2, 100)` (weighted 1.0 at >=80% aggregate disk busy, otherwise 0.2). The question-mark control explains the formula. Per-process swap is unavailable; commit minus working set is not swap. Observed process core occupancy is not yet collected and is explicitly marked. High thread/handle counts alone do not establish a fault.
- Uses Toolhelp process snapshots, GetProcessTimes, GetProcessMemoryInfo, GetProcessHandleCount and GetProcessIoCounters for core readings. Rate baselines include creation time to protect against PID reuse. Supplemental metrics use language-neutral English PDH counters in isolated providers. Newly created/exited processes or restricted/missing counters can show N/A. Hardware and access limitations are not silently converted to healthy readings.
- This is not an ETW stack profiler: driver stalls, thermal throttling, individual device saturation and exact blocked-thread causes may require Windows Performance Recorder/Analyzer or vendor tools.

## Checks and fixes

**System checks** reviews fixed-drive free space (15% is a review threshold, not a universal requirement), pagefile management, startup entries, servicing restart flags, uptime, the current power plan, and client-area animations. Unknown results are identified explicitly. It does not scan files or run a stress test.

Three actions are available individually and through **Fix all (3 settings)**:

- Disable client-area animations; optional, with potentially small performance benefit.
- Switch the standard Power saver plan to Balanced only when active; may use more energy. Other plans are preserved.
- Set **initial = maximum = max(50% installed physical RAM,16 GiB)**, rounded up to MiB. Examples: 16/32 GiB RAM → 16 GiB pagefile; 64 GiB → 32 GiB; 128 GiB → 64 GiB. Checks preview size, configured/running allocation and disk eligibility. Retains one existing location, refuses multiple/custom layouts, and reserves room for growth plus max(2 GiB,5% volume). Requires administrator rights and restart; neither occurs automatically. Windows swapfile.sys is separate.

Fixes require confirmation and report each result. Animation/power originals are saved in `%LOCALAPPDATA%\SuperOpti\fix-backup.json`; pagefile originals under `HKLM\SOFTWARE\SuperOpti`. **Undo fixes** restores them; pagefile restore requires restart. Fixed sizing is the selected policy, not a universal optimum: it limits commit headroom and can prevent complete crash dumps. [Microsoft sizing guidance](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/how-to-determine-the-appropriate-page-file-size-for-64-bit-versions-of-windows).

Storage/startup/update buttons open Windows settings for manual review. Fix all does not delete files, disable security/services, terminate processes, install updates or reboot.

## Process and network details

Focused reports include path/creation/parent identity, user/kernel CPU, working/private/peak memory, handles, threads, and separate READ/IN and WRITE/OUT I/O totals and rates. Process I/O includes files, network and devices; it is not TCP-only.

**Traffic history** records TCP/UDP flow byte totals for a short on-demand session using Windows ETW. Windows asks for UAC approval when the private recorder needs it; the main UI remains unelevated. No packet payloads are saved. The live table refreshes every two seconds and retains the latest 500 one-second flow buckets; full retained data remains in the local archive. Browsing or applying filters pauses visual follow. **Follow live** returns to new rows without stopping recording.

Search saved history by process, numeric endpoint or port, filter TCP/UDP and UTC time ranges, and group by process/endpoint, process or destination. Export saves the currently displayed page. Session storage is capped at 64 MiB and the archive at 256 MiB; old data is not silently deleted. Loss, unsupported-event counts and capture errors make coverage incomplete. IPv4 and IPv6 addresses are retained without reverse DNS. Link-local IPv6 scope correlation and connection-instance accounting still need additional validation; do not treat endpoint buckets as an exact count of transport connections.

**Network devices** uses an explicit neighbor-table scan and local change history. Optional discovery probes are bounded; known IPv6 neighbors can be reprobed without attempting to enumerate an IPv6 subnet. Absence is not proof that a device is offline. These are original Windows-native implementations informed by NetAlertX/netshow feature concepts, not embedded copies of those projects.

Open-file snapshots list process-wide handles; they do not establish which thread used a file. **Open executable folder** and file-folder actions select the item in Explorer without launching it. Protected processes and unavailable metadata are reported as such. Endpoints are not proof of domain ownership, intent or malicious activity. Details do not activate continuous background tracing.

## Development and release policy

The repository workspace is `C:\Users\Andreas\workspace\superopti`, on `main`.

```powershell
cargo fmt --all
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
./scripts/Build.ps1
./target/release/superopti.exe --smoke-test work/smoke-result.json
```

Requires Rust 1.97.1 (rust-toolchain.toml), Python 3.12 (`pip install -r scripts/requirements-sbom.txt`), `cargo install cargo-cyclonedx --version 0.5.9 --locked`, `cargo install cargo-audit --version 0.22.2 --locked`, an MSVC toolchain and Visual Studio C++ build tools/Windows SDK. `windows` 0.62.2 is the only OS/UI framework; serde handles local JSON. Cargo.lock pins dependency resolution. The native app uses unsafe Win32 calls at the FFI boundary, owned process/query handles, and Windows Job Objects for cleanup. The smoke test checks native counters, a real 10-second capture deadline, immediate cancellation during provider initialization, no events while idle, and native thread inspection. Set SUPEROPTI_TRACE to a local file path only when debugging PDH initialization; normal operation writes no such log.

Commit and push every completed, validated change to `main`. Each push (or manual workflow run) checks formatting, lint and tests, builds the Windows x64 distribution, and publishes a uniquely tagged preview release with EXE, installer ZIP, CycloneDX inventory, SBOM/license/audit and CRA bundles, and SHA-256 checksums. Failed pipelines publish no release. The build workflow is the release build authority; local intermediate compiler/test iterations are not separate published products. No force-push is needed. Release artifacts do not contain diagnostic captures.

## References

- [Microsoft: collecting performance data](https://learn.microsoft.com/en-us/windows/win32/perfctrs/collecting-performance-data)
- [Microsoft: language-neutral PDH counters](https://learn.microsoft.com/en-us/windows/win32/api/pdh/nf-pdh-pdhaddenglishcounterw)
- [Microsoft: Windows performance improvement guidance](https://support.microsoft.com/en-us/windows/experience/performance-optimization/tips-to-improve-pc-performance-in-windows)
- [windows-rs](https://github.com/microsoft/windows-rs)


## Inventory and CRA scope

[Inventory](sbom/INVENTORY.md) covers every locked transitive/build/proc-macro Cargo package, relationships, verified archive hashes and license texts. Release evidence adds binary hash, Rust toolchain/standard library and direct Windows PE imports. CycloneDX 1.5 matches the pinned generator and is validated with its official vendored schema. OS-internal transitive components vary by environment; this boundary is explicit and overall composition is marked incomplete. A clean audit is not proof of no vulnerabilities.

The maintainer declares individual non-commercial MIT distribution. [CRA documentation](docs/cra/README.md) records conditional scope, technical evidence, requirements mapping, risks and gaps. It does not assert certification or CE marking. [Private security reporting](https://github.com/andreaswiren/superopti/security/advisories/new) is enabled.
