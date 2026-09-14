# CRA requirements mapping

Voluntary review under the facts in SCOPE.md. Status describes engineering evidence, not certification. References: [Regulation (EU) 2024/2847](https://eur-lex.europa.eu/eli/reg/2024/2847/oj/eng). Annex I applicability is risk-based.

| Reference / subject | Current evidence | Gap / follow-up |
|---|---|---|
| Annex I I(1), risk-appropriate security | Local-only architecture and risk register; partial | Independent threat/security review and residual-risk approval |
| I(2)(a), known exploitable vulnerabilities | Release-gated locked dependency audit; partial | Application security review/fuzzing; audit is not proof of no exploitable defects |
| I(2)(b), secure defaults/reset | Monitoring/autostart off, asInvoker, fix Undo; partial | Complete administrator/installer/recovery test matrix |
| I(2)(c), correction/updates | Corrected releases and manual update procedure; partial | Secure update/notification design and support period |
| I(2)(d), unauthorized access | No remote control interface, OS ACLs; partial | Elevated execution from user-writable install remains a trust risk |
| I(2)(e), confidentiality | No upload/payload recording, profile ACLs; partial | Exports unencrypted; assess deployment-specific protection needs |
| I(2)(f), integrity | Lock/archive hashes, binary checksums, pinned actions/tools; partial | Authenticode/provenance signing and protected release governance |
| I(2)(g), minimization | Bounded sessions, explicit export, numeric endpoints | Revisit new collection features and retention |
| I(2)(h), availability | Jobs, provider isolation, Stop/deadline tests; partial | Broaden low-resource/high-core and failure tests |
| I(2)(i), effects on other services | No idle sampling, bounded polling; partial | Diverse hardware overhead benchmarks; fixed pagefile limits commit |
| I(2)(j), attack surface | No service, driver, browser or network listener; partial | Review unsafe FFI and privileged settings paths |
| I(2)(k), exploit impact | Standard user default, capped buffers, owned helpers; partial | Fuzz native/IPC data; action worker is not sandboxed |
| I(2)(l), security information | Action results and unknown/failure feedback; partial | No persistent security event log; assess requirements |
| I(2)(m), removal/transfer | Optional exports and retention/removal instructions; partial | Uninstall retains files; complete deletion QA |
| Annex I II(1), components/vulnerabilities | Complete Cargo CycloneDX graph/hashes/licenses and audit | Overall OS runtime coverage explicitly incomplete |
| II(2), remediation | SECURITY triage/fix/release policy | No guaranteed SLA/support lifetime |
| II(3), regular review/testing | Release tests/audit; partial | No independent penetration test or scheduled advisory monitoring |
| II(4), public vulnerability information | Advisory procedure | Publish verified affected/fixed versions when incidents arise |
| II(5), coordinated disclosure | Private GitHub reporting enabled | Exercise triage/disclosure and maintain coverage |
| II(6), reporting contact | Private GitHub reporting link; partial | No separate email; enduring incident ownership needed if required |
| II(7), secure update distribution | HTTPS releases/checksums; partial | Unsigned binaries; hashes alone do not authenticate publisher |
| II(8), timely/free updates/advice | Free previews and changelog; policy | Establish supported lifetime/notice obligations if in scope |
| Annex II, user information | README, security, metrics, install/update/Undo; partial | Formal manufacturer address/support end/conformity info absent under current scope |
| Annex VII, technical file | Architecture, source, risk, SBOM, validation; partial | No conformity/lab record, applied-standard list or EU declaration |
| Articles 13/14/24 | Conditional scope/reporting assessment | Reassess commercial/steward facts before distribution changes |
