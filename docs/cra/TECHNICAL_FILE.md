# Technical file

## Product and user information

SuperOpti 0.2.0 Windows 10/11 x64 native preview, MIT. Maintainer/contact: [SECURITY.md](../../SECURITY.md). Purpose: local short investigations of slowness and process activity. API compatibility does not extend the operating system vendor's security-support lifecycle. [README](../../README.md) documents installation, operation, metrics, updates, Undo and limitations.

## Architecture and data flows

1. A windows-rs Win32 message loop renders GDI graphs/Common Controls. Idle GUI/worker channels block without a monitoring timer or browser runtime.
2. Explicit captures launch hidden owned collectors in kill-on-close Job Objects. Native process APIs provide core readings; four isolated optional PDH providers supplement them. Stop/deadline terminates helper threads and closes jobs; Windows may finish pending I/O cancellation afterward without blocking the command coordinator. Unavailable metrics stay N/A.
3. A serial action worker handles checks, fixes and focused inspection. Thread inspection uses a bounded child helper. A held process handle and creation time defend against PID reuse. Reports remain local UI text.
4. IP Helper tables expose numeric TCP/UDP endpoints. Explicit TCP EStats measures ten seconds, polling once per second, capped at 256 established connections. Counters enabled by the app are restored where possible. There is no DNS lookup, payload capture or external service call.
5. Explicit export writes bounded capture JSON under LOCALAPPDATA. Installation/autostart use current-user locations. On-demand checks invoke built-in PowerShell/CIM. PowerShell resolves from the Windows system directory; captured output is capped and drained concurrently; operations time out after thirty seconds.

Source evidence: src/main.rs, ui.rs, engine.rs, native.rs, network.rs, metrics.rs, actions.rs and scripts/Pagefile.ps1. Resources: build.rs, resources/app.manifest and assets/. Supply chain: sbom/ and scripts/generate_sbom.py.

## Permissions and mutations

Default manifest is asInvoker. Windows ACLs restrict readable processes. TCP enabling may need an explicitly administrator-launched app. Pagefile writes require administrator rights. The application never elevates itself or automatically restarts Windows. Individual/Fix-all confirmation describes concrete changes.

Animation and power-plan originals are saved in the user profile. Fixed pagefile policy is max(half installed RAM,16 GiB), initial=max, rounded up to MiB. It retains one existing location, refuses multiple/custom layouts and checks growth plus max(2 GiB,5% volume) reserve. The original automatic-management flag and PagingFiles multi-string are saved under HKLM\SOFTWARE\SuperOpti before changes. Configuration is verified, errors trigger attempted rollback, Undo verifies restoration, and a restart activates changes. This selected policy can limit commit headroom and complete crash dumps; Windows swapfile.sys is separate.

## Measurement and privacy boundaries

Process read/write I/O includes files, network and devices. It is distinct from TCP bytes. EStats payload octets include retransmissions but not headers. Snapshots only expose already-enabled counters; explicit measurements use deltas, not reconstructed history. Polling misses short flows/closing bytes, so totals are lower bounds. UDP/QUIC remote peers and per-peer bytes are unavailable. A listener does not prove remote communication; numeric endpoints do not establish domains, intent or ownership.

Names, paths and endpoints may expose activity. No telemetry/upload is implemented. Local files use profile permissions without application encryption. Captures are excluded from releases. Uninstall retains captures/backups. Details reports are not silently appended to exports.

## Build and evidence

Rust 1.97.1 MSVC, windows 0.62.2, locked dependencies. SDK resource compiler embeds original icons and asInvoker/system-DPI manifest. Main pushes run formatting, lint, tests, optimized build, SBOM/schema/hash validation, advisory audit and native lifecycle smoke before publishing a uniquely tagged unsigned preview.

The inventory covers every resolved Cargo dependency, including build/proc macros, relationships, archive hashes and license texts. It records Rust standard library/toolchain, direct PE imports, binary hash and source revision. OS-internal transitive dependencies and serviced DLL versions vary by environment and are not exhaustively inventoried. This limitation is explicit in CycloneDX composition.

[VALIDATION](../../VALIDATION.md) records actual evidence. No full accessibility audit, penetration test, signed updater or commercial support guarantee exists.

References: [Microsoft TCP counters](https://learn.microsoft.com/en-us/windows/win32/api/tcpestats/ns-tcpestats-tcp_estats_data_rod_v0), [counter enablement](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-setpertcpconnectionestats), [pagefile sizing/dump requirements](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/how-to-determine-the-appropriate-page-file-size-for-64-bit-versions-of-windows).
