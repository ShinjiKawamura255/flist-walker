# v0.30.0 Complete Release Range Classification

Range: `v0.29.0..v0.30.0`, source `129ec84cbf24ffb02bba1cade5b75fa042fd0c45`,18 commits. Every commit subject and diff stat was inspected. Public behavior changes map to the retained release body; prior-release records, test-only and internal process changes have explicit exclusions.

| Commit | Subject | Release-note mapping |
| --- | --- | --- |
| [8fd62b89](https://github.com/ShinjiKawamura255/flist-walker/commit/8fd62b89345e0feb75734e7630bbd73d1deb6574) | docs: record v0.29.0 release closure | Excluded: records previous release closure |
| [64418531](https://github.com/ShinjiKawamura255/flist-walker/commit/64418531b1ba8cf12e892bb3250c87990d42d21e) | feat(gui): add preview keyboard control mode | Added: Preview keyboard mode |
| [649d6fc8](https://github.com/ShinjiKawamura255/flist-walker/commit/649d6fc8dcd88007346e08f406dc6f610293bbbd) | feat(gui): show snapshot age and detect root FileList changes | Added: snapshot age/root FileList checking |
| [bede48e6](https://github.com/ShinjiKawamura255/flist-walker/commit/bede48e65e02c07cf0f59c95db89b0f71c4efda2) | test(gui): cover snapshot freshness contract gaps | Excluded: freshness regression tests supporting Added |
| [64df765c](https://github.com/ShinjiKawamura255/flist-walker/commit/64df765cc90eabf14cfd33a0bb842b24da2ba2a3) | fix: distinguish worker panic from healthy shutdown | Fixed: worker panic distinction |
| [56ac4f4f](https://github.com/ShinjiKawamura255/flist-walker/commit/56ac4f4f5313339fbb7676d56387c5b508aaa5e4) | refactor: separate persistence document and settings owners | Changed: persistence owner separation |
| [6dff5eee](https://github.com/ShinjiKawamura255/flist-walker/commit/6dff5eeec331e5063ab31bb181d5da68c3fdcd95) | Fix restored query cancellation and incremental empty-query results | Fixed: restored query cancellation/empty progressive results |
| [33f43e6b](https://github.com/ShinjiKawamura255/flist-walker/commit/33f43e6b7fac68d2f26a98965bb11b9fef3744c3) | chore(deps): bump serde_json from 1.0.149 to 1.0.151 in /rust | Changed: serde_json patch |
| [48798929](https://github.com/ShinjiKawamura255/flist-walker/commit/48798929a73cc49411c5d7c50265efd743c33e3c) | chore(deps): bump semver from 1.0.27 to 1.0.28 in /rust | Changed: semver patch |
| [82420340](https://github.com/ShinjiKawamura255/flist-walker/commit/824203405e9d49ddc3b1ebdcf174e44f0bf8b1d7) | Fix GUI candidate preparation and scratch retirement liveness | Changed/Fixed: sliced preparation, ignore reevaluation, scratch retirement |
| [573392f7](https://github.com/ShinjiKawamura255/flist-walker/commit/573392f7d05b1bffb6c540f91ce90f671b6258e6) | Prepare v0.30.0 after project health review | Summary: v0.30 metadata/health; Fixed: N-1 predecessor registration |
| [4131f9f2](https://github.com/ShinjiKawamura255/flist-walker/commit/4131f9f223ac72fd3f3ea79563428b8b7d2ece0e) | Make filter and endurance regression tests deterministic | Excluded: deterministic test/harness correction; production inputs unchanged |
| [1a2305ce](https://github.com/ShinjiKawamura255/flist-walker/commit/1a2305ce34fa0c075a490902ce2bba79b3a84e58) | fix(ci): select pinned Rust for release audit installation | Excluded: internal audit tool selector warning correction; not product security fix |
| [c3f07d2b](https://github.com/ShinjiKawamura255/flist-walker/commit/c3f07d2b760df19cd1888fbc45f5f4eb3fd2304b) | docs(ci): document explicit selector and adjacent install pin guards | Excluded: CI pin operation documentation |
| [77f90cf4](https://github.com/ShinjiKawamura255/flist-walker/commit/77f90cf46aa6aad799e96c2d4c6a85affd386c38) | docs(ci): record release audit Guardian rollout and restoration | Excluded: CI controlled rollout/restoration records |
| [997cfb01](https://github.com/ShinjiKawamura255/flist-walker/commit/997cfb014f9a2389510105affa983fe9f414c3e7) | docs(release): select validation by change impact and reuse evidence | Excluded: release validation process, no new product behavior |
| [67617162](https://github.com/ShinjiKawamura255/flist-walker/commit/6761716292f432e77fab36c0d1cbcd6b8a1b1ee6) | fix(gui): keep narrow results settings and preview controls reachable | Fixed: supported narrow Results/Settings and paged Preview reachability |
| [129ec84c](https://github.com/ShinjiKawamura255/flist-walker/commit/129ec84cbf24ffb02bba1cade5b75fa042fd0c45) | docs(release): record confirmed GUI repairs and validation selection | Summary/Fixed: final changelog/date and GUI proof; no new runtime change |

The complete18-commit range includes the final narrow Results/Settings/Preview repair and release-record update. It does not summarize only the latest fixes. See [RELEASE-BODY.md](RELEASE-BODY.md) and the [execution packet](../../v0.30.0.md).
