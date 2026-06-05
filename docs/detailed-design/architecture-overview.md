<a id="top"></a>

# Detailed Design Architecture Overview

## 5. Architecture Overview

FlistWalker is a single-process desktop/CLI application. The GUI path creates an eframe app and a set of background worker threads. The CLI path uses the same index and search modules synchronously.

```mermaid
flowchart LR
  User[User] --> Main[main.rs]
  Main -->|--cli| Cli[CLI adapter]
  Main -->|GUI| Gui[FlistWalkerApp]
  Cli --> Indexer[indexer]
  Cli --> Search[search]
  Gui --> Shell[AppShellState]
  Shell --> Runtime[AppRuntimeState]
  Shell --> Tabs[TabSessionState]
  Shell --> Features[FeatureStateBundle]
  Shell --> Cache[CacheStateBundle]
  Shell --> Bus[WorkerBus]
  Bus --> IndexWorker[Index worker]
  Bus --> SearchWorker[Search worker]
  Bus --> PreviewWorker[Preview worker]
  Bus --> ActionWorker[Action worker]
  Bus --> SortWorker[Sort worker]
  Bus --> FileListWorker[FileList worker]
  Bus --> UpdateWorker[Update worker]
  IndexWorker --> Indexer
  SearchWorker --> Search
  PreviewWorker --> UiModel[ui_model]
  ActionWorker --> Actions[actions]
  FileListWorker --> FileListWriter[indexer/filelist_writer]
  UpdateWorker --> Updater[updater + update_security]
```

The central design rule is that `FlistWalkerApp` remains an orchestration shell. State mutations should flow through owner modules and reducers rather than direct ad hoc field updates inside rendering code.

### Deployment View

```mermaid
flowchart TB
  subgraph Runtime[User machine]
    Binary[flistwalker binary]
    SettingsDir[platform-specific settings dir]
    UiState[.flistwalker_ui_state.json]
    Root[Search root]
    FileList[FileList.txt / filelist.txt]
    OsShell[OS file manager / default app]
  end
  subgraph GitHub[GitHub Releases]
    Latest[latest release API]
    Asset[platform asset]
    Sums[SHA256SUMS]
    Sig[SHA256SUMS.sig]
  end
  Binary --> SettingsDir
  Binary --> UiState
  Binary --> Root
  Root --> FileList
  Binary --> OsShell
  Binary -->|update check| Latest
  Latest --> Asset
  Latest --> Sums
  Latest --> Sig
```

There is no server-side component. GitHub Releases is only used for update discovery and asset download.

[[↑ Back to Top]](#top)
