# v0.30.0 Closure Validation

This change retains publication evidence, the exact public body, complete release classification and the final validation/review decisions. It changes documentation only. Product code, dependencies, workflows, trusted policy, settings, published tag/body/assets and runtime contracts are unchanged.

## Scope and selection

Base: `129ec84cbf24ffb02bba1cade5b75fa042fd0c45`. Branch: `codex/v0300-release-closure`, created after clean new-change worktree preflight. The validation plan and documentation/reference checks are recorded in the owning closure PR.

TDD is inapplicable because this is a documentation-only record of completed external actions. Substitute validation is the selected VM-001 doc diff/reference review, repository contract, exact public evidence comparison and independent closure review. Successful product/native/performance checks are retained at their original identities; no Rust/full GUI/performance suite is repeated for this text-only closure.

## Current checks

- Validation plan: PASS, only VM-001 selected across8 documentation files.
- Repository contract: PASS.
- Diff whitespace check: PASS.
- Local Markdown references: PASS,44 resolved references.
- Exact retained public body/hash and28 asset identities: PASS.
- Complete release classification: PASS,18 commits with mappings or explicit exclusions.

Documentation validation and final independent closure review are completed before the closure commit. Exact results and required CI Gate/CI Policy Guardian run/head/merge readback are retained in the owning PR, which is authoritative for protected merge completion. Publication and closure merge are separate completion conditions.

## Release evidence

- [Public download and identity checks](PUBLICATION-VALIDATION.md)
- [Exact28 public assets](PUBLICATION-ASSETS.json)
- [Retained public body](RELEASE-BODY.md)
- [Native repair and whole-release selection](GUI-REPAIR-VALIDATION.md)
- [Review dispositions](REVIEW-NOTES.md)

Windows GUI remains formally NOT RUN / ACCEPTED WITH DEVIATION. Mac native confirmation is selected representative local-build evidence reused after source/build reconciliation. No adjacent deterministic/CI axis relabels a missing native axis PASS. Exact-run external warning exceptions remain limited to their approved runs and emissions.
