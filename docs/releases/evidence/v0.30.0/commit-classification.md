# v0.30.0 Release Range Classification

Previous public tag: `v0.29.0`. Preparation input: `v0.29.0..2138076` (10 commits), plus this task's eventual preparation commit. Each listed subject was checked against its per-commit diff/stat; final tag range will be reconciled before publishing.

| Commit | Classification | Public note mapping / exclusion reason |
|---|---|---|
| 8fd62b8 | Prior-release closure docs | Excluded as a new product claim; records already-published v0.29.0. |
| 6441853 | Feature | Added: preview keyboard control mode and Color/page/scroll route. |
| 649d6fc | Feature | Added: snapshot age and root FileList change detection. |
| bede48e | Test and contract coverage | Excluded as a separate feature; strengthens freshness failure/stale contract coverage. |
| 64df765 | Runtime failure fix | Fixed: distinguish worker panic from healthy shutdown. |
| 56ac4f4 | Internal owner separation | Changed: document/settings persistence owners; no new durable data format or stronger crash guarantee. |
| 6dff5ee | User-visible correctness fix | Fixed: restored-query cancel and empty-query incremental results. |
| 33f43e6 | Dependency patch | Changed: serde_json 1.0.149→1.0.151. No reported vulnerability correction. |
| 4879892 | Dependency patch | Changed: semver 1.0.27→1.0.28. No reported vulnerability correction. |
| 2138076 | Responsiveness/ownership fix | Changed: sliced preparation/subset reuse; Fixed: ignore reevaluation and off-thread scratch retirement. |
| d860896 preparation | Release gate/metadata/docs | Fixed: shipped v0.29.0 updater capability registration; version, changelog and this evidence. |

The subsequent CI repair is test/harness and TC-183 traceability only. It is excluded as a separate product feature or behavioral change; the final SHA and full immutable tag range will be reconciled before publication.

0.30.0 is a minor release because it includes new user-facing features. Breaking/Deprecated/Security product changes: none established. All versions/compare links/body/tag are checked again against the immutable final tag. Public downloads are listed only after real bundle inventory verification. Large-dataset latency and macOS notarization posture remain disclosed.
