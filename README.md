# FlistWalker

FlistWalker is a Rust GUI/CLI tool that provides an `fzf --walker`-style experience for fast fuzzy searching of files and folders, then opening or executing the selected result.

- Display name: `FlistWalker`
- GitHub repository: `flist-walker`
- Universal binary: `flistwalker` (`FlistWalker.exe` on Windows release assets), providing GUI, batch CLI, and TUI
- Fast CLI-only binary: `fw` (`fw.exe` on Windows), using the same CLI/TUI contract without `--cli`

Language docs:

- Japanese: [README-ja.md](README-ja.md)
- Support: [docs/SUPPORT.md](docs/SUPPORT.md)
- Documentation index: [docs/INDEX.md](docs/INDEX.md)
- Project status for maintainers: [docs/CURRENT_STATUS.md](docs/CURRENT_STATUS.md)

## Features

- Multi-tab workspace
- FileList priority loading from the repository root only: `FileList.txt` / `filelist.txt`
- Fast file and folder indexing plus search
- FZF-compatible query operators: exact match (`'`), exclusion (`!`), prefix (`^`), suffix (`$`)
- Highlighted matches, hide-non-matches mode, and pinned multi-selection
- Preview panel with on-demand file skipping
- Saved roots and default root support
- Shared search history across tabs
- `Create File List` generation from the current root
- Ignore list support via `flistwalker.ignore.txt` next to the executable
- Runtime config and session file support under `%LocalAppData%\flistwalker\` on Windows, or under `~/.flistwalker/` on Linux/macOS

## Quick Start

```bash
cd rust
source ~/.cargo/env
cargo run --bin flistwalker -- --root ..
```

1. Type in the search box to narrow candidates.
2. Press `Enter` to open or execute the selected item.
3. Press `Shift+Enter` to open the containing folder; identical folders are opened only once.
4. Press `Tab` / `Shift+Tab` to toggle pinned multi-selection.
5. Press `Ctrl+Shift+C` to copy the selected path (`Cmd+Shift+C` on macOS).
6. Press `Ctrl+R` to fuzzy-search query history, then `Enter` / `Ctrl+J` / `Ctrl+M` to load it into the search box.

### Main Shortcuts

- `F1`: open or close the in-app help for keyboard shortcuts and query syntax (also available from `Help`)
- `Ctrl+Shift+P`: open the preset picker, fuzzy-filter names, apply with `Enter`, add the current search, edit with `F2`, or delete the selected preset (`Cmd+Shift+P` on macOS)
- `Up` / `Down` or `Ctrl+P` / `Ctrl+N`: move the current row
- `Ctrl+V` / `Alt+V`: page navigation
- `Enter` / `Ctrl+J` / `Ctrl+M`: open or execute
- `Shift+Enter`: open the containing folder
- `Tab` / `Shift+Tab` / `Ctrl+I`: toggle pin on the current row
- `Ctrl+Shift+C`: copy selected paths
- `Esc` / `Ctrl+G`: clear query and pinned items
- `Ctrl+L`: focus the search box
- `Ctrl+T`: new tab
- `Ctrl+W`: close the current tab
- `Ctrl+Tab` / `Ctrl+Shift+Tab`: switch tabs
- Drag and drop a tab to reorder tabs

## Shortcut Differences on macOS

On macOS, the following primary shortcuts are mapped from `Ctrl` to `Cmd`:

- `Ctrl+T` / `Ctrl+W`
- `Ctrl+L`
- `Ctrl+Shift+C`
- `Ctrl+Shift+P`

Tab switching still uses `Ctrl+Tab` / `Ctrl+Shift+Tab` on macOS.

## Query History

- Search history is shared across tabs and persisted up to 100 entries.
- History is saved in the session file under `%LocalAppData%\flistwalker\` on Windows, or under `~/.flistwalker/` on Linux/macOS. Avoid putting sensitive data in search terms or paths.
- Set `history_persist_disabled` to `true` in the runtime config file to disable history load and save.
- Press `Ctrl+R` to enter history search mode and fuzzy-search the same query box.
- While in history search, `Enter` / `Ctrl+J` / `Ctrl+M` loads the selected history entry into the search box, and `Esc` / `Ctrl+G` cancels and restores the previous query.
- History is not written on every keystroke. It is committed after a short idle period or when result navigation starts.
- Intermediate IME composition text is not stored; only committed query text is kept.

## Session Restore

- Set `restore_tabs_enabled` to `true` in the runtime config file to restore the previous tab state on the next launch.
- Restored fields include `root`, `query`, `Use FileList`, `Regex`, `Files`, `Folders`, and the active tab.
- If you explicitly pass `--root` or a startup query, those values take precedence over restore.
- When this feature is enabled, `Set as default` is disabled because the startup root is determined by tab restore.

## Runtime Configuration

- On Windows, runtime settings files are stored under `%LocalAppData%\flistwalker\`. On Linux/macOS, they are stored under `~/.flistwalker/`.
- The runtime config file is created from the current `FLISTWALKER_*` environment values on first launch if it does not exist yet.
- Only values that are actually set by environment variables are written on first launch; unset options are omitted and fall back to built-in defaults when loaded.
- Once the file exists, it becomes the source of truth for runtime settings and the matching environment variables are only an initial seed.
- The same Windows-vs-home placement also applies to UI state, saved roots, and window trace files.
- If you upgrade from an older build, the first launch will automatically move legacy executable-side or home-directory files into the new platform-specific location when the new files do not already exist.
- Only the commonly useful toggles are documented here. Advanced keys are intentionally undocumented.
- The file is plain JSON, so you can edit it directly.
- If you delete the file, the next launch will recreate it from the current environment values.
- `walker_max_entries` is also exposed here because it caps large-root Walker scans in both the GUI and TUI.
- Set `emacs_keybindings_enabled` to `false` to disable Emacs-like shortcuts such as `Ctrl+N`, `Ctrl+P`, `Ctrl+V`, `Alt+V`, `Ctrl+J`, `Ctrl+M`, and query editing chords in both the GUI and TUI. It is enabled by default.
- Set `tab_pin_moves_to_next_row` to `true` to move to the next result after `Tab`, `Shift+Tab`, or enabled `Ctrl+I` pin toggles in both the GUI and TUI. It is disabled by default.

Example:

```json
{
  "walker_max_entries": 500000,
  "history_persist_disabled": false,
  "restore_tabs_enabled": false,
  "emacs_keybindings_enabled": true,
  "tab_pin_moves_to_next_row": false
}
```

- Keep values that should behave like booleans as `true` / `false`.

To enable session restore, set the runtime config value directly:

```json
{
  "restore_tabs_enabled": true
}
```

## Public Environment Variables

- Runtime settings are now config-file driven through the platform-appropriate settings files.
- On Windows, those settings files live under `%LocalAppData%\flistwalker\`; on Linux/macOS, they live under `~/.flistwalker/` and related files.
- The matching `FLISTWALKER_*` variables are only used to seed the config file when it does not exist yet.
- Signing and release build variables are documented only in [docs/RELEASE.md](docs/RELEASE.md).

## Rust App

```bash
cd rust
source ~/.cargo/env
cargo run --bin flistwalker -- --root ..
```

CLI mode:

```bash
# Recommended short command from a release asset:
fw "main" --root .. --limit 1000

# The universal binary remains compatible:
flistwalker --cli "main" --root .. --limit 1000
```

`fw` excludes the GUI entry path and is optimized for short one-shot invocations. All CLI examples below may replace `flistwalker --cli` with `fw`.

In CLI mode:

- If no query is provided, the tool prints up to `limit` candidates.
- If a query is provided, matching paths are printed one per line.
- `--limit` is treated as a real upper bound, not rounded down to 1000.
- `--max-depth N` limits candidates to `N` path components below the root (`1` means direct children). Directories at depth `N` remain candidates but are not descended into. Omitting it keeps traversal unlimited; `0` is rejected. The same limit applies to Walker, FileList, batch CLI, TUI startup, and GUI startup, while FileList creation always remains complete and unlimited.
- Output is root-relative, newline-delimited, and free of scores/ANSI by default. Use `--absolute` and `--print0` for scripts.
- Compatibility: query output is now the same script-safe path-only format as empty-query output. Consumers of the former `[score] absolute-path` rows should use `--absolute`; score text is no longer emitted.
- `--type all|file|folder`, `--regex`, and `--case-sensitive` control matching.
- `--source auto|filelist|walker` controls indexing. `filelist` fails if no root FileList exists; `auto` prefers it and falls back to the walker.
- `--ignore-file PATH` replaces the executable-side ignore list; `--no-ignore` disables ignore filtering. These options conflict.
- Ignore files are UTF-8 and may start with a UTF-8 BOM or use CRLF. `/` and `\\` path separators are equivalent in ignore rules. A missing default sidecar means no rules; a present unreadable or invalid UTF-8 file is an explicit CLI/TUI error.
- In batch mode, `--progress` writes indexing start, indexed candidate count/time, and match/return count/time only to stderr. Batch-only `--fail-no-match` changes an empty result from exit 0 to exit 1; interactive mode rejects both options. Cancellation exits 130.
- `--sort score|name-asc|name-desc|modified-desc|modified-asc|created-desc|created-asc|size-desc|size-asc` sorts before `--limit`. `--use-default-root`, `--saved-root INDEX`, and `--list-saved-roots` provide explicit access to persisted roots; listing supports `--print0`.
- Named roots and pure-search presets can be managed in CLI/TUI and from the GUI picker's contextual controls. See [Named Roots and Search Presets](#named-roots-and-search-presets) for the complete workflow.
- Scope individual terms with `name:`, `path:`, `dir:`, or `ext:`. Existing unscoped terms still search the full visible path. Fields compose with `!`, `'`, `^`, `$`, token-local `|`, and regex mode; quote the complete QUERY even when it is a single token so shell parsing stays predictable.
- `--action print|open|reveal` defaults to `print`. Open/reveal write diagnostics only to stderr and require `--action-all` before targeting more than one result; they reject `--absolute` and `--print0`.
- `-x` / `--exec` consumes the remaining command template and replaces exactly one standalone `{}` argument with every post-limit result as separate absolute argv values. Results are packed greedily up to the current platform command-line limit and run sequentially; `--exec-max-args N` adds a per-batch path cap and `--dry-run` reports counts without starting the command. Zero results start no command. Exec mode rejects output framing and built-in non-print action options and never invokes a shell implicitly; put all FlistWalker options before `-x`.
- The child inherits FlistWalker's user privileges, environment, and standard streams. On Windows, direct `.bat` / `.cmd` programs are rejected to prevent an implicit shell launch. Shell interpreters and batch scripts have their own parsing rules; using `sh -c`, `cmd.exe /C script.cmd`, or PowerShell command strings explicitly opts into those rules.
- `--create-filelist` builds a fresh walker-based root FileList without prompting and writes no stdout. `--overwrite-filelist` is required to replace an existing root FileList; `--propagate-ancestors` is an explicit opt-in for existing ancestor FileLists. Creation rejects query/search/output/action options, returns 0 on success, 130 on clean cancellation, and 1 for read/write/rollback failures.

### Named Roots and Search Presets

Named roots assign a stable name to a search root. Presets save a named root or root snapshot together with the query, entry type, source, regex, case-sensitivity, ignore, sort, and maximum-depth settings. They never save an action or external command. In the GUI, press `Ctrl+Shift+P` (`Cmd+Shift+P` on macOS), type to fuzzy-filter preset names, use `Up` / `Down`, and press `Enter` to apply the selected preset to the current tab or `Esc` to close without changing the current search. Applying a preset changes only the active tab; its depth remains in effect until changed from the `Depth: All` / `Depth: ≤ N` control or by another preset. Other tabs are unchanged, new tabs start at `All`, and the state is restored with the tab session. The depth control is on the second main-panel row after `Folders` and before `Preview`. Use `Add` to open a draft initialized from the current tab's pure-search state. Press `F2` or use `Edit` to edit the selected preset, and use the `Delete` button to remove it after confirmation. `Ctrl+Enter` (`Cmd+Enter` on macOS) or `Save` updates only the catalog and does not apply the preset. Use `Manage named roots...` in the picker heading or `Manage...` in the preset editor to list, add, rename/repath, or delete named roots. Renames update preset references; deletes leave each preset's saved path snapshot available as its fallback. `Esc` / `Cancel` discards a draft or deletion confirmation. The GUI picker never opens or executes a search result and adds no permanent preset control to the main panel.

```bash
# Register a named root. Quote the complete NAME=PATH value when the path may contain spaces.
flistwalker --cli --add-named-root "work=./my-project"

# Save the current pure-search options and exit without running the search.
flistwalker --cli "dir:src ext:rs !dir:target" --named-root work --type file --source walker --sort name-asc --save-preset rust-src

# Inspect the saved names.
flistwalker --cli --list-named-roots
flistwalker --cli --list-presets

# Apply the preset in batch mode or start the TUI with it.
flistwalker --cli --preset rust-src
flistwalker --cli --interactive --preset rust-src

# Remove catalog entries when they are no longer needed.
flistwalker --cli --remove-preset rust-src
flistwalker --cli --remove-named-root work
```

Quote the complete query argument even when it contains only one term. `--preset` cannot be combined with an explicit query or with root, entry-type, source, regex, case, ignore, sort, or `--max-depth` selectors because those values come from the preset. Invocation-specific options such as `--limit`, output framing, and explicit actions remain available when applying it.

Examples:

```bash
# Create a new root FileList; refuse an existing one unless overwrite is explicit.
flistwalker --cli --root . --create-filelist
flistwalker --cli --root . --create-filelist --overwrite-filelist

# Explicitly open every post-limit match (stdout remains empty).
flistwalker --cli "report" --root . --limit 10 --action open --action-all

# Match Rust files under a src directory while excluding generated directories.
flistwalker --cli "dir:src ext:rs !dir:generated" --root .

# Pass every post-limit match to an external command in platform-sized batches.
flistwalker --cli "report" --root . --exec-max-args 100 -x archive-tool -- {}

# Inspect the number of paths and batches without starting the command.
flistwalker --cli "report" --root . --dry-run -x archive-tool -- {}
```

In PowerShell, quote the placeholder as `'{}'` so it is passed as an argument instead of parsed as a script block.

For shell-safe path handling:

```bash
flistwalker --cli --root . --type file --print0 | xargs -0 -n1 printf '%s\n'
```

Interactive CLI mode:

```bash
fw --interactive --root ..
```

The Windows release keeps `FlistWalker.exe` for GUI, batch CLI, and interactive TUI compatibility and adds `fw.exe` as a CLI-only standalone asset. PowerShell and cmd wait for CLI/TUI completion and receive its exit code normally; only the universal GUI mode detaches from the console before the native window starts.

The TUI reads the same runtime shortcut settings as the GUI. With `emacs_keybindings_enabled=true`, it accepts `Ctrl+N` / `Ctrl+P`, `Ctrl+V` / `Alt+V`, `Ctrl+I`, `Ctrl+J` / `Ctrl+M`, `Ctrl+G` / `Ctrl+R`, and Emacs-style editing in both the normal query and history filter. The contextual help reflects whether these shortcuts are enabled. `tab_pin_moves_to_next_row` controls whether `Tab`, `Shift+Tab`, and enabled `Ctrl+I` advance after toggling a pin.

The TUI Walker uses the same adaptive traversal limits and `walker_max_entries` cap as the GUI. When the cap is reached, the status line reports the applied limit. FileList creation still builds a complete fresh Walker snapshot and is not truncated by the display-index cap.

This starts a lightweight terminal UI. `--root`, `--use-default-root`, and `--saved-root` select its startup root; `--max-depth` fixes the startup/preset depth for the TUI session, `--sort` sets its initial ordering, and `--no-ignore` starts with Ignore visibly disabled. The F2 options overlay does not edit maximum depth. CLI output is plain by default; use `--color` (or `--color auto`) to highlight matched text on a TTY, `--color always` to force it, or `--color never` to disable it. `auto` respects a non-empty `NO_COLOR` environment variable, while piped paths remain plain. Edit with `Left` / `Right` / `Home` / `End` / `Backspace` / `Delete` or paste; navigate with `Up` / `Down` / `PageUp` / `PageDown`; press `Tab` / `Shift+Tab` to pin results in output order and `Enter` to print them.

`F2` opens an apply/cancel options overlay for Files, Folders, Regex, Ignore Case, loaded Ignore terms, and source (`Auto` / `FileList` / `Walker`); source and file-kind changes reindex, while search-only changes reuse the current snapshot. `F3` selects Score, name, modified, created, or size ordering (ascending/descending where applicable); non-score ordering ranks all matches before applying the limit. `F4` opens saved roots and switches to the highlighted root; `F5` refreshes the current root. `F6` creates a FileList after choosing root-only or ancestor propagation; an existing root FileList requires a separate overwrite confirmation. Creation runs in the background, and pending selection, exit, or root-switch requests wait for its committed cancellation/rollback result. Root switching clears old selections and pins but keeps the query, history, and options; refresh keeps pins.

`Ctrl+O` opens or executes only the current row, while `Shift+Enter` reveals only its containing folder; pinned rows are never included in either action. With Emacs keybindings enabled, `Ctrl+G` clears the query and pins and `Ctrl+R` opens persisted query-history search. `Alt+P` toggles the width-aware preview, and `F1` opens contextual help. In history, help, options, sort, root, or FileList overlays, `Enter`, `Esc`, and enabled `Ctrl+G` apply or close only that overlay; `Ctrl-C` always cancels the whole TUI. `Esc` / `Ctrl-C` in normal mode restores the terminal, prints nothing, and exits 130. The TUI requires terminal stdin and stderr, while stdout may be redirected, so `flistwalker --cli --interactive > selection.txt` is supported. All screen/status output stays on stderr; after terminal restoration, selected paths are either written to stdout or passed to the explicit `-x` command.

## Behavior

- If `FileList.txt` or `filelist.txt` exists at the repository root, it is loaded first.
- Nested `FileList.txt` / `filelist.txt` entries under the root FileList are expanded as needed.
- If no list exists, the app falls back to recursive walker-based scanning.
- Selecting a file opens or executes it via the default app; selecting a folder opens it in the file manager.
- `Create File List` creates a new walker-based tab when needed and writes a fresh `FileList.txt`.

### Option Checkboxes

- `Use FileList`: prefer `FileList.txt` / `filelist.txt`
- `Files`: toggle file visibility
- `Folders`: toggle folder visibility
- `Regex`: enable regular-expression search
- `Preview`: show or hide the preview pane
- `Use Ignore List`: enable or disable executable-relative ignore rules. It is on by default.

### Ignore List

- Put `flistwalker.ignore.txt` in the same folder as `flistwalker` or `FlistWalker.exe`.
- One rule per line is the simplest form. Blank lines and lines starting with `#` are ignored.
- Each token is applied like a literal search exclusion, without fuzzy matching. For example, `old` and `~` behave like typing `!old !~`.
- You can also place multiple terms on one line, separated by spaces.
- The `Use Ignore List` checkbox controls whether these rules are applied. It is enabled by default.
- A sample file is available at [flistwalker.ignore.txt.example](flistwalker.ignore.txt.example).
- If the sample is missing, FlistWalker recreates it beside the executable at startup.
- Rename `flistwalker.ignore.txt.example` to `flistwalker.ignore.txt` if you want it to act as the live ignore list.

### Root Actions

- `Browse...`: change root
- `Set as default`: save the current root for the next launch
- `Manage list`: open a non-blocking native saved roots manager window. Add folders by typing a path or using `Browse...`. Select one saved root and use `Edit` to change its path. Use `Remove...` to enter a dedicated removal mode where checkboxes are shown for multi-select removal. Commit draft changes with `Apply` or `OK`; `Cancel` closes the manager without changing the saved roots list.

## Testing

```bash
cd rust
source ~/.cargo/env
cargo test
```

On Windows PowerShell, day-to-day development should use the same GNU target
family as the distributed Windows build:

```powershell
.\scripts\dev-check-windows.ps1
```

The script verifies `rustup`, `rustc`, `cargo`, the `x86_64-pc-windows-gnu`
Rust target, and the mingw-w64 tools, then runs `cargo test --target
x86_64-pc-windows-gnu` from `rust/`. Add `-IncludeFmt` or `-IncludeClippy`
when you want the optional formatting or clippy checks too. Use
`-Toolchain msvc` only as a local fallback when the GNU toolchain is not
installed yet.

Required one-time setup:

```powershell
rustup target add x86_64-pc-windows-gnu
```

Install MSYS2 to `C:\msys64`, then install the MINGW64 GCC package from an
MSYS2 shell:

```bash
pacman -S mingw-w64-x86_64-gcc
```

`scripts/dev-check-windows.ps1` automatically adds `C:\msys64\mingw64\bin` for
the current check when it exists.

For a release-equivalent Windows build directly from PowerShell:

```powershell
.\scripts\build-rust-win.ps1
# clean build
.\scripts\build-rust-win-clean.ps1
```

The scripts detect Rustup, the GNU Rust target, MSYS2, and the required MinGW
tools. When something is missing, the normal mode explains the package and
asks before each installation. Use `-CheckOnly` for detection only,
`-NoInstall` to fail with manual commands without prompting, or
`-InstallMissing` for explicit unattended approval.

## Support and Bug Reports

Use the GitHub Issues templates for bug reports and feature requests. Before filing an issue, read [docs/SUPPORT.md](docs/SUPPORT.md) and redact usernames, project names, full paths, tokens, and other sensitive data.

## Windows Build

For normal development on Windows, prefer the GNU target flow above so local
checks match the distributed Windows artifact:

```powershell
cd rust
cargo test --target x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu
```

The MSVC host target is still useful as a fallback for quick Windows UI smoke
checks, but it is not the release-equivalent build.

From Windows PowerShell:

```powershell
.\scripts\build-rust-win.ps1
```

From WSL or a Linux shell:

```bash
./scripts/build-rust-win.sh
```

Both paths build `x86_64-pc-windows-gnu` and preserve the Explorer icon and
`asInvoker` manifest. The PowerShell path can guide installation of missing
Rustup/MSYS2 dependencies after explicit approval.

Required tools:

- `x86_64-w64-mingw32-gcc`
- `x86_64-w64-mingw32-g++`
- `x86_64-w64-mingw32-ar`
- `x86_64-w64-mingw32-ranlib`
- `x86_64-w64-mingw32-windres`
- `x86_64-w64-mingw32-strip`

On Ubuntu / Debian:

```bash
sudo apt install -y gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64
```

The release profile applies `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, and `strip = "symbols"`, then runs `x86_64-w64-mingw32-strip` after the build to keep the Windows GNU binary small.

Clean build:

```bash
./scripts/build-rust-win-clean.sh
```

Or from PowerShell:

```powershell
.\scripts\build-rust-win-clean.ps1
```

Artifacts:

`rust/target/x86_64-pc-windows-gnu/release/FlistWalker.exe`

## macOS Build

Normal build:

```bash
./scripts/build-rust-macos.sh
```

Clean build:

```bash
./scripts/build-rust-macos-clean.sh
```

Artifact:

`rust/target/release/flistwalker`

## License and Release Notices

- `LICENSE`
- `THIRD_PARTY_NOTICES.txt`
- Release packaging rules: [docs/RELEASE.md](docs/RELEASE.md)
