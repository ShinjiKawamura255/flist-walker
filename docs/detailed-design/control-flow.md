<a id="top"></a>

# Control Flow and Sequence

## 8. Control Flow and Sequence

### 8.1 GUI Startup, Index, Search, Preview

```mermaid
sequenceDiagram
  actor U as User
  participant Main as main.rs
  participant App as FlistWalkerApp
  participant Index as IndexWorker
  participant Search as SearchWorker
  participant Preview as PreviewWorker
  participant UI as egui frame

  U->>Main: launch args
  Main->>App: from_launch(root, limit, query, root_explicit)
  App->>Index: IndexRequest(request_id, tab_id, root, flags)
  Index-->>App: Started(source)
  Index-->>App: Batch(entries)
  App->>UI: render incremental results/status
  U->>UI: edit query
  App->>Search: SearchRequest(request_id, Arc entries, query)
  Search-->>App: SearchResponse(results or error)
  App->>UI: update base_results/results/current row
  App->>Preview: PreviewRequest(current path)
  Preview-->>App: PreviewResponse(text)
  App->>UI: render preview
  Index-->>App: Finished(source)
```

Key guarantees:

- Index batches can arrive while the user continues typing.
- Search request IDs prevent old responses from replacing newer query results.
- Preview runs asynchronously and can lag behind row movement without blocking it.

### 8.2 FileList Priority and Fallback

```mermaid
flowchart TD
  Start[Build index] --> Flags{include files or dirs?}
  Flags -->|no| None[IndexSource::None]
  Flags -->|yes| UseFileList{Use FileList?}
  UseFileList -->|no| Walker[Walk root]
  UseFileList -->|yes| Find[Find root FileList]
  Find -->|found| Parse[Stream parse FileList]
  Find -->|not found| Walker
  Parse --> Nested{Nested FileList entries?}
  Nested -->|yes| Override[Apply newer subtree overrides]
  Nested -->|no| Emit[Emit entries]
  Override --> Emit
  Walker --> Emit
```

The root FileList detection is intentionally limited to the root directory. Hierarchical expansion is driven by candidate entries, not arbitrary recursive discovery.

### 8.3 Create File List

```mermaid
sequenceDiagram
  actor U as User
  participant UI as RenderCommand
  participant App as FlistWalkerApp
  participant FL as FileListManager
  participant Index as IndexWorker
  participant Worker as FileListWorker
  participant Writer as filelist_writer

  U->>UI: Create File List
  UI->>App: command
  App->>FL: evaluate source/root/ancestor/overwrite state
  alt Source is FileList
    FL-->>App: request temporary Walker index for same tab
    App->>Index: IndexRequest(use_filelist=false, tab_id)
    Index-->>App: Finished(Walker snapshot)
    App->>FL: resume deferred-after-index creation
  end
  alt overwrite or ancestor propagation needs confirmation
    FL-->>UI: show confirmation dialog
    U->>UI: confirm / cancel
  end
  App->>Worker: FileListRequest(request_id, tab_id, root, entries, cancel)
  Worker->>Writer: write_filelist_cancellable(...)
  Writer-->>Worker: path/count or error/canceled
  Worker-->>App: Finished / Failed / Canceled
  App->>FL: correlate request_id + requested root
  alt current requested root
    FL-->>App: cleanup + optional reindex original tab
  else stale requested root
    FL-->>App: cleanup only
  end
```

The manager boundary exists because this flow combines UI confirmations, temporary indexing, file writes, ancestor propagation, cancellation, and tab routing. The worker owns filesystem work; the app/manager owns state cleanup and follow-up dispatch.

### 8.4 Action Execution

```mermaid
sequenceDiagram
  actor U as User
  participant UI as Render/Input
  participant App as FlistWalkerApp
  participant Worker as ActionWorker
  participant Act as actions.rs
  participant OS as Operating System

  U->>UI: Enter / Ctrl+J / action button
  UI->>App: action command
  App->>App: validate selected paths within current root
  App->>Worker: ActionRequest(paths, open_parent_for_files)
  Worker->>Act: execute_or_open(path)
  Act->>Act: choose_action(path)
  Act->>OS: Command/ShellExecute/open/xdg-open
  Worker-->>App: ActionResponse(notice)
  App-->>UI: show notice
```

Root containment is checked immediately before dispatch. This preserves FileList indexing speed while preventing root-external execution.

### 8.5 Self-update

```mermaid
sequenceDiagram
  participant App as UpdateManager
  participant Worker as UpdateWorker
  participant GH as GitHub Releases
  participant Sec as update_security
  participant Script as Staged updater

  App->>Worker: UpdateRequest::Check
  Worker->>GH: latest release
  GH-->>Worker: release + assets
  Worker-->>App: Available(candidate)
  App->>Worker: DownloadAndApply(candidate)
  Worker->>GH: download asset + sidecars + SHA256SUMS + sig
  Worker->>Sec: verify signature
  Worker->>Worker: verify checksums
  Worker->>Script: start platform updater
  Worker-->>App: ApplyStarted(target_version)
  App->>App: request close for install
```

If update support is manual-only, the GUI can present the release URL without launching replacement logic.

[[↑ Back to Top]](#top)
