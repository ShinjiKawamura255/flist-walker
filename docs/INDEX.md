# Documentation Index

This file is the repository-level documentation map for FlistWalker. Use it to choose the smallest document set needed for a change.

## First Reading Paths

| Audience or task | Start here | Then read |
| --- | --- | --- |
| Human user or maintainer | [README.md](../README.md) | [CURRENT_STATUS.md](CURRENT_STATUS.md) for project posture, then the relevant SDD or release docs |
| AI agent | [AGENTS.md](../AGENTS.md) | [CURRENT_STATUS.md](CURRENT_STATUS.md), then this index and the relevant change-type docs |
| New code change | [CURRENT_STATUS.md](CURRENT_STATUS.md) | [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md), [TESTPLAN.md](TESTPLAN.md), and the related SDD topic |
| Docs-only change | [TESTPLAN.md](TESTPLAN.md) | Apply VM-001 in [testplan/validation-matrix.md](testplan/validation-matrix.md) |
| Release or update work | [RELEASE.md](RELEASE.md) | [OSS_COMPLIANCE.md](OSS_COMPLIANCE.md), [testplan/validation-matrix.md](testplan/validation-matrix.md), and the project-local release skills |

## Current Truth

| Role | Document |
| --- | --- |
| Current implementation direction and maintenance priorities | [CURRENT_STATUS.md](CURRENT_STATUS.md) |
| Human product overview, setup, and usage | [README.md](../README.md) / [README-ja.md](../README-ja.md) |
| Agent guardrails, validation routing, and project-local skill policy | [AGENTS.md](../AGENTS.md) |
| History discovery | [TASKS.md](TASKS.md) |
| Completed historical detail | [history/](history/) |

## SDD And Validation

| Role | Entry point | Detail docs |
| --- | --- | --- |
| Requirements | [REQUIREMENTS.md](REQUIREMENTS.md) | [requirements/](requirements/) |
| Specification | [SPEC.md](SPEC.md) | [spec/](spec/) |
| Design | [DESIGN.md](DESIGN.md) | [design/](design/) |
| Test plan and validation matrix | [TESTPLAN.md](TESTPLAN.md) | [testplan/](testplan/) |
| GUI validation | [GUI-TESTPLAN.md](GUI-TESTPLAN.md) | [GUI-TESTREPORT.template.md](GUI-TESTREPORT.template.md) |

## Architecture And Design Maps

| Need | Document |
| --- | --- |
| Short runtime and ownership overview | [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) |
| Detailed module ownership and regression guards | [ARCHITECTURE.md](ARCHITECTURE.md) |
| Repository-level detailed design map | [DETAILED_DESIGN.md](DETAILED_DESIGN.md) |
| Detailed design sections | [detailed-design/](detailed-design/) |

## Operations And Release

| Need | Document |
| --- | --- |
| Release build, draft release, and publish operation | [RELEASE.md](RELEASE.md) |
| Release incident handling | [RELEASE_INCIDENT_RUNBOOK.md](RELEASE_INCIDENT_RUNBOOK.md) |
| OSS license and notice compliance | [OSS_COMPLIANCE.md](OSS_COMPLIANCE.md) |
| Support and issue triage guidance | [SUPPORT.md](SUPPORT.md) |
| Release notes and evidence | [releases/](releases/) |

## Document Ownership Rules

- `CURRENT_STATUS.md` owns current project posture. Do not move completed roadmap detail into it.
- `TASKS.md` is a history index, not the active work queue.
- Root SDD files are entry points. Topic-level content lives under `requirements/`, `spec/`, `design/`, and `testplan/`.
- Validation commands and change-type gates live in `TESTPLAN.md` and `testplan/validation-matrix.md`.
- Release workflow details live in `RELEASE.md` and project-local release skills, not README.
