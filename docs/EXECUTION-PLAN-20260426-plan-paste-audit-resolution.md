# EXECUTION PLAN: Paste Audit Warning Resolution

## Metadata
- Date: 2026-04-26
- Owner: Codex
- Target Project: FlistWalker
- Plan Role: single
- Execution Profile: light
- Planning Depth: single-plan
- Review Pattern: solo-main
- Review Requiredness: optional
- Scope Label: paste-audit-resolution
- Related Tickets/Issues: none
- Review Status: レビュー済み
- Review Notes:
  - 2026-04-26 initial feasibility review completed by main agent.
  - Current `cargo audit` reports allowed warning `RUSTSEC-2024-0436` for `paste 1.0.15` through the locked `eframe 0.29.1` GUI stack.
  - `cargo tree -i paste` and `cargo tree --target all -i paste` print no active dependency path, so the warning is lockfile/audit debt rather than a direct application dependency.
  - `cargo search eframe` shows latest `eframe 0.34.1`; `cargo info eframe@0.34.1` reports Rust requirement `1.92`. The local stable toolchain is `rustc 1.93.1`.

## 1. Background
The `quality-hardening-90` closure left one security-hygiene residual: `RUSTSEC-2024-0436` / `paste 1.0.15` is accepted transitive dependency debt. The project should attempt to remove it if the upstream GUI stack can be updated without broad product changes.

## 2. Goal
Resolve the accepted `paste` audit warning if feasible by updating the GUI stack and lockfile. If the update is too risky or fails validation, retain the accepted-risk posture with fresh evidence and stop before committing a risky partial upgrade.

## 3. Scope
### In Scope
- `rust/Cargo.toml`
- `rust/Cargo.lock`
- `docs/OSS_COMPLIANCE.md`
- `docs/TASKS.md`
- `THIRD_PARTY_NOTICES.txt` if dependency/license contents change materially
- This plan and `AGENTS.md` temporary rule cleanup

### Out of Scope
- GUI behavior redesign.
- Release tag or release publication.
- Self-update behavior changes.
- Adding new GUI automation framework.

## 4. Constraints
- Prefer the smallest dependency update that removes `paste`.
- Do not accept a GUI framework update if compile/test fixes become broad behavioral rewrites.
- Preserve configured `eframe` features: `default_fonts`, `glow`, `x11`, `wayland` unless the new version requires a documented equivalent.
- If dependencies change, run OSS compliance checks and update notices/docs as needed.

## 5. Execution Strategy
1. Update candidate dependencies.
   - Try `eframe` latest compatible update and refresh `Cargo.lock`.
   - Verify whether `paste` disappears from lockfile and `cargo audit`.
2. Fix only mechanical API changes.
   - Limit code edits to compile compatibility with the newer `egui` / `eframe`.
   - Stop if the update requires GUI behavior redesign.
3. Validate.
   - Required: `cargo test --locked`, `cargo clippy --all-targets -- -D warnings`, `cargo audit`, `cargo tree --target all -i paste`, `git diff --check`.
   - Run targeted GUI/headless tests if compile changes touch GUI rendering/input.
4. Update OSS/security docs.
   - Remove or revise the accepted `paste` warning section if `cargo audit` is clean.
   - Update `THIRD_PARTY_NOTICES.txt` if dependency inventory or licenses materially change.
5. Commit as one rollback unit.

## 6. Rollback Plan
- Revert this plan's commit to restore the previous `eframe 0.29.1` lockfile and accepted-risk documentation.
- If validation fails before commit, revert local dependency/code/doc changes and record the blocker in this plan.

## 7. Completion Checklist
- [ ] Dependency update attempted
- [ ] `paste` warning removed or infeasibility recorded
- [ ] Required validation passed or blocker recorded
- [ ] OSS compliance docs/notices updated if needed
- [ ] `AGENTS.md` temporary rule removed
- [ ] Change committed

