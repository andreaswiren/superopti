# SuperOpti

A native Rust + windows-rs Windows tray app for short, on-demand performance investigations. No webview, browser engine, background service, scheduled sampler, telemetry or network upload. Windows 10/11 x64; GPU counters require a compatible WDDM driver. This is an initial preview.

## Run and install

Download `SuperOpti-windows-x64.zip` from the repository's Releases page and extract it. Run `superopti.exe` for portable use. Choose **Install for me** to install for the current user, or run `Install.ps1` in PowerShell. The installer copies to `%LOCALAPPDATA%\SuperOpti`, creates a Start-menu shortcut, and registers an uninstall entry in Windows Settings. No admin account is required for installation. Files are unsigned; managed-device policy may prohibit execution.

**Startup on/off** changes only SuperOpti's HKCU Run entry. Autostart launches `--tray` with monitoring OFF. Install before enabling autostart so the executable has a stable location. The optional installer `-AutoStart` switch enables the same behavior. Closing or minimizing hides the app; use **Exit** or the tray menu to quit. A second launch reveals the existing app.

Exit the installed app before updating or uninstalling it. Uninstall using Windows Settings or the installed `Uninstall.ps1`. Capture files and fix backups are retained; use Undo before uninstalling if you want settings restored.

## Capture slowness

1. Open the tray icon when the machine feels slow.
2. Choose 2, 5 or 15 minutes. The default interval is 2 seconds; enable **5s interval** to sample less often. The first interval warms rate counters.
3. Follow CPU, RAM, GPU, aggregate disk busy, commit and pagefile graphs. The displayed trend spans the last two minutes and uses a 0–100% scale.
4. Review the live top 10 likely contributing processes. Enter a PID and choose **Inspect threads** for a separate 2-second snapshot of its 40 busiest readable threads, kernel/user CPU split, base priorities and exit status.
5. Stop early or let the capture stop automatically. **Export JSON** saves the current session to `%LOCALAPPDATA%\SuperOpti\captures`. A new capture replaces the previous in-memory session after confirmation. Nothing is saved unless exported.

Sampling stops on completion, Stop, or Exit. While idle the worker blocks on a channel and the GUI blocks on the Win32 message loop: no monitoring timer, polling loop, periodic log writes, or continuous rendering. During monitoring, a native core collector and four optional provider collectors run as hidden child processes; GDI graphs repaint on samples and normal window events. At most 900 samples are retained. System checks run only when requested and may briefly add overhead through Windows CIM/PowerShell. Thread inspection enumerates threads once, opens only those owned by the chosen PID and measures their CPU times over two seconds. It runs in a separate helper with a 12-second timeout. Checks and fixes run serially on a separate action worker; PowerShell commands have a 30-second timeout. The capture coordinator remains responsive during these actions. A watchdog enforces the capture deadline even when a Windows counter provider stalls. Windows Job Objects close all collector processes on Stop, completion, Exit or application termination. No collectors remain resident while idle. Optional metrics may take longer to initialize and become N/A when their last reading is older than max(15 seconds, 3 sample intervals).

## Interpreting the data

- **CPU:** elapsed-time processor usage through GetSystemTimes; on machines with more than one processor group, the system CPU graph is N/A to avoid reporting one group as the entire machine. Process CPU is normalized to whole-system capacity across all groups. This differs from Task Manager's frequency-adjusted utility metric. Thread snapshot CPU is percent of one logical processor.
- **RAM:** system physical memory load. Process RAM is working set, including shared pages; do not sum it as private memory. **Commit** is percentage of the system commit limit, not RAM or swap. **Pagefile** is allocated paging-file usage, not page-in traffic. Pages Input/sec also includes file-backed hard faults.
- **GPU:** system value is the busiest physical GPU engine after summing its process instances. Per-process values sum engines and cap at 100%, so parallel engines can overstate whole-device load; no GPU value means unavailable, not zero. There is no VRAM or temperature sensor support in this version.
- **Disk:** aggregate physical-disk busy = 100 - idle. Also reports throughput, queue length and average transfer latency. Aggregate values can hide one saturated device. Process **I/O** includes disk, network and device I/O; it is not a measurement of per-process disk throughput. Network totals may double-count virtual interfaces.
- **Top 10:** process instances, not a grouped application view. Score = maximum of CPU%, GPU%, memory share (weighted 1.0 at >=85% system RAM, otherwise 0.2), and `min(I/O MiB/s * 2, 100)` (weighted 1.0 at >=80% aggregate disk busy, otherwise 0.2). This heuristic flags likely contributors and is not proof that a process caused slowness. High thread/handle counts alone do not establish a fault. The dropdown changes ordering within the current pressure-ranked top 10.
- Uses Toolhelp process snapshots, GetProcessTimes, GetProcessMemoryInfo, GetProcessHandleCount and GetProcessIoCounters for core readings. Rate baselines include creation time to protect against PID reuse. Supplemental metrics use language-neutral English PDH counters in isolated providers. Newly created/exited processes or restricted/missing counters can show N/A. Hardware and access limitations are not silently converted to healthy readings.
- This is not an ETW stack profiler: driver stalls, thermal throttling, individual device saturation and exact blocked-thread causes may require Windows Performance Recorder/Analyzer or vendor tools.

## Checks and fixes

**System checks** reviews fixed-drive free space (15% is a review threshold, not a universal requirement), pagefile management, startup entries, servicing restart flags, uptime, the current power plan, and client-area animations. Unknown results are identified explicitly. It does not scan files or run a stress test.

Two direct actions are implemented, individually and through **Fix all (2 fixes)**:

- Disable client-area animations with SystemParametersInfoW; optional, affects appearance, potentially small performance benefit.
- Change the standard Power saver plan to Balanced only if Power saver is currently active; may use more energy. Custom/high-performance plans are preserved. Windows power-mode overlays need manual review.

The confirmation describes the concrete changes. Original values are saved before mutation to `%LOCALAPPDATA%\SuperOpti\fix-backup.json`; **Undo fixes** restores them. Fix all reports each result, including failures. Policy restrictions can prevent changes. Storage, startup, updates, virtual memory and power settings buttons open Windows controls for choices requiring human judgment. Fix all does not delete files, disable security/services/pagefiles, terminate processes, install updates, reboot, or make arbitrary registry performance tweaks.

## Development and release policy

The repository workspace is `C:\Users\Andreas\workspace\superopti`, on `main`.

```powershell
cargo fmt --all
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
./scripts/Build.ps1
./target/release/superopti.exe --smoke-test work/smoke-result.json
```

Requires a current Rust MSVC toolchain and Visual Studio C++ build tools/Windows SDK. `windows` 0.62.2 is the only OS/UI framework; serde handles local JSON. Cargo.lock pins dependency resolution. The native app uses unsafe Win32 calls at the FFI boundary, owned process/query handles, and Windows Job Objects for cleanup. The smoke test checks native counters, a real 10-second capture deadline, immediate cancellation during provider initialization, no events while idle, and native thread inspection. Set SUPEROPTI_TRACE to a local file path only when debugging PDH initialization; normal operation writes no such log.

Commit and push every completed, validated change to `main`. Each push (or manual workflow run) checks formatting, lint and tests, builds the Windows x64 distribution, and publishes a uniquely tagged preview release with EXE, ZIP installer bundle and SHA-256 checksums. Failed pipelines publish no release. The build workflow is the release build authority; local intermediate compiler/test iterations are not separate published products. No force-push is needed. Release artifacts do not contain diagnostic captures.

## References

- [Microsoft: collecting performance data](https://learn.microsoft.com/en-us/windows/win32/perfctrs/collecting-performance-data)
- [Microsoft: language-neutral PDH counters](https://learn.microsoft.com/en-us/windows/win32/api/pdh/nf-pdh-pdhaddenglishcounterw)
- [Microsoft: Windows performance improvement guidance](https://support.microsoft.com/en-us/windows/experience/performance-optimization/tips-to-improve-pc-performance-in-windows)
- [windows-rs](https://github.com/microsoft/windows-rs)

