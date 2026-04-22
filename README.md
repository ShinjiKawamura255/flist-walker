# FlistWalker

FlistWalker is a Rust GUI/CLI tool that provides an `fzf --walker`-style experience for fast fuzzy searching of files and folders, then opening or executing the selected result.

- Display name: `FlistWalker`
- GitHub repository: `flist-walker`
- CLI binary name: `flistwalker` (`FlistWalker.exe` on Windows release assets)

Language docs:

- Japanese: [README-ja.md](README-ja.md)
- Support: [docs/SUPPORT.md](docs/SUPPORT.md)

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
- Runtime config and session file support beside the executable on Windows, or via `~/.flistwalker_config.json` on Linux/macOS

## Quick Start

```bash
cd rust
source ~/.cargo/env
cargo run -- --root ..
```

1. Type in the search box to narrow candidates.
2. Press `Enter` to open or execute the selected item.
3. Press `Shift+Enter` to open the containing folder; identical folders are opened only once.
4. Press `Tab` / `Shift+Tab` to toggle pinned multi-selection.
5. Press `Ctrl+Shift+C` to copy the selected path (`Cmd+Shift+C` on macOS).
6. Press `Ctrl+R` to fuzzy-search query history, then `Enter` / `Ctrl+J` / `Ctrl+M` to load it into the search box.

### Main Shortcuts

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

Tab switching still uses `Ctrl+Tab` / `Ctrl+Shift+Tab` on macOS.

## Query History

- Search history is shared across tabs and persisted up to 100 entries.
- History is saved to a plain-text file beside the executable on Windows, or under the home directory on Linux/macOS. Avoid putting sensitive data in search terms or paths.
- Set `FLISTWALKER_DISABLE_HISTORY_PERSIST=1` to disable history load and save.
- Press `Ctrl+R` to enter history search mode and fuzzy-search the same query box.
- While in history search, `Enter` / `Ctrl+J` / `Ctrl+M` loads the selected history entry into the search box, and `Esc` / `Ctrl+G` cancels and restores the previous query.
- History is not written on every keystroke. It is committed after a short idle period or when result navigation starts.
- Intermediate IME composition text is not stored; only committed query text is kept.

## Session Restore

- Set `FLISTWALKER_RESTORE_TABS=1` to restore the previous tab state on the next launch.
- Restored fields include `root`, `query`, `Use FileList`, `Regex`, `Files`, `Folders`, and the active tab.
- If you explicitly pass `--root` or a startup query, those values take precedence over restore.
- When this feature is enabled, `Set as default` is disabled because the startup root is determined by tab restore.

## Runtime Configuration

- On Windows, runtime settings files are stored beside the executable. On Linux/macOS, they are stored under the home directory in `~/.flistwalker_config.json`.
- The runtime config file is created from the current `FLISTWALKER_*` environment values on first launch if it does not exist yet.
- Once the file exists, it becomes the source of truth for runtime settings and the matching environment variables are only an initial seed.
- The same Windows-vs-home placement also applies to UI state, saved roots, and window trace files.
- If you upgrade from an older Windows build, the first launch will automatically move legacy home-directory files into the new executable-side location when the new files do not already exist.
- Only the commonly useful toggles are documented here. Advanced keys are intentionally undocumented.
- The file is plain JSON, so you can edit it directly.
- If you delete the file, the next launch will recreate it from the current environment values.
- `walker_max_entries` is also exposed here because it affects large-root scans.

Example:

```json
{
  "walker_max_entries": 500000,
  "history_persist_disabled": false,
  "restore_tabs_enabled": false
}
```

- Keep values that should behave like booleans as `true` / `false`.

Windows PowerShell, persistent user environment variable:

```powershell
[Environment]::SetEnvironmentVariable("FLISTWALKER_RESTORE_TABS", "1", "User")
```

Windows PowerShell, current session only:

```powershell
$env:FLISTWALKER_RESTORE_TABS = "1"
```

Windows CMD, current session only:

```cmd
set FLISTWALKER_RESTORE_TABS=1
```

macOS, current session only:

```bash
export FLISTWALKER_RESTORE_TABS=1
```

macOS, persistent zsh setting:

```bash
echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.zshrc
```

Linux, current session only:

```bash
export FLISTWALKER_RESTORE_TABS=1
```

Linux, persistent bash setting:

```bash
echo 'export FLISTWALKER_RESTORE_TABS=1' >> ~/.bashrc
```

## Public Environment Variables

- Runtime settings are now config-file driven through the platform-appropriate settings files.
- On Windows, those settings files live beside the executable; on Linux/macOS, they live under `~/.flistwalker_config.json` and related home-directory files.
- The matching `FLISTWALKER_*` variables are only used to seed the config file when it does not exist yet.
- Signing and release build variables are documented only in [docs/RELEASE.md](docs/RELEASE.md).

## Rust App

```bash
cd rust
source ~/.cargo/env
cargo run -- --root ..
```

CLI mode:

```bash
cd rust
source ~/.cargo/env
cargo run -- --cli "main" --root .. --limit 1000
```

In CLI mode:

- If no query is provided, the tool prints up to `limit` candidates.
- If a query is provided, results are shown with scores.
- `--limit` is treated as a real upper bound, not rounded down to 1000.
- The current CLI does not have a `Regex` toggle like the GUI; it performs normal search only.

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
- Each token is applied like a search exclusion. For example, `old` and `~` behave like typing `!old !~`.
- You can also place multiple terms on one line, separated by spaces.
- The `Use Ignore List` checkbox controls whether these rules are applied. It is enabled by default.
- A sample file is available at [flistwalker.ignore.txt.example](flistwalker.ignore.txt.example).

### Root Actions

- `Browse...`: change root
- `Set as default`: save the current root for the next launch
- `Add to list`: add the current root to the saved roots list
- `Remove from list`: remove the current root from the saved roots list

## Testing

```bash
cd rust
source ~/.cargo/env
cargo test
```

## Support and Bug Reports

Use the GitHub Issues templates for bug reports and feature requests. Before filing an issue, read [docs/SUPPORT.md](docs/SUPPORT.md) and redact usernames, project names, full paths, tokens, and other sensitive data.

## Windows Build

From WSL or a Linux shell:

```bash
./scripts/build-rust-win.sh
```

This builds `x86_64-pc-windows-gnu` entirely from WSL/Linux.
Explorer icon embedding is also handled on the WSL side, so PowerShell and Windows-side Rust are not required.

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

The old `scripts/build-rust-win.ps1` / `scripts/build-rust-win-clean.ps1` scripts have been retired and now return an error that points to the WSL/Linux build flow.

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
