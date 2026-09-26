# v0.29.0 Release Range Classification

## Source
- Previous public tag: `v0.28.0`.
- Reviewed source: tagged commit `ec4d9b8b12e0b92a5a441f45016f55621faac93e`.
- Range: `v0.28.0..v0.29.0` (26 commits).
- Basis: commit subjects and per-commit diff stats; user-facing entries map to `CHANGELOG.md` v0.29.0.

## Commit-by-Commit Disposition

| Commit | Classification | Release note mapping / exclusion reason |
| --- | --- | --- |
| `1cda0c8` docs: record published v0.28.0 release | Prior-release record inside the post-tag range | Excluded from v0.29.0 product notes; this is one of the 18 commits after the v0.28.0 tag, but it only records the already-published v0.28.0 release. |
| `41558e4` fix(gui): keep clear selected layout stable on hover | User-visible fix | `Fixed`: selected-row hover no longer moves the clear-selection control. |
| `73d67af` perf: avoid intermediate path key allocations | Performance change | `Changed`: reduce intermediate path-key allocations. |
| `ffbff34` docs: separate CI procedures from rollout history | Documentation-only | Excluded from user-facing release notes; reorganizes CI operations and historical records. |
| `ba71caf` Extract shared settings and history persistence | Internal refactor | Excluded as an independent behavior claim; module extraction supports the later bounded persistence and recovery changes. |
| `32ec310` Unify tab query state and result publication policy | User-observable consistency change | `Changed`: improve query/result consistency across tab switches and background publication. |
| `d6f9eed` Protect persistence recovery after resilience review | Reliability fix | `Changed`: preserve settings/session state and expose recoverable persistence failures. |
| `f63e9fc` Bound active candidate filtering and preserve deferred query state | User-visible responsiveness change | `Changed`: bound active result filtering and keep deferred query state. |
| `abfe5f7` Protect and bound session persistence with visible failures | Reliability change | `Changed`: bounded session/settings persistence and visible failures; grouped with `d6f9eed`. |
| `2828c0e` Document residual architecture contracts and validation | Documentation-only | Excluded from user-facing release notes; records architecture and validation contracts. |
| `9ad85b9` Fix application UI tooltip language | User-visible fix | `Fixed`: remove mixed-language tooltip/dialog copy and make it consistently English. |
| `5758b54` Format top panel with pinned toolchain | Formatting-only | Excluded; no behavior change. |
| `3c23198` fix: align paged preview font metrics | User-visible fix | `Fixed`: align paged preview font metrics. |
| `afdd019` Add CSV and TSV preview highlighting | User-facing feature | `Added`: CSV/TSV syntax highlighting. |
| `a70de24` Color CSV and TSV previews by column | User-facing feature refinement | `Added`: stable per-column colors across records and fields. |
| `5e4e4ce` test: cover CSV and TSV column colors | Test-only | Excluded from user-facing release notes; guards the CSV/TSV column-color contract. |
| `075443e` test: guard paged preview font metrics | Test-only | Excluded from user-facing release notes; guards paged preview font metrics. |
| `6f4f19a` fix: preserve restored sort result counts | User-visible fix | `Fixed`: preserve accurate result counts after restoring sort state. |
| `9190122` chore(release): prepare v0.29.0 | Release metadata | Excluded as a separate product claim; applies version, changelog, and release procedure for changes already classified above. |
| `e674250` fix: register v0.28.0 updater capability | Release compatibility metadata | Excluded as a separate product claim; repairs the N-1 compatibility gate and supports the disclosed v0.24.3 bridge. |
| `8d8e3a2` test: stabilize paged preview retirement accounting | Test-only | Excluded; stabilizes release-gate evidence without changing shipped behavior. |
| `13c9eed` test: await TC-207 latest-root settlement | Test-only, superseded in-range | Excluded; intermediate CI stabilization later refined by `29907c1`. |
| `29907c1` test: settle startup indexing before TC-207 root retirement | Test-only | Excluded; stabilizes asynchronous test settlement without changing shipped behavior. |
| `f7f3567` test: stabilize async release gates | Test-only | Excluded; hardens release validation without changing shipped behavior. |
| `decaecc` test: stabilize adaptive frontier saturation fixture | Test-only | Excluded; stabilizes a bounded-indexing fixture without changing shipped behavior. |
| `ec4d9b8` docs: tighten release candidate handoff | Documentation-only | Excluded from product notes; updates release operation and candidate evidence handoff. |

## Cross-Checks
- Cargo package version, root lockfile package version, changelog heading, compare links, tag, and release body must all identify `0.29.0`.
- The exact `GSM-012` fixture and both-theme native GUI checks are specified by `docs/GUI-TESTPLAN.md`; no scripted query probe is treated as visual evidence.
- `git rev-list --count v0.28.0..v0.29.0` returned 26 after immutable tag creation; the table contains 26 dispositions.
- No dependency, asset, updater format, or third-party notice change is included in this release preparation.
