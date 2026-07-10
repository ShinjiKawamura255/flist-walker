# Documentation Index

This is the canonical map of FlistWalker documentation. Start from the question or change type below and read only the linked documents.

## Start Here

| Reader or purpose | First document | Next document |
| --- | --- | --- |
| Use or install FlistWalker | [README.md](../README.md) or [README-ja.md](../README-ja.md) | [SUPPORT.md](SUPPORT.md) when reporting a problem |
| Maintain the current project | [CURRENT_STATUS.md](CURRENT_STATUS.md) | The relevant change-type row below |
| Work as an AI agent | [AGENTS.md](../AGENTS.md) | This index, then the relevant change-type row |
| Learn the repository layout | [STRUCTURE.md](STRUCTURE.md) | [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) for the runtime path |
| Find completed work | [history/INDEX.md](history/INDEX.md) | The specific historical record |
| Find release records or evidence | [releases/INDEX.md](releases/INDEX.md) | [RELEASE.md](RELEASE.md) for the release procedure |

## Find The Answer

| Question | Source of truth |
| --- | --- |
| What is implemented and what currently matters? | [CURRENT_STATUS.md](CURRENT_STATUS.md) |
| What is in or out of product scope? | [REQUIREMENTS.md](REQUIREMENTS.md) |
| What behavior is required? | [SPEC.md](SPEC.md) |
| Which DES item realizes a requirement? | [DESIGN.md](DESIGN.md) |
| Where does the implementation live? | [STRUCTURE.md](STRUCTURE.md), then [ARCHITECTURE.md](ARCHITECTURE.md) |
| How does a runtime flow or module work internally? | [DETAILED_DESIGN.md](DETAILED_DESIGN.md) |
| Which checks are required for my change? | [TESTPLAN.md](TESTPLAN.md), then the [Validation Matrix](testplan/validation-matrix.md) |
| How is a release built and published? | [RELEASE.md](RELEASE.md) |
| What evidence exists for a previous release? | [releases/INDEX.md](releases/INDEX.md) |
| Where is active or completed task context? | [TASKS.md](TASKS.md) |

## Choose By Change Type

| Change | Read before editing | Validate with |
| --- | --- | --- |
| Documentation only | The owning document from this index | [VM-001](testplan/validation-matrix.md#docs-only-or-sddtdd-document-updates) |
| Search syntax, matching, ranking, or actions | [Search, Actions, CLI Specification](spec/search-actions-cli.md), [Architecture Overview](ARCHITECTURE_OVERVIEW.md) | [VM-004](testplan/validation-matrix.md#search-or-query-contract-changes) |
| FileList, walker, indexing, or performance | [Indexing and Performance Specification](spec/indexing-performance.md), [ARCHITECTURE.md](ARCHITECTURE.md) | [VM-003](testplan/validation-matrix.md#indexing-filelist-walker-or-kind-resolution-changes) |
| GUI behavior, input, tabs, sessions, or responsiveness | [GUI Behavior Specification](spec/gui-behavior.md), [ARCHITECTURE.md](ARCHITECTURE.md), [GUI-TESTPLAN.md](GUI-TESTPLAN.md) | [VM-002](testplan/validation-matrix.md#gui-orchestration-rendering-input-tabs-or-session-changes) |
| Runtime configuration or startup bootstrap | [Operations and Runtime Specification](spec/operations-release-config.md), [Detailed Module Design](detailed-design/module-design.md) | [VM-008](testplan/validation-matrix.md#runtime-config-settings-or-startup-bootstrap-changes) |
| Build, updater, packaging, or release | [RELEASE.md](RELEASE.md), [OSS_COMPLIANCE.md](OSS_COMPLIANCE.md), project-local release skills | [VM-005](testplan/validation-matrix.md#cli-build-release-updater-or-oss-packaging-changes) |
| GUI validation docs or smoke tooling | [GUI-TESTPLAN.md](GUI-TESTPLAN.md), [GUI-TESTREPORT.template.md](GUI-TESTREPORT.template.md) | [VM-006](testplan/validation-matrix.md#ci-coverage-gui-validation-docs-or-smoke-script-changes) |
| Support or issue templates | [SUPPORT.md](SUPPORT.md) | [VM-007](testplan/validation-matrix.md#supportability-docs-templates-or-diagnostics-wording) |

## Architecture And Design Layers

These documents answer different questions and are not interchangeable.

| Document | Primary role | Use it when |
| --- | --- | --- |
| [STRUCTURE.md](STRUCTURE.md) | Repository and directory navigation | You need to locate code, docs, tooling, or configuration |
| [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) | Short runtime and ownership orientation | You are new to the codebase or selecting initial files |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Current module ownership, boundaries, threading, and regression guards | You are changing implementation structure or coordination |
| [DESIGN.md](DESIGN.md) | Normative SDD design and DES trace entrypoint | You are changing a requirement-backed design contract |
| [DETAILED_DESIGN.md](DETAILED_DESIGN.md) | Deep module, data, sequence, resilience, and operations reference | You need implementation mechanics beyond the architecture map |

## SDD And Validation

Root SDD files are concise entrypoints. Topic content remains grouped by SDD responsibility so FR/NFR/CON, SP, DES, and TC ownership stays explicit.

| Responsibility | Entry point | Topic directory |
| --- | --- | --- |
| Requirements and acceptance criteria | [REQUIREMENTS.md](REQUIREMENTS.md) | [requirements/](requirements/) |
| Normative behavior | [SPEC.md](SPEC.md) | [spec/](spec/) |
| Implementation design and trace | [DESIGN.md](DESIGN.md) | [design/](design/) |
| Test intent, cases, and validation routing | [TESTPLAN.md](TESTPLAN.md) | [testplan/](testplan/) |

## Operations, Reference, And Records

| Need | Document or collection |
| --- | --- |
| Release procedure and asset rules | [RELEASE.md](RELEASE.md) |
| Release failure response | [RELEASE_INCIDENT_RUNBOOK.md](RELEASE_INCIDENT_RUNBOOK.md) |
| OSS licenses, notices, and audit posture | [OSS_COMPLIANCE.md](OSS_COMPLIANCE.md) |
| Support and issue-report guidance | [SUPPORT.md](SUPPORT.md) |
| Active-task boundary and task routing | [TASKS.md](TASKS.md) |
| Completed maintenance history | [history/INDEX.md](history/INDEX.md) |
| Release records, rejected candidates, and evidence | [releases/INDEX.md](releases/INDEX.md) |

## Ownership Rules

- `CURRENT_STATUS.md` owns the short current project posture; it links to validation and history instead of duplicating them.
- `TASKS.md` explains task-state boundaries. It does not duplicate current posture or completed records.
- `history/INDEX.md` owns navigation to completed maintenance history.
- `releases/INDEX.md` owns navigation to release records and evidence; `RELEASE.md` owns the procedure.
- Root SDD files own their respective indexes and trace excerpts; topic directories own detailed content.
- `TESTPLAN.md` and `testplan/validation-matrix.md` own validation selection and canonical commands.
