# Release and vulnerability procedure

## Each distribution build

1. Review changes/changelog; exclude host captures, endpoints, secrets and private Cargo paths.
2. Run pinned Rust fmt, clippy with warnings denied and unit tests.
3. Build resources/executable; run native capture/deadline/cancellation smoke test.
4. Generate all-package CycloneDX, verify crate archives/lock graph, validate official vendored schema, include license texts and binary/runtime provenance.
5. Audit current RustSec database; fail on vulnerabilities, retain database revision/results, separately review warnings and application risks.
6. Bundle EXE/installers, README/license/changelog/security/CRA/SBOM and SHA-256 checksums. Publish unique preview tag against tested commit only after gates pass.
7. Inspect workflow/assets; document only performed validation. Exact-release inventory supersedes a working-tree snapshot.

## Vulnerability reports

Use SECURITY.md private channel. Record report time, versions, reproduction, severity, exploitation status and owner. Coordinate a reviewed fix, regression/security tests, updated inventory, corrected release and advisory. Respect reporter consent and protect captures. If regulatory reporting applies, follow SCOPE.md and current ENISA SRP deadlines; do not wait for a fix before an applicable early warning.

## Open gates before commercial conformity claims

- Reassess scope, category, conformity route and manufacturer/steward obligations.
- Establish actual manufacturer identity/address/contact, support lifetime/end date and resourced vulnerability handling.
- Address update authenticity; unsigned preview checksums are insufficient publisher authentication.
- Complete admin/pagefile/reboot/Undo, install/update/uninstall, accessibility, low-resource/high-core, negative-path and security testing.
- Review component/runtime coverage, license obligations and provenance; approve threat treatment.
- Determine applicable standards/assessment route, preserve required technical evidence and user information before any valid declaration/marking.

This procedure does not claim all manual gates have passed. No recurring monitoring or regulatory notifications are sent automatically.
