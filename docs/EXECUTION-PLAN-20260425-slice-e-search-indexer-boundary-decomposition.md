# EXECUTION PLAN: Slice E Search / Indexer Boundary Decomposition

## Metadata
- Date: 2026-04-25
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Profile: safety-critical
- Planning Depth: roadmap+slice
- Review Pattern: specialist-subagents
- Review Requiredness: required-before-and-after-revision
- Execution Mode: none
- Execution Mode Policy: Inherits the parent roadmap policy. This slice is behavior-preserving domain refactoring and must complete plan review, required revisions, convergence review, and Review Notes updates before implementation.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-e-search-indexer-boundary
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-25 initial plan created after Slice D commit `b9a412f`.
  - 2026-04-25 initial specialist review completed with architecture/performance and testing/validation perspectives.
  - Architecture/performance review found one blocking issue: `perf_search_100k_candidates_reports_latency` was listed as a smoke run but lacked a concrete before/after no-regression criterion.
  - Testing/validation review found three blocking issues: call-graph checks did not cover all planned moved search types/helpers, indexer checks omitted the FileList mtime precedence helper, and coverage/audit gates from the parent roadmap were omitted without rationale.
  - 2026-04-25 revised plan to name target modules, add before/after search perf criterion, expand negative/positive boundary checks, add targeted ranking/FileList hierarchy regression intent, and include coverage/audit gates.
  - 2026-04-25 convergence review completed by architecture/performance and testing/validation reviewers.
  - Convergence result: all initial blockers were resolved in the plan; no material blockers remain.
  - Status changed to `レビュー済み`; implementation may start.

## 1. Background
`rust/src/search/mod.rs` remains over 1,200 lines even though cache, execution mode, collection, and ranking already have separate modules. It still owns query compilation, literal/regex matching, searchable-entry materialization, candidate scoring, public search APIs, and tests in one file.

`rust/src/indexer/mod.rs` remains over 1,000 lines. FileList reading, writing, and walker traversal are already split, but nested FileList hierarchy override orchestration still lives in `mod.rs` and is called by `filelist_reader.rs` through a private parent function.

The roadmap calls for search/indexer boundary decomposition after render cleanup. This slice should reduce review surface without changing search ranking, FileList override semantics, or public module APIs.

## 2. Goal
Make search and indexer ownership easier to review without changing behavior:

- Keep `search/mod.rs` as the public search facade and high-level orchestration entrypoint.
- Move private query compilation, literal/regex matching, searchable entry building, and candidate scoring/evaluation into a dedicated private search module.
- Keep `search/cache.rs`, `search/config.rs`, `search/execute.rs`, and `search/rank.rs` responsibilities intact.
- Keep `indexer/mod.rs` as the public indexer facade and public type/API owner.
- Move nested FileList hierarchy override orchestration from `indexer/mod.rs` into a dedicated private indexer module.
- Preserve all public functions, result ordering, score semantics, FileList override precedence, and cancellation behavior.

## 3. Scope
### In Scope
- `rust/src/search/mod.rs`
- new private `rust/src/search/*` helper module if needed
- `rust/src/search/execute.rs` import boundary adjustments
- `rust/src/indexer/mod.rs`
- new private `rust/src/indexer/*` helper module if needed
- `rust/src/indexer/filelist_reader.rs` import boundary adjustments
- Search/indexer docs and roadmap/TASKS progress updates

### Out of Scope
- Changing fzf-like query syntax or ranking behavior.
- Changing parallel search thresholds, rayon settings, or prefix-cache policy.
- Changing FileList detection priority, nested FileList mtime precedence, or walker classification behavior.
- Changing public CLI/GUI search/index behavior.
- Moving tests out of existing modules unless compilation requires a minimal import update.
- Performance tuning beyond preserving existing behavior and perf gates.

## 4. Constraints and Assumptions
- This is behavior-preserving refactoring only.
- Search hot path changes must not add allocations or locking beyond existing behavior.
- Indexing changes must preserve VM-003 performance guard behavior and cancellation handling.
- Public API compatibility matters: `main.rs`, app workers, and tests should keep using existing `crate::search::*` and `crate::indexer::*` surfaces.
- Rust changes require `cargo test --locked` and `cargo clippy --all-targets -- -D warnings`.
- Because indexer hot path files are touched, VM-003 ignored perf tests must be run explicitly.
- Because search evaluation code is touched, `perf_search_100k_candidates_reports_latency` must be run explicitly.

## 4.1 Planned Boundary
Search:

- `search/mod.rs` keeps public structs/APIs, `rank_search_results`, collection orchestration, and test module.
- New private `search/match_eval.rs` module owns:
  - `LiteralPattern`
  - `AlternativeSet`
  - `IncludeAlternative`
  - `IncludeMatcher`
  - `CompiledQuery`
  - `SearchContext`
  - `SearchableEntry`
  - query compilation helpers
  - literal/include matcher helpers
  - searchable path materialization
  - `evaluate_candidate`
  - score helper used by candidate evaluation
- `search/execute.rs` imports candidate evaluation and private types from `search/match_eval.rs` rather than from `search/mod.rs`.

Indexer:

- `indexer/mod.rs` keeps public exports, `IndexSource`, `IndexBuildResult`, `build_index_with_metadata`, and `build_index`.
- New private `indexer/filelist_hierarchy.rs` module owns:
  - `apply_nested_filelist_overrides`
  - nested FileList queue/discovery helpers
  - subtree replacement helpers
  - FileList mtime precedence helper
- `indexer/filelist_reader.rs` imports the nested override helper from the new module.

## 5. Current Risks
- Risk: Search private type visibility changes break `execute.rs` or tests.
  - Impact: compile failure or forced public API expansion.
  - Mitigation: use `pub(super)` only for module-internal seams needed by sibling modules; keep public exports unchanged.
- Risk: Moving score/evaluation helpers changes ranking behavior.
  - Impact: visible search ordering regressions.
  - Mitigation: do not rewrite logic; move code with minimal import changes; run full search tests and 100k perf smoke.
- Risk: Moving nested FileList override logic changes ordering or mtime precedence.
  - Impact: incorrect FileList override source, missed nested override, or stale FileList precedence.
  - Mitigation: move logic intact; run indexer nested FileList tests and VM-003 perf tests.
- Risk: Large movement obscures real logic changes.
  - Impact: review difficulty.
  - Mitigation: keep movement and import adjustments only; do not mix tuning or behavior changes into this slice.

## 6. Execution Strategy
1. Confirm current call graph and public boundary
   - Files/modules/components: `rust/src/search/mod.rs`, `rust/src/search/execute.rs`, `rust/src/indexer/mod.rs`, `rust/src/indexer/filelist_reader.rs`.
   - Expected result: identify exact private functions/types to move and public APIs to keep stable.
   - Verification: `rg` call-site checks and compile.
2. Extract search matching/evaluation boundary
   - Files/modules/components: `rust/src/search/mod.rs`, new private search module, `rust/src/search/execute.rs`.
   - Expected result: `search/mod.rs` no longer owns private query matching/scoring internals; public search APIs remain unchanged.
   - Verification: search unit tests, `cargo test --locked search::tests`, and 100k perf before/after comparison.
3. Extract indexer nested FileList hierarchy boundary
   - Files/modules/components: `rust/src/indexer/mod.rs`, new private indexer module, `rust/src/indexer/filelist_reader.rs`.
   - Expected result: nested FileList override orchestration has a single owner outside `mod.rs`; public indexer facade remains unchanged.
   - Verification: indexer unit tests and VM-003 perf tests.
4. Synchronize docs and progress records
   - Files/modules/components: `docs/ARCHITECTURE.md`, `docs/DESIGN.md`, `docs/DETAILED_DESIGN.md`, `docs/TESTPLAN.md`, `docs/TASKS.md`, roadmap, this slice.
   - Expected result: docs describe `search/mod.rs` and `indexer/mod.rs` as facades and the new private modules as domain owners.
   - Verification: docs diff review.
5. Run validation and commit
   - Files/modules/components: all touched files.
   - Expected result: Slice E is one independent rollback unit.
   - Verification: full test/clippy/perf matrix from section 8.

## 7. Detailed Task Breakdown
- [x] Review this slice plan with architecture/performance/testing focus.
- [x] Record pre-change `perf_search_100k_candidates_reports_latency` output as Slice E baseline.
- [x] Confirm search/indexer call graph and public boundary with `rg`.
- [x] Extract search matching/evaluation helpers without changing scoring semantics.
- [x] Extract nested FileList hierarchy override helpers without changing precedence/cancellation semantics.
- [x] Keep public search/indexer API stable.
- [x] Update permanent docs for search/indexer boundary ownership.
- [x] Run required validation including ignored perf gates.
- [x] Update roadmap/TASKS and mark Slice E complete.
- [x] Commit Slice E as an independent rollback unit.

## 8. Validation Plan
- Automated tests:
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo test --locked search::tests`
  - `cd rust && cargo test --locked indexer::tests`
  - `cd rust && cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 70`
  - `cd rust && cargo audit`
  - `cd rust && cargo test --locked perf_search_100k_candidates_reports_latency --lib -- --ignored --nocapture`
  - `cd rust && cargo test --locked perf_filelist_stream_is_faster_than_metadata_probe_baseline --lib -- --ignored --nocapture`
  - `cd rust && cargo test --locked perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
  - `git diff --check`
- Search perf no-regression:
  - Run `perf_search_100k_candidates_reports_latency` before changing search code and record the printed latency in this plan's Progress Log.
  - Run the same test after implementation.
  - Post-change latency must remain under the project target of 100ms and must not exceed the pre-change baseline by more than 20% unless the run is clearly noisy and a rerun returns within tolerance.
  - If this criterion fails, stop and either revert the search extraction or update this plan with an explicit performance investigation before committing.
- Boundary checks:
  - Negative search check: `rg -n "struct LiteralPattern|struct AlternativeSet|struct IncludeAlternative|enum IncludeMatcher|struct CompiledQuery|struct SearchContext|struct SearchableEntry|fn compile_|fn matches_|fn searchable_full|fn build_searchable_entry|fn score_entry|fn evaluate_candidate" rust/src/search/mod.rs` should return no private matching/evaluation definitions after extraction.
  - Positive search check: `rg -n "struct LiteralPattern|struct AlternativeSet|struct IncludeAlternative|enum IncludeMatcher|struct CompiledQuery|struct SearchContext|struct SearchableEntry|fn evaluate_candidate" rust/src/search/match_eval.rs` should show the moved private boundary.
  - Negative indexer check: `rg -n "fn apply_nested_filelist_overrides|fn enqueue_nested_filelists_from_entries|fn path_depth_from_root|fn nearest_active_modified|fn replace_entries_in_subtree|fn is_path_in_subtree|fn is_filelist_newer" rust/src/indexer/mod.rs` should return no private nested override definitions after extraction.
  - Positive indexer check: `rg -n "fn apply_nested_filelist_overrides|fn enqueue_nested_filelists_from_entries|fn nearest_active_modified|fn replace_entries_in_subtree|fn is_filelist_newer" rust/src/indexer/filelist_hierarchy.rs` should show the moved private boundary.
  - `rg -n "pub fn search_entries|pub fn try_search_entries_with_scope|pub fn try_search_entries_indexed_with_scope|pub fn search_entries_with_scope" rust/src/search/mod.rs` should show public facade APIs remain in `search/mod.rs`.
  - `rg -n "pub fn build_index_with_metadata|pub fn build_index|pub enum IndexSource|pub struct IndexBuildResult" rust/src/indexer/mod.rs` should show public facade APIs remain in `indexer/mod.rs`.
- Targeted regression intents:
  - Search ranking/order: `orders_by_score_and_limit`, `limited_search_matches_full_indexed_ranking`, `parallel_collection_matches_sequential_ranking`, `multi_term_query_prioritizes_exact_term_hits`, `multi_term_query_prefers_literal_hits_per_token_over_subsequence_only_hits`.
  - FileList hierarchy: newer nested override, older nested ignored, newest-per-depth, and cancellation during nested FileList parse.
- Formatting:
  - format touched Rust files with `rustfmt`
  - touched-file `rustfmt --check` for changed search/indexer files, expected set: `src/search/mod.rs src/search/execute.rs src/search/match_eval.rs src/indexer/mod.rs src/indexer/filelist_reader.rs src/indexer/filelist_hierarchy.rs`
  - repository-wide `cargo fmt -- --check` is informative only until existing baseline failures are fixed; record known baseline files if checked
- Manual checks:
  - No manual GUI smoke is required unless search/index behavior changes beyond code movement. If behavior changes become necessary, stop and update this plan first.

## 9. Rollback Plan
- Revert the new private search/indexer modules and matching import/docs/test changes together.
- Because public APIs and behavior are intended to remain unchanged, rollback restores the previous `mod.rs` ownership without migration.
- If extraction reveals hidden semantic coupling, stop and split this slice into search-only and indexer-only follow-up slices before implementation.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap rule already present in `AGENTS.md`.

## 11. Progress Log
- 2026-04-25 Planned.
- 2026-04-25 Pre-change search perf baseline recorded: `perf_search_100k_candidates_reports_latency` printed `search_100k_elapsed_ms=44`.
- 2026-04-25 Extracted search matching/evaluation internals to `rust/src/search/match_eval.rs` while keeping public search facade APIs in `rust/src/search/mod.rs`.
- 2026-04-25 Extracted nested FileList hierarchy override internals to `rust/src/indexer/filelist_hierarchy.rs` while keeping public indexer types and build APIs in `rust/src/indexer/mod.rs`.
- 2026-04-25 Boundary checks passed:
  - negative search definition check on `rust/src/search/mod.rs`
  - positive search owner check on `rust/src/search/match_eval.rs`
  - negative indexer definition check on `rust/src/indexer/mod.rs`
  - positive indexer owner check on `rust/src/indexer/filelist_hierarchy.rs`
  - public search facade API check on `rust/src/search/mod.rs`
  - public indexer facade API/type check on `rust/src/indexer/mod.rs`
- 2026-04-25 Post-change search perf recorded: `search_100k_elapsed_ms=29`, below the 100ms target and below the pre-change baseline of 44ms.
- 2026-04-25 Validation passed:
  - `cd rust && cargo test --locked search::tests`
  - `cd rust && cargo test --locked indexer::tests`
  - `cd rust && rustfmt src/search/mod.rs src/search/execute.rs src/search/match_eval.rs src/indexer/mod.rs src/indexer/filelist_reader.rs src/indexer/filelist_hierarchy.rs`
  - `cd rust && rustfmt --check src/search/mod.rs src/search/execute.rs src/search/match_eval.rs src/indexer/mod.rs src/indexer/filelist_reader.rs src/indexer/filelist_hierarchy.rs`
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo llvm-cov --locked --workspace --lcov --output-path target/llvm-cov/lcov.info --fail-under-lines 70`
  - `cd rust && cargo audit`
  - `cd rust && cargo test --locked perf_search_100k_candidates_reports_latency --lib -- --ignored --nocapture`
  - `cd rust && cargo test --locked perf_filelist_stream_is_faster_than_metadata_probe_baseline --lib -- --ignored --nocapture`
  - `cd rust && cargo test --locked perf_walker_classification_is_faster_than_eager_metadata_resolution --lib -- --ignored --nocapture`
  - `git diff --check`
- 2026-04-25 `cargo audit` completed with the existing allowed transitive `paste` unmaintained warning through `wgpu`/`eframe`; this remains deferred to Slice G as planned.
- 2026-04-25 VM-003 perf results remained within guard expectations: FileList stream `188.406ms` vs metadata probe `282.781ms` (`1.50x`), walker fast classification `187.624ms` vs eager metadata `255.855ms` (`1.36x`).

## 12. Communication Plan
- Return to user if:
  - search scoring or FileList override behavior must change to complete extraction
  - perf gates fail for reasons not attributable to known baseline noise
  - public API expansion appears necessary
  - validation fails for unrelated baseline reasons that make the slice unsafe to commit

## 13. Completion Checklist
- [x] Slice reviewed according to required-before-and-after-revision
- [x] Search public facade remains stable
- [x] Indexer public facade remains stable
- [x] Private matching/evaluation and nested FileList hierarchy owners are extracted
- [x] Required validation passed
- [x] Roadmap/TASKS updated
- [x] Slice committed

## 14. Final Notes
This slice should prefer mechanical movement over redesign. It is a maintainability slice, not a search or indexing behavior change.
