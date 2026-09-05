# Regression Guards

Back to the [Validation Matrix](validation-matrix.md).

### Regression Guard: reclaimer drop observers are test-session isolated

- Scenario: global drop observer が有効な間に並列 test thread が resource payload を生成すると、その payload が observer sender を捕捉し、後から無関係な test 名を drop 証跡へ混入させる。
- Expected Behavior: observer は登録した test thread で生成された payload だけへ sender を埋め込み、実際の drop thread 名は reclaimer 側で記録する。別 test thread が同時に生成した payload は観測しない。
- Non-goals: production reclaimer の thread/queue 契約変更、Rust test runner の直列化、drop 完了順序の固定。
- Related Tests: TC-207、`tc_207_drop_observer_ignores_payloads_captured_by_parallel_test_threads`、reclaimer drop-thread assertions。
- Notes for Future Changes: process-global test hook は設定時の test identity を payload capture 条件へ含め、observer lock だけで並列 test との隔離を仮定しない。

### Regression Guard: Walker classification perf isolates per-entry metadata cost

- Scenario: 深い directory-heavy fixture では両経路に共通する再帰 `read_dir` が計測の大半を占め、通常 entry の追加 metadata probe を除いた効果が環境差で埋もれて 1.25x gate が不安定になる。
- Expected Behavior: 計測区間外で shallow/file-heavy fixture を構築し、両経路を warm-up 後に交互順で7回測定した median を比較する。候補件数の一致と既存の 1.25x 下限は維持する。
- Non-goals: しきい値の緩和、fixture 作成時間の計測、特殊 entry や symlink の分類契約変更。
- Related Tests: TC-083、`perf_walker_classification_is_faster_than_eager_metadata_resolution`。
- Notes for Future Changes: 再帰方式そのものを比較する場合はこの分類 gate に共有 traversal cost を混ぜず、adaptive walker matrix を使う。

### Regression Guard: incremental-finalization tests are bounded and progress-checked

- Scenario: 100k entry の background finalization test が `background_finalizations` の消滅または cursor 到達まで無期限に loop し、production の進捗停止時に test process 自体が終了しない。
- Expected Behavior: test driver は最大2,000 frameで停止し、計測 instrumentation による単発の budget 消費は許容しつつ、未完了状態で32 frameを超えて input remaining、completed、filter/kind cursor、output、scratch state のいずれも進まなければ失敗する。停止時は request_id、tab index、frame、連続停滞数、全 cursor state を診断へ含める。
- Non-goals: production の1 frame budget変更、100k fixtureの縮小、reclaimer Full rollbackを進捗として偽装すること。
- Related Tests: TC-207, `tc_207_stalled_background_finalization_guard_fails_deterministically_regression` と `tab_background_responses` の100k incremental finalization cases。
- Notes for Future Changes: finalization phase/cursor を追加した場合は共有 progress snapshot と target 判定を更新し、個別 test に新しい無期限 `while` を追加しない。

### Regression Guard: stateful response ownership is request-exact

- Scenario: endurance oracle が index/search の全 routed tab を response owner として除外し、実際には応答を消費しない別 tab の mutation を見逃す。または reclaimer 待機の根拠がない notice 変化を scheduler side effect として無条件に許す。
- Expected Behavior: 応答直前に oldest/newest として実際に選ばれる request_id と、前 event から submitted 済みで未消費の request_id の route owner だけを一次 owner とする。応答処理後は family/request_id/tab_id の組で新規 route を判定し、同じ tab の旧 route が残っていても新要求の owner transfer を識別する。resource reservation により lifecycle が新たに `Evicted` へ遷移した exact tab だけも証跡付き downstream owner とする。reclaimer notice の例外は、直前 snapshot に対応する reclaim-pending state があり、既知の reclamation/finalization phase 間を遷移する場合だけ認める。
- Non-goals: worker 完了順序の固定、非応答イベント間の tab mutation 比較、scheduler が正当に追加した新規 route の拒否。
- Related Tests: TC-183, TC-184, `tc_184_response_owner_is_exact_and_cross_owner_mutation_is_rejected_regression`, `tc_184_downstream_owner_uses_request_identity_when_tab_is_already_routed`, `tc_184_response_owner_includes_only_the_exact_newly_evicted_tab`, `tc_184_unrelated_reclaimer_wait_notice_is_not_ignored_regression`, `tc_184_reclaimer_wait_notice_requires_matching_pending_transition`, `tc_184_reclaimer_phase_notice_transition_requires_known_pending_phases`。
- Notes for Future Changes: response event を追加する場合は pending request の選択規則と request_id route を同じ owner resolver に接続する。notice 文字列だけで例外化せず、直前 state の因果フラグを snapshot に含める。

### Regression Guard: Recent Inactive engagement follows actual mutations

- Scenario: Clear Selected が PIN を解除しても engagement を記録しない一方、空 Results の Esc、history 先頭/末尾での移動、選択済み history row の再クリックが状態を変えずに engagement を記録する。
- Expected Behavior: query、selection、PIN、history selection など保護判定対象の状態が実際に変化した時だけ active tab を meaningfully engaged とする。PIN の全解除は非空から空への変更として記録し、境界操作・同一値設定・空状態の clear は記録しない。
- Non-goals: 2秒以上の active tenure による独立した保護、明示 Refresh の engagement 契約、notice/focus だけの変化を保護対象へ追加すること。
- Related Tests: TC-209, `tc_209_clear_pinned_marks_meaningful_interaction_regression`, `tc_209_empty_clear_query_attempt_is_not_meaningful_interaction_regression`, `tc_209_history_boundary_attempt_is_not_meaningful_interaction_regression`, `tc_209_same_history_row_click_is_not_meaningful_interaction_regression`。
- Notes for Future Changes: engagement は UI event の発生箇所ではなく state mutation owner で before/after を比較して記録する。同一値代入や clamp 済み navigation を新しい meaningful interaction として扱わない。

### Regression Guard: evicted selection survives partial result snapshots

- Scenario: Evicted tab の再 index 中、保存した selected path より前の batch だけで empty-query Results を更新する。または non-empty query の増分 search response が selected path をまだ含まない。
- Expected Behavior: partial snapshot の miss では selection restore intent を保持し、後続 snapshot に path が現れた時点でその行へ復元する。同 generation の成功した terminal index snapshot または index 完了後の authoritative search response にも path がなければ intent を破棄する。active/background の search 経路で同じ契約を維持する。
- Non-goals: limit 外の path の強制表示、失敗した search response での intent 破棄、root/query 境界を越えた selection 復元。
- Related Tests: TC-207, `tc_207_evicted_selection_survives_partial_empty_query_miss_and_restores_later_regression`, `tc_207_empty_query_terminal_absence_clears_evicted_selection_intent_regression`, `tc_207_background_empty_terminal_clears_absent_evicted_selection_intent_regression`, `tc_207_evicted_selection_survives_partial_search_miss_and_restores_later_regression`, `tc_207_authoritative_search_absence_clears_evicted_selection_intent_regression`, `tc_207_background_search_restores_evicted_selected_path`。
- Notes for Future Changes: incremental result reducer は restore intent を最初の miss で `take()` しない。intent の破棄は path match または generation の authoritative completion に限定する。

### Regression Guard: request mailbox closure preserves resident index capacity

- Scenario: refresh、root change、tab close が、worker が取得済みの request-scoped mailbox を通常 cleanup で閉じる。隣接する2要求で同時に起きると、mailbox への `Started` / data / terminal publish は失敗する。
- Expected Behavior: publish failure はその要求だけを終了し、常駐する2 index worker は次の要求を処理できる。shutdown で request receiver を閉じた場合だけ worker loop を終了する。
- Non-goals: 閉じた mailbox への応答再送、cleanup 済み要求の terminal 復元、index worker 数や mailbox 容量の変更。
- Related Tests: TC-206, `tc_206_closed_request_mailboxes_do_not_terminate_resident_index_workers_regression`。
- Notes for Future Changes: request mailbox の `Closed` / `SlotOccupied` は worker endpoint の切断と同一視しない。index response publish の失敗経路を変更した場合は、2 worker が隣接して mailbox close を受けた後の後続要求を必ず検証する。

### Regression Guard: restored-tab job and resource ownership
- Scenario: active priority is restored by preempting every background request or moving background batches into an unbounded deferred queue; closed/open-inactive tabs retain every heavy snapshot; refresh clears the last-good view.
- Expected: Active + sole Warm scheduling, ordered request-scoped bounded mailboxes, lifecycle plus optional committed snapshot, common live/closed LRU, engagement-qualified Recent Inactive soft protection with a hard bound, and bounded off-UI reclaimer satisfy TC-203 through TC-211.
- Non-goals: Persisting full snapshots across restarts or imposing a hard byte cap on one active FileList snapshot.
- Future-change rule: Changes to index dispatch/response, tab transition, close/restore, snapshot compaction, Recent Inactive classification/budget, or worker shutdown MUST run TC-203 through TC-211 as selected by the affected owner and MUST update SP-010/DES-009 when a bound or transition changes.

### Regression Guard: application-wide Emacs command mapping

- Scenario: picker/modal が通常キーを feature 内で直接処理し、共有 mapping を通さないため、その画面だけ Emacs 風 navigation/accept/cancel が無効になる。
- Expected Behavior: `emacs_keybindings_enabled=true` なら、通常の `Down` / `Up` / `Enter` / `Esc` を持つ全 GUI/TUI 対話面で `Ctrl+N` / `Ctrl+P` / `Ctrl+J` または `Ctrl+M` / `Ctrl+G` が同じ application command を実行し、全 application-owned 単一行入力が `Ctrl+A/E/B/F/H/D/K/Y/U` の共有 reducer を使う。設定無効時はアプリ独自操作として処理しない。
- Non-goals: 通常 command が存在しない画面への新規操作、OS/IME 所有の text composition、別設定が所有する GUI `Ctrl+W` 編集。
- Related Tests: TC-201, `regression_emacs_navigation_and_accept_apply_to_the_preset_picker`, `regression_emacs_preset_picker_shortcuts_respect_the_runtime_setting`, `regression_emacs_navigation_applies_to_the_named_root_manager`, `regression_emacs_cancel_closes_help_only_when_enabled`, `regression_emacs_ctrl_a_and_ctrl_d_edit_the_preset_filter`, `regression_emacs_ctrl_e_and_ctrl_h_edit_preset_editor_fields`, `regression_macos_native_ctrl_b_and_ctrl_f_move_preset_filter_once`, `regression_emacs_ctrl_k_and_ctrl_y_share_the_kill_buffer_in_preset_fields`, `regression_disabled_emacs_setting_prevents_native_ctrl_k_in_preset_fields`, `regression_emacs_text_editing_applies_to_the_gui_history_filter`, `regression_modal_singleline_fields_cannot_bypass_the_shared_emacs_adapter`, `tc_162_tui_emacs_navigation_pin_and_select_follow_runtime_toggle`, `tc_162_tui_emacs_query_editing_uses_the_same_runtime_toggle`, `tc_162_help_overlay_has_precedence_and_ctrl_g_only_closes_it`.
- Notes for Future Changes: 新しい picker/modal/overlay は GUI の共有 semantic helper または TUI の共有 input command mapping を使い、feature 内で通常キーと Emacs chord の対応を複製しない。新しい単一行入力は共有 text-editing adapter を使い、素の `TextEdit::singleline` で reducer を迂回しない。macOS では egui 自身が `Ctrl+A/E/B/F` を処理するため、共有 adapter は同じ cursor motion を二重適用しない。

### Regression Guard: query focus toggle and result cursor viewport tracking

- Scenario: Primary+L の pending focus flag が TextEdit 描画へ反映されず toggle できない。または仮想化した Results の保存位置を異なる ScrollArea ID で読み、既定 offset 0 として計算するため、十分下へ移動した後の上移動でも毎行 scroll する。current row の絶対位置を無条件に設定する場合も viewport 内の移動を壊す。
- Expected Behavior: Windows/Linux の `Ctrl+L` と macOS の `Cmd+L` は full frame を跨いで検索欄 focus を双方向に toggle する。`ArrowUp` / `ArrowDown` と有効な `Ctrl+P` / `Ctrl+N` は、query focus の有無と repeat に関係なく、行全体が既存 viewport 内なら offset を維持する。上端・下端を越えた場合だけ必要最小量を scroll し、上下反転・端の部分表示・resize 後も同じ契約を保つ。
- Non-goals: picker/modal が所有する focus、mouse wheel/scroll bar 自体の入力処理、PageUp/PageDown のページ移動量、IME composition 中の text editing。手動 scroll 後の位置からの keyboard 移動は対象に含む。
- Related Tests: TC-212, `regression_primary_l_toggles_query_focus_through_full_frames`, `regression_single_step_selection_does_not_pin_current_row_to_viewport_top`, `regression_results_cursor_round_trip_tracks_both_viewport_edges`, `regression_results_cursor_preserves_partial_scroll_after_resize`, `regression_results_cursor_clamps_at_list_ends_after_offscreen_jump`, `results_renderer_processes_only_visible_rows_regression`, `ctrl_n_and_ctrl_p_move_selection_even_when_query_is_focused`, `regression_arrow_keys_move_selection_even_when_query_focused`.
- Notes for Future Changes: shortcut dispatch は TextEdit 描画前に focus request を確定する。Results は ScrollArea 自身が保存する ID と同じ ID で state を読み、persisted offset と viewport 境界から visibility clamp を計算する。テストは full frame の入力→描画→無入力 frame を通し、ScrollArea 出力の offset/viewport と実際の行矩形を観測する。`show_rows` の範囲には部分表示・先読み行も含まれるため、描画対象への包含だけで可視性を判定しない。座標丸めの許容差は1物理pixel以内とする。意図的な「読み出し offset を常に0」「毎回先頭合わせ」が上下往復テストで失敗することを確認する。

### Regression Guard: update installation failure modal input ownership

- Scenario: automatic update failure後のmanual recovery modalがapplication shortcut dispatcherに登録されず、modal表示中の`Ctrl+T`、selection、query入力などが背面UIへ漏れる。
- Expected Behavior: install-failure modal表示中はshortcut/text/IME eventをmodalが所有し、`Enter` / `Esc` / enabled Emacs accept/cancelだけがdialogを閉じ、背面tab・query・selection・action stateを変更しない。
- Non-goals: updater download/apply処理、live network update、previous-update recovery evidence、release page hyperlinkのnative起動。
- Related Tests: TC-200, `regression_update_install_failure_owns_shortcuts_and_closes_without_background_action`, `failed_update_response_replaces_prompt_with_manual_recovery_state`, `tc_200_gui_surface_snapshot_exposes_install_failure_recovery`.
- Notes for Future Changes: blocking modalを追加した場合はrender layerだけでなく`handle_shortcuts`の最優先ownershipへ接続し、表示初回frameから背景shortcutを遮断する。

### Regression Guard: updater restart success and display normalization

- Scenario: Windows GUI自己更新のcommit直後に新GUIが通常startupとしてlive helperを検査し、一時的なidentity照会失敗を`Ambiguous`と誤判定して、versionは更新済みなのに`Previous Update Failed`を表示する。深いerror contextが複数のverbatim pathを連結すると、個別path helperだけでは`\\?\`が残る。
- Expected Behavior: helper起動のGUIは専用internal flagでterminal handoff recoveryを先行し、helper終了とmarker/hash再検証後だけ通常GUIへ進む。保存診断・update state・status noticeは共有文字列表示境界を通り、drive/UNCを含む文中すべての`\\?\`を除去する。
- Non-goals: 通常startupのfail-closed identity契約、filesystem APIへ渡すverbatim path、30秒超のhelper強制終了、production binaryを置換するunit test。
- Related Tests: TC-202, `tc202_regression_gui_restart_uses_internal_recovery_handoff`, `tc202_regression_headless_restart_keeps_terminal_internal_dispatch`, `tc202_regression_failure_record_hides_embedded_windows_verbatim_prefixes`, `tc202_regression_display_text_strips_all_embedded_verbatim_path_prefixes`, `tc202_regression_display_text_preserves_non_verbatim_content`, `tc202_regression_status_notice_hides_embedded_verbatim_paths`.
- Notes for Future Changes: updater error/noticeの新しい表示面は個別replaceを追加せず共有文字列表示境界へ接続し、process entryまたはrestart flag変更時はGUI/Headless双方のfocused testを実行する。

### Regression Guard: focused Ctrl+W query editing priority

- Scenario: GUI の application shortcut が TextEdit より先に `Ctrl+W` をタブ終了として消費し、opt-in した Emacs 単語削除が実行されない。または TextEdit と独自 reducer が同じ event を処理して2語削除する。
- Expected Behavior: Emacs と `ctrl_w_deletes_word_in_query` が有効な通常検索欄・履歴検索フィルターでは `Ctrl+W` が直前単語だけを一度削除し、タブ数を変えない。検索欄外またはいずれかの設定が無効ならタブ終了を維持し、IME 合成中はどちらも起動しない。
- Non-goals: TUI の既存 `Ctrl+W`、他の Emacs chord、タブ close ボタン、macOS の `Cmd+W`。
- Related Tests: TC-199, `regression_opted_in_ctrl_w_deletes_query_word_without_closing_tab`, `regression_opted_in_ctrl_w_deletes_history_filter_word_without_closing_tab`, `regression_opted_in_ctrl_w_during_ime_changes_neither_query_nor_tabs`, `regression_ctrl_w_still_closes_tab_outside_opted_in_query_editing`, `regression_ctrl_w_does_not_close_tab_during_opted_in_ime_composition`.
- Notes for Future Changes: `run_ui_frame`、shortcut ordering、TextEdit adapter を変更する際は Ctrl+W の owner を一つに保ち、同じ key event を global close と text reducer の双方へ渡さない。

### Regression Guard: TUI Windows extended-path display

- Scenario: Windows で canonical root が `\\?\D:\...` または `\\?\UNC\...` になり、TUI の options summary など一部表示だけが raw `Path::display` を使うと extended prefix が露出する。
- Expected Behavior: TUI が所有する全ユーザー向け root path は共有表示境界を通り、drive path は `D:\...`、UNC path は `\\server\share\...` と表示される。
- Non-goals: filesystem I/O、path identity、認可、CLI stdout のmachine-readable framingは変更しない。
- Related Tests: `tc_177_regression_tui_root_surfaces_strip_drive_and_unc_extended_prefixes`, `tc_177_regression_tui_path_rendering_never_uses_raw_os_strings`.
- Notes for Future Changes: `rust/src/cli_tui.rs` と `rust/src/cli_tui/` の本番コードでは user-facing path を `.display()` / `to_string_lossy()` で直接文字列化せず、TUI共有表示境界を使う。

### Regression Guard: FileList test ancestor isolation

- Scenario: FileList propagation testのrootをsystem tempへ置くと、production同様のancestor探索がfixtureを越え、developer profileのpermissionや実在FileListにtest結果が依存する。
- Expected Behavior: unit testは明示的なexclusive ancestor boundaryでfixture内の複数FileListへの伝播を維持し、全plan targetをfixture内へ限定する。実binaryを使うCLI contract fixtureはworkspace内へ置き、developer profileを探索しない。
- Non-goals: productionのancestor探索範囲、CLIの`--propagate-ancestors`契約、FileList更新順序の変更。
- Related Tests: `regression_bounded_ancestor_plan_stays_inside_fixture_and_preserves_propagation`, `tc_165_batch_create_filelist_wires_overwrite_ancestors_and_saved_roots`, TC-165, TC-166.
- Notes for Future Changes: propagation testでsystem tempのrootをproduction APIへ直接渡さず、paired boundary helperとVM-006を維持する。

### Regression Guard: Windows release archive-local names

- Scenario: PowerShell packagingがflat release asset用の`FlistWalker-<version>-windows-x86_64.README.txt`をそのままZIPへ渡し、archive契約の`README.txt`が欠落する。
- Expected Behavior: Windows ZIPのarchive rootは`flistwalker.exe`、`README.txt`、`LICENSE.txt`、`THIRD_PARTY_NOTICES.txt`の4項目だけを含む。
- Non-goals: GitHub Release上のsidecar asset名、Linux/macOS archive形式、README本文の変更。
- Related Tests: TC-178, `scripts/test-prepare-release-archive.ps1`.
- Notes for Future Changes: `scripts/prepare-release.ps1`後に生成ZIPへTC-178を実行し、flat asset名とarchive-local名を別契約として維持する。

### Regression Guard: result cursor preservation after refresh

- 発生条件: 検索結果の更新時に 100 行目へカーソルがある状態で結果数が 100 未満へ減る、または current row が未選択のまま再検索が走る。
- 期待動作: current row はユーザ操作なしで別の行へ移動せず、保持できる場合は同じ行番号を維持し、縮小した場合のみ末尾へ丸める。未選択状態は自動選択に変換しない。
- 非対象範囲: 手動の Arrow キー移動、Sort 切替、Root 変更による既存 selection 破棄。
- 関連テストID: TC-068.

### Regression Guard: Windows copy notice state owner

- 発生条件: `copy_selected_paths` の Windows-only テストで、`FlistWalkerApp` の旧 `notice` 直参照が残る。
- 期待動作: notice は live runtime の `app.shell.runtime.notice` を参照し、`\\?\` 付きの extended prefix を正規化した結果だけを検証する。
- 非対象範囲: copy パス実装そのものの出力形式変更、Windows 以外の OS の path normalization。
- 関連テストID: TC-121.

### Regression Guard: Shift primary copy event routing

- 発生条件: `egui-winit` が `Ctrl+Shift+C` / `Cmd+Shift+C` を `Event::Copy` に変換し、`Key::C` の shortcut test だけでは path copy 経路が検知できない。
- 期待動作: Shift 付き primary copy event は選択中または PIN 済み path をコピーし、Shift なしの通常 copy event は path copy shortcut として扱わない。
- 非対象範囲: TextEdit 内の通常 query text copy、Copy Path(s) ボタン経由の直接実行。
- 関連テストID: TC-018.

### Regression Guard: visible-only walker kind resolution

- 発生条件: Walker 完了後に visible な結果が少数しかないのに、全件 kind 解決が走って巨大な on-demand root を走査し続ける。
- 期待動作: kind 解決は visible results に限定し、検索/index が停止済みの idle 状態では全件 metadata 解決を継続しない。
- 非対象範囲: Files / Folders の単一フィルタ時に必要な kind 解決、preview 要求に伴う単発の kind 解決。
- 関連テストID: TC-122.
