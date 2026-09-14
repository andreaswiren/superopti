# Technical file

## Product and user information

SuperOpti 0.3.0 development candidate, Windows 10/11 x64 native preview, MIT. Maintainer/contact: [SECURITY.md](../../SECURITY.md). Purpose: local short investigations of slowness and process activity. API compatibility does not extend the operating system vendor's security-support lifecycle. [README](../../README.md) documents installation, operation, metrics, updates, Undo and limitations. Release acceptance is recorded separately in VALIDATION.md.

## Architecture and data flows

1. A windows-rs Win32 message loop renders GDI graphs/Common Controls. Idle GUI/worker channels block without a monitoring timer or browser runtime.
2. Explicit captures launch hidden owned collectors in kill-on-close Job Objects. Native process APIs provide core readings; four isolated optional PDH providers supplement them. Stop/deadline terminates helper threads and closes jobs; Windows may finish pending I/O cancellation afterward without blocking the command coordinator. Unavailable metrics stay N/A.
3. A serial action worker handles checks, fixes and focused inspection. Thread inspection uses a bounded child helper. A held process handle and creation time defend against PID reuse. Reports remain local UI text.
4. IP Helper tables expose numeric TCP/UDP endpoints. An explicit private ETW session records TCP/UDP IPv4/IPv6 endpoint byte buckets. Two-second updates feed a bounded 500-row live view; full retained history is capped at 64 MiB/session and 256 MiB/archive. A private helper/watchdog limits capture lifetime. No DNS lookup or packet payload capture occurs.
5. Process metadata and open-file inspections run in bounded helper processes. Process identities include creation time. Windows service and descendant snapshots are requested explicitly. File handles are process-wide; no thread ownership is inferred. Explorer folder actions do not execute inspected files.
5. Explicit export writes bounded capture JSON under LOCALAPPDATA. Installation/autostart use current-user locations. On-demand checks invoke built-in PowerShell/CIM. PowerShell resolves from the Windows system directory; captured output is capped and drained concurrently; operations time out after thirty seconds.

Source evidence: src/main.rs, ui.rs, engine.rs, native.rs, network.rs, metrics.rs, actions.rs and scripts/Pagefile.ps1. Resources: build.rs, resources/app.manifest and assets/. Supply chain: sbom/ and scripts/generate_sbom.py.

## Permissions and mutations

Default manifest is asInvoker. Windows ACLs restrict readable processes. A network recording that needs administrator access requests UAC for a dedicated helper; the main UI remains unelevated. The helper accepts only its fixed recording protocol over a local named pipe, with peer PID checks and an owner/administrator/system DACL. Parent-side code owns capture paths and writes. Pagefile writes still require administrator rights; automatic targeted pagefile elevation remains outstanding. Windows is never restarted automatically. Individual/Fix-all confirmation describes concrete changes.

Animation and power-plan originals are saved in the user profile. Fixed pagefile policy is max(half installed RAM,16 GiB), initial=max, rounded up to MiB. It retains one existing location, refuses multiple/custom layouts and checks growth plus max(2 GiB,5% volume) reserve. The original automatic-management flag and PagingFiles multi-string are saved under HKLM\SOFTWARE\SuperOpti before changes. Configuration is verified, errors trigger attempted rollback, Undo verifies restoration, and a restart activates changes. This selected policy can limit commit headroom and complete crash dumps; Windows swapfile.sys is separate.

## Measurement and privacy boundaries

Process read/write I/O includes files, network and devices. It is distinct from ETW transport-event byte accounting. Traffic coverage includes reported lost events/buffers, aggregation drops, unsupported event layouts and recording failures. IPv6 addresses are preserved, but link-local scope correlation and connection-instance boundaries require further validation; endpoint buckets are not an exact transport connection inventory. A listener does not prove remote communication; numeric endpoints do not establish domains, intent or ownership. An optional bounded device-discovery action sends probes only when requested.

Names, paths and endpoints may expose activity. No telemetry/upload is implemented. Local files use profile permissions without application encryption. Captures are excluded from releases. Uninstall retains captures/backups. Details reports are not silently appended to exports.

## Build and evidence

Rust 1.97.1 MSVC, windows 0.62.2, locked dependencies. SDK resource compiler embeds the adapted Lucide Signal icons and a per-monitor-DPI/asInvoker manifest. Pinned upstream SVGs, licenses and adaptation geometry are included in the source inventory. Main pushes run formatting, lint, tests, optimized build, SBOM/schema/hash validation, advisory audit and native lifecycle smoke before publishing a uniquely tagged unsigned preview.

The inventory covers every resolved Cargo dependency, including build/proc macros, relationships, archive hashes and license texts. It records Rust standard library/toolchain, direct PE imports, binary hash and source revision. OS-internal transitive dependencies and serviced DLL versions vary by environment and are not exhaustively inventoried. This limitation is explicit in CycloneDX composition.

[VALIDATION](../../VALIDATION.md) records actual evidence. No full accessibility audit, penetration test, signed updater or commercial support guarantee exists.

References: [Microsoft TCP counters](https://learn.microsoft.com/en-us/windows/win32/api/tcpestats/ns-tcpestats-tcp_estats_data_rod_v0), [counter enablement](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-setpertcpconnectionestats), [pagefile sizing/dump requirements](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/how-to-determine-the-appropriate-page-file-size-for-64-bit-versions-of-windows).
