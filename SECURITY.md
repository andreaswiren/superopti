# Security policy

SuperOpti is maintained by an individual through [andreaswiren](https://github.com/andreaswiren), distributed non-commercially under MIT. Maintenance is best effort for the latest preview. No guaranteed response SLA, support end date or multi-year commitment is established. Earlier previews are superseded; update manually from repository Releases.

## Report privately

Use [GitHub private vulnerability reporting](https://github.com/andreaswiren/superopti/security/advisories/new), enabled for this repository. Include affected build, reproduction, impact, Windows version and a sanitized example. Do not put exploit details, private endpoints, captures or credentials in public issues. This channel requires a GitHub account; no separate security email is designated.

The maintainer triages severity/affected versions, reproduces safely, coordinates a tested fix, audits dependencies and publishes a corrected release/advisory. Reporter credit requires consent. No response timeline is promised. A clean audit means no matching known advisory in that database at that time, not absence of all vulnerabilities.

## Operation and updates

Run as a standard user normally. No service, driver, listening endpoint, telemetry or automatic updater is implemented. Explicit network accounting, scheduler observations and pagefile changes may need administrator rights. ETW observations request UAC for a limited helper; the main UI remains unelevated. Helpers exchange bounded messages over a local ACL-restricted pipe and accept no arbitrary output paths. A per-user installation is user-writable and is not a privileged trust boundary. Do not elevate an untrusted executable or installer.

Release checksums detect changes when compared with trusted metadata; they are not publisher signatures. Binaries are unsigned. Verify repository origin, choose the latest corrected release, exit the running app and replace/reinstall. No background update traffic occurs.

Local exports are opt-in, unencrypted and protected by Windows profile permissions. Process names, paths, PIDs and endpoints may be sensitive. Sanitize before sharing, and delete captures when no longer needed. Uninstall retains captures and fix backups; use Undo first if desired. Machine-wide pagefile backup in HKLM\SOFTWARE\SuperOpti is retained until Undo succeeds.

See [CRA scope](docs/cra/SCOPE.md), [risk register](docs/cra/RISK_ASSESSMENT.md) and [SBOM](sbom/INVENTORY.md). These document evidence and gaps, not certification.
