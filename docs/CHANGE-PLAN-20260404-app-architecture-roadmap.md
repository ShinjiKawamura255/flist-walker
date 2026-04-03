# CHANGE PLAN: App Architecture Roadmap

## Metadata
- Date: 2026-04-04
- Owner: Codex
- Target Project: FlistWalker
- Scope Label: app-architecture-roadmap
- Related Tickets/Issues: God Object follow-up

## 1. Background
- `FlistWalkerApp` の God Object 解消は、`FileList`、`Update`、`root change`、tab lifecycle、tab activation/background restore、tab close cleanup、tab reorder まで段階的に分割してきた。
- その結果、主要な tab lifecycle と workflow の大半は局所化されたが、まだ `request_tab_routing` の owner が shared bag に近いこと、`render.rs` と `mod.rs` に残る横断 orchestration があること、slice 間の最終整理が残っている。
- 今後は「大きな枠」と「小さな枠」を分けて、上位ロードマップに沿って個別 slice を進める。

## 2. Goal
- 残りの app architecture 改善を、依存順のある roadmap と個別 slice 計画の二段で管理する。
- 上位 plan は、残りの主要領域と依存関係、完了条件、レビュー/検証方針を固定する。
- 下位 plan は、上位 plan の 1 slice を今までの粒度で具体化し、その slice 単体で plan-driven-changes を回す。

## 3. Roadmap Scope
### In Scope
- `request_tab_routing` の owner 局所化
- `render.rs` に残る UI orchestration の局所化
- `mod.rs` に残る cross-feature dispatch / coordinator cleanup
- docs / validation matrix の最終同期

### Out of Scope
- 検索契約や CLI 契約の変更
- updater / release / OSS compliance の再設計
- index/search worker protocol の全面変更

## 4. Remaining Workstreams
1. Request Routing Ownership
   - `RequestTabRoutingState` を shared bag のまま持たず、preview/action/sort の owner を近接 module へ寄せる。
   - 最初の下位 plan はこの workstream を対象にする。
2. Render/UI Orchestration
   - `render.rs` に残る dialog / action / reorder 周辺の coordinator を整理し、描画と state transition の境界をさらに明確化する。
3. Final Coordinator Cleanup
   - `mod.rs` に残る cross-feature dispatch と shared glue を見直し、`FlistWalkerApp` を coordinator として最小化する。
4. Docs and Validation Closure
   - `DESIGN.md` / `TESTPLAN.md` / `TASKS.md` を最終形へ同期し、一時 plan をすべて撤去する。

## 5. Execution Model
- 各 workstream は、別の下位 plan (`docs/CHANGE-PLAN-<date>-<slice>.md`) として具体化する。
- 下位 plan は、この roadmap のどの workstream に属するかを明記する。
- 実装順は原則として以下に従う。
  1. `request_tab_routing` owner localization
  2. `render.rs` UI orchestration cleanup
  3. `mod.rs` final coordinator cleanup
  4. docs / validation closure
- 依存関係やリスクが変わる場合は、先にこの roadmap を更新してから下位 plan を更新する。

## 6. Risks
- Risk: 下位 plan が局所最適になり、全体の依存順を壊す。
  - Impact: 高
  - Mitigation: 下位 plan は必ずこの roadmap を参照し、scope/順序変更時は roadmap も更新する。
- Risk: `render.rs` と `mod.rs` の cleanup を急ぎすぎると、既存 slice 境界を壊して逆流する。
  - Impact: 高
  - Mitigation: `request_tab_routing` と owner 局所化を先に終え、shared state を減らしてから UI orchestration へ進む。
- Risk: 仕上げ段階で docs がコード実態からずれる。
  - Impact: 中
  - Mitigation: 各下位 plan の exit criteria に docs 更新を含め、最後に closure workstream を設ける。

## 7. Validation Strategy
- 各下位 plan は、`docs/TESTPLAN.md` の Validation Matrix に従う。
- `request_tab_routing` や `render.rs` に触る slice では、少なくとも `cargo test` を前提とする。
- index/filelist/walker 経路へ触れる slice だけ、ignored perf テスト 2 本を追加実行する。
- 上位 roadmap 自体は docs-only とし、review は architecture 観点を優先する。

## 8. Exit Criteria
- roadmap 配下の主要 workstream が完了し、残りの大きい shared bag / coordinator concern が説明可能な小ささに収まっている。
- `AGENTS.md` に紐づく下位 plan が残っていない。
- `DESIGN.md` / `TESTPLAN.md` / `TASKS.md` が最終構造と一致している。

## 9. Relationship To Lower-Level Plans
- この roadmap は上位 plan であり、単独では実装しない。
- 実装は必ず下位 plan を通じて行う。
- 下位 plan は、この roadmap の該当 workstream、依存する前提、逸脱時に更新すべき上位項目を明記する。

## 10. Temporary Rule Draft
- For the remaining app architecture work, read both `docs/CHANGE-PLAN-20260404-app-architecture-roadmap.md` and the active lower-level change plan before starting implementation.
- Follow the roadmap first for scope/order decisions, then follow the lower-level change plan for implementation detail.
- If a lower-level plan changes the roadmap's scope, dependency, or order, update the roadmap first.
- Remove the temporary rule and delete both plans after the covered work is complete.
