# EXECUTION PLAN: Slice G Dependency / Audit Follow-up

## Metadata
- Date: 2026-04-26
- Owner: Codex
- Target Project: FlistWalker
- Plan Depth: 2
- Plan Role: slice plan
- Execution Profile: safety-critical
- Planning Depth: roadmap+slice
- Review Pattern: specialist-subagents
- Review Requiredness: required-before-and-after-revision
- Execution Mode: none
- Execution Mode Policy: Inherits the parent roadmap policy. This slice resolves the planned dependency/audit follow-up by either eliminating the known audit warning or documenting accepted transitive risk with review cadence before closure.
- Parent Plan: `docs/EXECUTION-PLAN-20260425-roadmap-quality-hardening-90.md`
- Child Plan(s): none
- Scope Label: quality-hardening-90 / slice-g-dependency-audit
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-26 initial plan created after Slice F commit `e15afa3`.
  - 2026-04-26 specialist subagent review remains unavailable due the same quota exhaustion encountered in Slice F. Main-agent fallback review was performed with security/supply-chain and release-risk checklists.
  - Fallback review finding 1: do not perform a major GUI framework upgrade inside this slice because it would require GUI behavior validation and broaden rollback risk. Accepted; this slice is docs/audit posture only.
  - Fallback review finding 2: accepted warning must include owner, cadence, trigger, observed path, and required evidence. Accepted in `docs/OSS_COMPLIANCE.md`.
  - Fallback convergence: audit policy is not loosened, no dependency files are changed, and closure can evaluate the residual risk explicitly.

## 1. Background
`cargo audit` currently exits successfully but reports one allowed warning:

- `RUSTSEC-2024-0436`: `paste 1.0.15` is unmaintained.
- Dependency path: `paste -> metal -> wgpu-hal -> wgpu-core/wgpu -> egui-wgpu -> eframe -> flist-walker`.

Slices B and E accepted this warning temporarily and deferred the final posture to this slice. The roadmap success condition requires the warning to be either eliminated or documented as accepted transitive risk with review cadence.

## 2. Goal
Close the dependency/audit follow-up with a defensible supply-chain posture:

- Re-run `cargo audit` and record the exact current warning.
- Confirm the dependency path with `cargo tree -i paste`.
- Decide whether this slice should upgrade GUI dependencies or document accepted transitive risk.
- If not upgrading, document why the risk is accepted, what would trigger re-evaluation, and when it must be reviewed again.
- Keep CI/release audit gates intact.

## 3. Scope
### In Scope
- `docs/OSS_COMPLIANCE.md`
- `docs/TESTPLAN.md`
- `docs/TASKS.md`
- Parent roadmap update
- This slice plan
- Optional `.cargo/audit.toml` inspection only; do not loosen audit policy unless already configured.

### Out of Scope
- Major `eframe` / `egui` / `wgpu` upgrade unless review proves it is low-risk and required.
- Runtime behavior changes.
- GUI rendering/input changes.
- Release publishing or tag creation.
- Adding new dependencies.

## 4. Constraints and Assumptions
- This slice is supply-chain hygiene and documentation unless the audit warning can be removed with a low-risk patch-level dependency update.
- Any dependency or lockfile change must update `THIRD_PARTY_NOTICES.txt`, relevant docs, and run `docs/OSS_COMPLIANCE.md` checks.
- A GUI framework upgrade would need a separate reviewed slice because it can affect rendering, input, platform support, and release assets.
- The existing `cargo audit` warning is unmaintained status, not an active vulnerability report.
- Network access may be unavailable; if dependency version checks require network and fail, record that and prefer documented risk acceptance over unreviewed upgrades.

## 5. Current Risks
- Risk: Accepting an unmaintained transitive crate without cadence becomes permanent drift.
  - Impact: future security posture weakens.
  - Mitigation: document review cadence, triggers, owner, and audit command.
- Risk: Upgrading `eframe`/`wgpu` just to remove `paste` causes GUI regressions.
  - Impact: behavior changes outside this roadmap slice.
  - Mitigation: do not perform major GUI stack upgrade in this slice unless separately planned.
- Risk: Audit policy gets loosened silently.
  - Impact: CI may stop catching real advisories.
  - Mitigation: do not add broad ignores; keep `cargo audit` required and record the allowed warning as explicit risk posture.

## 6. Execution Strategy
1. Re-confirm current audit state
   - Files/modules/components: `rust/Cargo.lock`, `rust/Cargo.toml`, audit output.
   - Expected result: current warning and dependency path are captured.
   - Verification: `cd rust && cargo audit`; `cd rust && cargo tree -i paste`.
2. Decide remediation versus risk acceptance
   - Files/modules/components: `docs/OSS_COMPLIANCE.md`, this slice.
   - Expected result: either a safe update path is chosen or accepted-risk posture is documented.
   - Verification: plan progress log and docs diff.
3. Update permanent docs
   - Files/modules/components: `docs/OSS_COMPLIANCE.md`, `docs/TESTPLAN.md`.
   - Expected result: audit warning policy includes owner, cadence, triggers, and validation commands.
   - Verification: `rg` checks for advisory ID and cadence.
4. Update roadmap/TASKS and validate
   - Files/modules/components: roadmap, `docs/TASKS.md`, this slice.
   - Expected result: Slice G complete and closure slice can start.
   - Verification: `git diff --check`.

## 7. Detailed Task Breakdown
- [x] Review this slice plan with security/supply-chain and release-risk focus.
- [x] Run `cargo audit` and record exact current warning.
- [x] Run `cargo tree -i paste` and record dependency path.
- [x] Decide whether to upgrade dependencies or accept transitive risk with cadence.
- [x] Update `docs/OSS_COMPLIANCE.md` and `docs/TESTPLAN.md`.
- [x] Update roadmap/TASKS and mark Slice G complete.
- [x] Commit Slice G as an independent rollback unit.

## 8. Validation Plan
- Required commands:
  - `cd rust && cargo audit`
  - `cd rust && cargo tree -i paste`
  - `rg -n "RUSTSEC-2024-0436|paste|cargo audit|review cadence|accepted transitive" docs/OSS_COMPLIANCE.md docs/TESTPLAN.md docs/TASKS.md docs/EXECUTION-PLAN-20260425-slice-g-dependency-audit-follow-up.md`
  - `git diff --check`
- Conditional commands if dependencies change:
  - `cd rust && cargo test --locked`
  - `cd rust && cargo clippy --all-targets -- -D warnings`
  - `cd rust && cargo audit`
  - OSS notice review and `THIRD_PARTY_NOTICES.txt` update if dependency graph/license output changes.

## 9. Rollback Plan
- If documentation-only, revert this slice's docs and plan updates.
- If dependency changes are introduced, revert `rust/Cargo.toml`, `rust/Cargo.lock`, notices, and docs together.
- Do not partially keep a changed audit posture without matching docs.

## 10. Temporary `AGENTS.md` Rule Draft
Use the parent roadmap rule already present in `AGENTS.md`; update its active slice reference to this plan while Slice G is active.

## 11. Progress Log
- 2026-04-26 Planned.
- 2026-04-26 `cd rust && cargo audit` passed and reported the known allowed warning `RUSTSEC-2024-0436` for `paste 1.0.15` as unmaintained.
- 2026-04-26 Audit output path recorded: `paste 1.0.15 -> metal 0.29.0 -> wgpu-hal 22.0.0 -> wgpu-core/wgpu -> egui-wgpu 0.29.1 -> eframe 0.29.1 -> flist-walker 0.17.2`.
- 2026-04-26 `cd rust && cargo tree -i paste` printed no dependency path for the active target graph and suggested `--target all`; `cd rust && cargo tree --target all -i paste` required registry access outside the sandbox and then also printed no reachable package.
- 2026-04-26 Decision: do not upgrade `eframe` / `egui` / `wgpu` in this slice. Accept the warning as transitive/unmaintained status with release-candidate review cadence and explicit re-evaluation triggers in `docs/OSS_COMPLIANCE.md`.
- 2026-04-26 Validation passed:
  - `cd rust && cargo audit`
  - `cd rust && cargo tree -i paste`
  - `cd rust && cargo tree --target all -i paste`
  - `rg -n "RUSTSEC-2024-0436|paste|cargo audit|review cadence|accepted transitive" docs/OSS_COMPLIANCE.md docs/TESTPLAN.md docs/TASKS.md docs/EXECUTION-PLAN-20260425-slice-g-dependency-audit-follow-up.md`
  - `git diff --check`

## 12. Communication Plan
- Return to user if:
  - `cargo audit` reports an active vulnerability rather than the known unmaintained warning
  - a safe non-major dependency update appears possible but requires network or broad GUI validation
  - audit policy changes would be needed

## 13. Completion Checklist
- [x] Slice reviewed according to required-before-and-after-revision
- [x] Current audit warning and dependency path recorded
- [x] Remediation or accepted-risk decision recorded
- [x] OSS/test docs updated with cadence and triggers
- [x] Required validation passed
- [x] Roadmap/TASKS updated
- [x] Slice committed

## 14. Final Notes
This slice should not hide warnings. The intended outcome is either removal by safe update or explicit risk ownership until a planned GUI stack upgrade can remove the transitive dependency.
