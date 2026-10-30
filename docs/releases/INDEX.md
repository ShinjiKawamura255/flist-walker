# Release Records Index

This directory contains records and evidence from specific release or candidate runs. It does not define the release procedure.

GitHub Releases is authoritative for published releases. At this repository revision, the latest published release is [v0.32.0 (2026-10-28 UTC)](https://github.com/ShinjiKawamura255/flist-walker/releases/tag/v0.32.0).

## Release And Candidate Records

| Record | Role |
| --- | --- |
| [v0.32.1 Preparation Packet](v0.32.1.md) | UI/theme/PIN/header and Windows path repair; current candidate gates and evidence-reuse boundaries |
| [v0.32.0 Execution Packet](v0.32.0.md) | Published source/tag, first candidate and native TC193, retained hosted-runner failure/one targeted recovery, signed tagged/public bytes, Latest and actual N-1 |
| [v0.31.1 Execution Packet](v0.31.1.md) | Published source/tag, Windows native verification, separate candidate/tagged/public gates and retained external-install CI failure/retry |
| [v0.31.0 Regenerated Candidate39810727499](evidence/v0.31.0/regenerated-39810727499/REPORT.md) | Source d1bf330; fresh audit failed on two TLS dependencies, so distribution/signature/actual N-1 remain unverified; updated Windows preparation |
| [v0.31.0 Linux Coverage Diagnosis](evidence/v0.31.0/LINUX-COVERAGE-DIAGNOSIS.md) | Unresolved SIGSEGV, exact diagnostic observations and merge hold |
| [v0.31.0 Execution Packet](evidence/v0.31.0/PREPARATION.md) | Published source/tag, separate candidate/tagged/public byte and warning gates, restored PR201 activation, rejected README candidate and explicit native/unknown-cause deviations |
| [v0.30.2 Execution Packet](v0.30.2.md) | Published source, candidate/tagged/public assets, native deviations and Windows startup measurements |
| [v0.30.1 Execution Packet](v0.30.1.md) | Published tag/source, separate candidate/tagged warning decisions, fresh assets/public readback and selected reuse |
| [v0.30.0 Execution Packet](v0.30.0.md) | Published tag, assets, validation selection, GUI repairs, approved deviations and exact-run warnings |
| [v0.29.0 Release Record](v0.29.0.md) | Published tag, workflows, assets, GUI deviations, warning dispositions, validation, and review evidence |
| [v0.28.0 Release Record](v0.28.0.md) | Published tag, workflows, assets, validation, and final review evidence |
| [v0.27.1 Release Record](v0.27.1.md) | Tag, candidate and tagged workflows, assets, GUI validation, warnings, and final review evidence |
| [v0.18.10 Release Record](v0.18.10.md) | Tag, CI, assets, native/GUI validation, warning decision, and review evidence |
| [v0.18.9 Candidate Rejection](v0.18.9-candidate-rejection.md) | Rejection decision, validation result, reason, and remediation |
| [v0.18.8 Release Notes](v0.18.8.md) | Published release content retained for reference |
| [v0.18.7 Release Notes](v0.18.7.md) | Published release content retained for reference |

## Evidence Collections

| Collection | Contents |
| --- | --- |
| `evidence/v0.32.0/` | Published28 assets/signature/N-1/readback, native raw timing rows, original tagged failure/new logs and explicit carry-forward reconciliation, independent reviews and sanitized observer snapshots |
| `evidence/v0.31.1/` | Native source/candidate results, signed candidate/tagged/public inventories, per-run warnings/reviews, exact CI timeout/retry and public/latest bindings |
| `evidence/v0.31.0/` | Original rejected candidates, published tag/source/28 assets, signed manifests, exact-run CI/warning decisions, public downloads, independent reviews and scoped native deviations |
| `evidence/v0.30.2/` | Preparation, exact candidate/tagged bindings, Windows native and raw TC-193 results, scoped deviations, separate warning decisions and public readback |
| `evidence/v0.30.1/` | Whole48-commit classification, predecessor/native reuse, latest Action diagnosis, per-run artifact/warning decisions and publication readback |
| `evidence/v0.30.0/` | Preparation and whole-release classification, native repair/reuse, published body, actual public downloads and closure validation |
| `evidence/v0.29.0/` | Published release body, exact validation readback, and review/deviation notes |
| `evidence/v0.28.0/` | Published release body, local validation, and review evidence |
| `evidence/v0.27.1/` | Published release body and sanitized GUI validation evidence |
| `evidence/v0.20.1/` | Historical release-body draft evidence retained for reference |
| `evidence/v0.18.9/` | GUI reports, headful smoke evidence, logs, and release-body draft |
| `evidence/v0.18.10/` | GUI report and release-body draft |
| `evidence/v0.18.11/` | Release-body draft evidence only; no release record is stored here |
| `evidence/v0.24.2/` | Historical release-candidate body draft for the published v0.24.2 release |

Use [RELEASE.md](../RELEASE.md) for the current release process, [OSS_COMPLIANCE.md](../OSS_COMPLIANCE.md) for license and notice checks, and the repository root [CHANGELOG.md](../../CHANGELOG.md) for the consolidated user-facing change history.
