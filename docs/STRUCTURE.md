# Repository Structure Map

## Purpose

Use this map to locate the relevant code, documentation, tooling, or configuration before making a change. For runtime responsibilities after locating an area, continue with [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) or [ARCHITECTURE.md](ARCHITECTURE.md).

## Scope And Coverage

- Repository root: `flistwalker/`
- Depth: top-level directories and the key `rust/src/` and `docs/` subdirectories
- Reviewed sources: repository tree, README files, Cargo manifests, CI workflows, architecture documents, validation matrix, scripts, and project-local skills
- Excluded as generated or transient: `.git/`, `.worktrees/`, `.pytest_cache/`, `dist/`, and `rust/target/`

## High-Level Layout

- `rust/` is the canonical product implementation.
- `docs/` separates current truth, SDD, validation, operations, and retained records.
- `scripts/`, `.github/`, and `skills/` provide repeatable build, validation, release, and maintenance workflows.
- `prototype/` retains the previous Python prototype and is not the primary implementation path.

## Directory Map

| Path | Role | Start here |
| --- | --- | --- |
| `rust/` | Canonical Rust GUI/CLI application | `rust/Cargo.toml`, `rust/src/main.rs`, `rust/src/lib.rs` |
| `rust/src/app/` | GUI coordination, rendering, state, worker routing, tabs, sessions, update, and FileList UI flows | `rust/src/app/mod.rs`, then the owner module named in `docs/ARCHITECTURE.md` |
| `rust/src/indexer/` | FileList detection/reading/writing and walker traversal | `rust/src/indexer/mod.rs` |
| `rust/src/search/` | Match evaluation, ranking, caching, configuration, and execution | `rust/src/search/mod.rs` |
| `rust/src/ui_model/` | UI-facing result and highlight models | `rust/src/ui_model/mod.rs` |
| `rust/src/runtime_config/` | Runtime configuration parsing and support | `rust/src/runtime_config.rs` and this directory |
| `rust/src/updater/` | Update discovery, validation, download, and apply support | `rust/src/updater.rs` and this directory |
| `prototype/python/` | Retained Python prototype | `prototype/python/pyproject.toml` |
| `docs/` | Canonical documentation entrypoints and operational references | `docs/INDEX.md` |
| `docs/requirements/` | Detailed FR/NFR/CON and acceptance-criteria content | `docs/REQUIREMENTS.md` |
| `docs/spec/` | Normative SP behavior grouped by topic | `docs/SPEC.md` |
| `docs/design/` | DES-level implementation design and trace | `docs/DESIGN.md` |
| `docs/detailed-design/` | Deep implementation mechanics, data, sequences, and resilience | `docs/DETAILED_DESIGN.md` |
| `docs/testplan/` | Test strategy, TC catalog, validation matrix, and manual regression procedures | `docs/TESTPLAN.md` |
| `docs/history/` | Completed maintenance and closure history | `docs/history/INDEX.md` |
| `docs/releases/` | Release records, rejected candidates, and release evidence | `docs/releases/INDEX.md` |
| `scripts/` | Build, packaging, smoke-test, signing, and release validation helpers | Select through `docs/RELEASE.md` or the Validation Matrix |
| `.github/workflows/` | Cross-platform CI, performance, and tagged release automation | `.github/workflows/ci-cross-platform.yml` |
| `.github/ISSUE_TEMPLATE/` | User-facing issue forms | `docs/SUPPORT.md` |
| `skills/` | Project-local high-risk release workflows | `skills/flistwalker-release-preflight/SKILL.md` |

## Key Entrypoints

- Product execution: `rust/src/main.rs`
- Shared Rust library surface: `rust/src/lib.rs`
- GUI coordinator: `rust/src/app/mod.rs`
- Documentation routing: [INDEX.md](INDEX.md)
- Current project posture: [CURRENT_STATUS.md](CURRENT_STATUS.md)
- Runtime ownership map: [ARCHITECTURE.md](ARCHITECTURE.md)
- Change-specific validation: [testplan/validation-matrix.md](testplan/validation-matrix.md)

## Build, Test, And Release Hooks

- Build metadata: `rust/Cargo.toml`, `rust/Cargo.lock`, `rust/rust-toolchain.toml`
- General Rust validation: select commands from [TESTPLAN.md](TESTPLAN.md)
- Windows GNU builds: `scripts/build-rust-win.*` and `scripts/build-rust-win-clean.*`
- Release packaging: `scripts/prepare-release*` and `scripts/validate-release-bundle.sh`
- CI and release automation: `.github/workflows/`
- Release readiness and notes: project-local skills under `skills/`

## Data And Configuration

- `flistwalker.ignore.txt.example`: public ignore-list example
- Runtime configuration implementation: `rust/src/runtime_config.rs` and `rust/src/runtime_config/`
- Release-side notices: `LICENSE` and `THIRD_PARTY_NOTICES.txt`
- Distribution output: `dist/` is generated and is not a source-of-truth directory

