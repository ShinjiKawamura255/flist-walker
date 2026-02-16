from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

from fast_file_finder.actions import choose_action, execute_or_open
from fast_file_finder.indexer import IndexBuildResult, build_index_with_metadata, write_filelist
from fast_file_finder.search import search_entries
from fast_file_finder.ui_model import build_preview_text, format_result_html, has_visible_match


class GuiDependencyError(RuntimeError):
    pass


def _load_qt():
    try:
        from PySide6.QtCore import QTimer, Qt
        from PySide6.QtGui import QKeySequence, QShortcut
        from PySide6.QtWidgets import (
            QAbstractItemView,
            QApplication,
            QCheckBox,
            QFileDialog,
            QHBoxLayout,
            QLabel,
            QLineEdit,
            QListWidget,
            QListWidgetItem,
            QMainWindow,
            QMessageBox,
            QPushButton,
            QSizePolicy,
            QSplitter,
            QTextEdit,
            QVBoxLayout,
            QWidget,
        )
    except Exception as exc:  # pragma: no cover
        raise GuiDependencyError(
            "PySide6 is required for GUI mode. Install with: pip install -e .[gui]"
        ) from exc

    return {
        "QTimer": QTimer,
        "Qt": Qt,
        "QKeySequence": QKeySequence,
        "QShortcut": QShortcut,
        "QApplication": QApplication,
        "QAbstractItemView": QAbstractItemView,
        "QCheckBox": QCheckBox,
        "QFileDialog": QFileDialog,
        "QHBoxLayout": QHBoxLayout,
        "QLabel": QLabel,
        "QLineEdit": QLineEdit,
        "QListWidget": QListWidget,
        "QListWidgetItem": QListWidgetItem,
        "QMainWindow": QMainWindow,
        "QMessageBox": QMessageBox,
        "QPushButton": QPushButton,
        "QSizePolicy": QSizePolicy,
        "QSplitter": QSplitter,
        "QTextEdit": QTextEdit,
        "QVBoxLayout": QVBoxLayout,
        "QWidget": QWidget,
    }


def parse_gui_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="FastFileFinder GUI prototype")
    parser.add_argument("--root", default=".", help="search root")
    parser.add_argument("--limit", type=int, default=1000, help="max result count (up to 1000)")
    parser.add_argument("--query", default="", help="initial query")
    return parser.parse_args(argv)


def _is_word_char(ch: str) -> bool:
    return ch.isalnum() or ch in {"_", "-"}


def run_gui(argv: list[str]) -> int:
    qt = _load_qt()
    args = parse_gui_args(argv)
    root = Path(args.root).resolve()

    if sys.platform.startswith("linux"):
        if not os.environ.get("DISPLAY") and not os.environ.get("WAYLAND_DISPLAY"):
            raise GuiDependencyError(
                "GUI cannot start: DISPLAY/WAYLAND_DISPLAY is not set in this environment."
            )

    QApplication = qt["QApplication"]
    QAbstractItemView = qt["QAbstractItemView"]
    QCheckBox = qt["QCheckBox"]
    QFileDialog = qt["QFileDialog"]
    QHBoxLayout = qt["QHBoxLayout"]
    QLabel = qt["QLabel"]
    QLineEdit = qt["QLineEdit"]
    QListWidget = qt["QListWidget"]
    QListWidgetItem = qt["QListWidgetItem"]
    QMainWindow = qt["QMainWindow"]
    QMessageBox = qt["QMessageBox"]
    QPushButton = qt["QPushButton"]
    QSizePolicy = qt["QSizePolicy"]
    QSplitter = qt["QSplitter"]
    QTextEdit = qt["QTextEdit"]
    QVBoxLayout = qt["QVBoxLayout"]
    QWidget = qt["QWidget"]
    QTimer = qt["QTimer"]
    Qt = qt["Qt"]
    QKeySequence = qt["QKeySequence"]
    QShortcut = qt["QShortcut"]

    app = QApplication(sys.argv)

    class EmacsLineEdit(QLineEdit):
        def keyPressEvent(self, event):  # type: ignore[override]
            key = event.key()
            mods = event.modifiers()

            if mods == Qt.KeyboardModifier.ControlModifier:
                if key == Qt.Key.Key_A:
                    self.home(False)
                    return
                if key == Qt.Key.Key_E:
                    self.end(False)
                    return
                if key == Qt.Key.Key_B:
                    self.cursorBackward(False, 1)
                    return
                if key == Qt.Key.Key_F:
                    self.cursorForward(False, 1)
                    return
                if key == Qt.Key.Key_H:
                    self.backspace()
                    return
                if key == Qt.Key.Key_D:
                    self.del_()
                    return
                if key == Qt.Key.Key_W:
                    if self.hasSelectedText():
                        self.cut()
                        return
                    text = self.text()
                    end = self.cursorPosition()
                    start = end
                    while start > 0 and text[start - 1].isspace():
                        start -= 1
                    while start > 0 and _is_word_char(text[start - 1]):
                        start -= 1
                    if start < end:
                        self.setSelection(start, end - start)
                        self.cut()
                    return
                if key == Qt.Key.Key_K:
                    start = self.cursorPosition()
                    length = len(self.text()) - start
                    if length > 0:
                        self.setSelection(start, length)
                        self.cut()
                    return
                if key == Qt.Key.Key_Y:
                    self.paste()
                    return
                if key == Qt.Key.Key_U:
                    pos = self.cursorPosition()
                    if pos > 0:
                        self.setSelection(0, pos)
                        self.del_()
                    return

            super().keyPressEvent(event)

    class MainWindow(QMainWindow):
        def __init__(self) -> None:
            super().__init__()
            self.setWindowTitle("FastFileFinder (Python GUI Prototype)")
            self.resize(1100, 700)

            self.root = root
            self.limit = max(1, min(args.limit, 1000))
            self.index = IndexBuildResult(entries=[], source="none")
            self.entries: list[Path] = []
            self.current_results: list[tuple[Path, float]] = []
            self.pinned_paths: set[Path] = set()

            root_widget = QWidget()
            self.setCentralWidget(root_widget)
            layout = QVBoxLayout(root_widget)

            root_row = QHBoxLayout()
            root_row.setContentsMargins(0, 0, 0, 0)
            root_row.setSpacing(8)
            root_row.addWidget(QLabel("Root:"))
            self.root_input = QLineEdit(str(self.root))
            self.root_input.setReadOnly(True)
            self.root_input.setPlaceholderText("Select search root directory")
            self.browse_button = QPushButton("Browse...")
            root_row.addWidget(self.root_input)
            root_row.addWidget(self.browse_button)
            layout.addLayout(root_row)

            option_bar = QWidget()
            option_bar.setMaximumHeight(30)
            option_row = QHBoxLayout(option_bar)
            option_row.setContentsMargins(0, 0, 0, 0)
            option_row.setSpacing(10)
            self.use_filelist_check = QCheckBox("Use FileList")
            self.use_filelist_check.setChecked(True)
            self.use_regex_check = QCheckBox("Regex")
            self.use_regex_check.setChecked(False)
            self.include_files_check = QCheckBox("Files")
            self.include_files_check.setChecked(True)
            self.include_dirs_check = QCheckBox("Folders")
            self.include_dirs_check.setChecked(True)
            option_row.addWidget(self.use_filelist_check)
            option_row.addWidget(self.use_regex_check)
            option_row.addWidget(self.include_files_check)
            option_row.addWidget(self.include_dirs_check)
            option_row.addStretch(1)
            self.source_label = QLabel("Source: -")
            self.source_label.setSizePolicy(
                QSizePolicy.Policy.Preferred,
                QSizePolicy.Policy.Fixed,
            )
            option_row.addWidget(self.source_label)
            layout.addWidget(option_bar)

            self.query_input = EmacsLineEdit(args.query)
            self.query_input.setPlaceholderText("Type to fuzzy-search files/folders...")
            layout.addWidget(self.query_input)

            self.splitter = QSplitter()
            self.results_list = QListWidget()
            self.results_list.setSelectionMode(QAbstractItemView.SelectionMode.SingleSelection)
            self.preview = QTextEdit()
            self.preview.setReadOnly(True)
            self.splitter.addWidget(self.results_list)
            self.splitter.addWidget(self.preview)
            self.splitter.setSizes([650, 450])
            layout.addWidget(self.splitter)

            action_row = QHBoxLayout()
            self.primary_button = QPushButton("Open / Execute")
            self.copy_button = QPushButton("Copy Path(s)")
            self.clear_selected_button = QPushButton("Clear Selected")
            self.create_filelist_button = QPushButton("Create File List")
            self.refresh_button = QPushButton("Refresh Index")
            action_row.addWidget(self.primary_button)
            action_row.addWidget(self.copy_button)
            action_row.addWidget(self.clear_selected_button)
            action_row.addWidget(self.create_filelist_button)
            action_row.addWidget(self.refresh_button)
            action_row.addStretch(1)
            layout.addLayout(action_row)

            self.statusBar().showMessage("Initializing...")

            self.query_timer = QTimer(self)
            self.query_timer.setSingleShot(True)
            self.query_timer.setInterval(120)

            self.query_input.textChanged.connect(self._schedule_search)
            self.query_timer.timeout.connect(self._update_results)
            self.results_list.currentRowChanged.connect(self._on_row_changed)
            self.results_list.itemDoubleClicked.connect(self._execute_selected)
            self.primary_button.clicked.connect(self._execute_selected)
            self.copy_button.clicked.connect(self._copy_selected_paths)
            self.clear_selected_button.clicked.connect(self._clear_pinned_paths)
            self.create_filelist_button.clicked.connect(self._create_filelist)
            self.refresh_button.clicked.connect(self._refresh_index)
            self.browse_button.clicked.connect(self._browse_root)
            self.use_filelist_check.toggled.connect(self._refresh_index)
            self.use_regex_check.toggled.connect(self._schedule_search)
            self.include_files_check.toggled.connect(self._on_type_toggle_changed)
            self.include_dirs_check.toggled.connect(self._on_type_toggle_changed)
            self._install_shortcuts()

            self._refresh_index()
            self.query_input.setFocus()

        def _build_source_text(self) -> str:
            if self.index.source == "filelist" and self.index.filelist_path:
                return f"Source: FileList ({self.index.filelist_path.name})"
            if self.index.source == "walker":
                return "Source: Walker"
            return "Source: None"

        def _rebuild_index(self) -> None:
            self.index = build_index_with_metadata(
                self.root,
                use_filelist=self.use_filelist_check.isChecked(),
                include_files=self.include_files_check.isChecked(),
                include_dirs=self.include_dirs_check.isChecked(),
            )
            self.entries = self.index.entries
            self.source_label.setText(self._build_source_text())

        def _set_root(self, new_root: Path) -> None:
            self.root = new_root
            self.root_input.setText(str(new_root))
            self._rebuild_index()
            self._update_results()

        def _browse_root(self) -> None:
            selected = QFileDialog.getExistingDirectory(self, "Select Search Root", str(self.root))
            if not selected:
                return
            try:
                self._set_root(Path(selected).resolve())
                self.statusBar().showMessage(f"Root changed: {self.root}")
            except Exception as exc:
                QMessageBox.critical(self, "Indexing failed", str(exc))

        def _on_type_toggle_changed(self) -> None:
            if not self.include_files_check.isChecked() and not self.include_dirs_check.isChecked():
                sender = self.sender()
                if sender is self.include_files_check:
                    self.include_dirs_check.setChecked(True)
                else:
                    self.include_files_check.setChecked(True)
                return
            self._refresh_index()

        def _install_shortcuts(self) -> None:
            QShortcut(QKeySequence(Qt.Key.Key_Down), self, activated=lambda: self._move_selection(1))
            QShortcut(QKeySequence(Qt.Key.Key_Up), self, activated=lambda: self._move_selection(-1))
            QShortcut(QKeySequence("Ctrl+N"), self, activated=lambda: self._move_selection(1))
            QShortcut(QKeySequence("Ctrl+P"), self, activated=lambda: self._move_selection(-1))
            QShortcut(QKeySequence(Qt.Key.Key_Return), self, activated=self._execute_selected)
            QShortcut(QKeySequence(Qt.Key.Key_Enter), self, activated=self._execute_selected)
            QShortcut(QKeySequence("Ctrl+M"), self, activated=self._execute_selected)
            QShortcut(QKeySequence("Ctrl+J"), self, activated=self._execute_selected)
            QShortcut(QKeySequence(Qt.Key.Key_Tab), self, activated=lambda: self._toggle_selection_and_move(1))
            QShortcut(
                QKeySequence(Qt.Key.Key_Backtab),
                self,
                activated=lambda: self._toggle_selection_and_move(-1),
            )
            QShortcut(QKeySequence("Ctrl+Shift+C"), self, activated=self._copy_selected_paths)

        def _move_selection(self, delta: int) -> None:
            if not self.current_results:
                return
            row = self.results_list.currentRow()
            if row < 0:
                row = 0
            next_row = max(0, min(len(self.current_results) - 1, row + delta))
            self.results_list.setCurrentRow(next_row)

        def _toggle_selection_and_move(self, delta: int) -> None:
            row = self.results_list.currentRow()
            if row < 0:
                if self.current_results:
                    self.results_list.setCurrentRow(0)
                return
            path = self.current_results[row][0]
            if path in self.pinned_paths:
                self.pinned_paths.remove(path)
            else:
                self.pinned_paths.add(path)
            self._refresh_result_widgets()
            self._move_selection(delta)

        def _schedule_search(self) -> None:
            self.query_timer.start()

        def _row_html(self, path: Path, query: str, *, is_current: bool, is_pinned: bool) -> str:
            current_marker = (
                "<span style='color:#60a5fa;font-weight:700;'>▶</span>"
                if is_current
                else "<span style='color:#374151;'>·</span>"
            )
            pinned_marker = (
                "<span style='color:#f59e0b;font-weight:700;'>◆</span>"
                if is_pinned
                else "<span style='color:#374151;'>·</span>"
            )
            return f"{current_marker} {pinned_marker} {format_result_html(path, self.root, query)}"

        def _add_result_item(self, path: Path, query: str, *, is_current: bool, is_pinned: bool) -> None:
            item = QListWidgetItem()
            widget = QLabel()
            widget.setTextFormat(Qt.TextFormat.RichText)
            widget.setText(self._row_html(path, query, is_current=is_current, is_pinned=is_pinned))
            widget.setContentsMargins(6, 2, 6, 2)
            item.setSizeHint(widget.sizeHint())
            self.results_list.addItem(item)
            self.results_list.setItemWidget(item, widget)

        def _refresh_result_widgets(self) -> None:
            query = self.query_input.text().strip()
            current_row = self.results_list.currentRow()
            for row, (path, _score) in enumerate(self.current_results):
                item = self.results_list.item(row)
                widget = self.results_list.itemWidget(item)
                if widget is None:
                    continue
                widget.setText(
                    self._row_html(
                        path,
                        query,
                        is_current=(row == current_row),
                        is_pinned=(path in self.pinned_paths),
                    )
                )

        def _update_results(self) -> None:
            query = self.query_input.text().strip()
            if query:
                results = search_entries(
                    query,
                    self.entries,
                    self.limit,
                    use_regex=self.use_regex_check.isChecked(),
                )
                self.current_results = [
                    item for item in results if has_visible_match(item[0], self.root, query)
                ]
            else:
                self.current_results = [(path, 0.0) for path in self.entries[: self.limit]]

            self.results_list.clear()
            for path, _score in self.current_results:
                self._add_result_item(path, query, is_current=False, is_pinned=(path in self.pinned_paths))

            if self.current_results:
                self.results_list.setCurrentRow(0)
                self._refresh_result_widgets()
            clipped = len(self.current_results) >= self.limit
            clip_text = f" (limit {self.limit} reached)" if clipped else ""
            pinned_text = f" | Pinned: {len(self.pinned_paths)}" if self.pinned_paths else ""
            self.statusBar().showMessage(
                f"Entries: {len(self.entries)} | Results: {len(self.current_results)}{clip_text}{pinned_text}"
            )

        def _get_current_path(self) -> Path | None:
            row = self.results_list.currentRow()
            if row < 0 or row >= len(self.current_results):
                return None
            return self.current_results[row][0]

        def _selected_paths(self) -> list[Path]:
            if self.pinned_paths:
                return sorted(self.pinned_paths, key=str)
            current = self._get_current_path()
            return [current] if current is not None else []

        def _on_row_changed(self, *_args) -> None:
            self._refresh_result_widgets()
            self._update_preview()

        def _update_preview(self) -> None:
            path = self._get_current_path()
            if path is None:
                self.preview.setPlainText("")
                return
            self.preview.setPlainText(build_preview_text(path))

        def _execute_selected(self, *_args) -> None:
            paths = self._selected_paths()
            if not paths:
                return
            try:
                for path in paths:
                    execute_or_open(path)
                if len(paths) == 1:
                    action = choose_action(paths[0])
                    self.statusBar().showMessage(f"Action: {action} -> {paths[0]}")
                else:
                    self.statusBar().showMessage(f"Action: launched {len(paths)} items")
            except Exception as exc:
                QMessageBox.critical(self, "Action failed", str(exc))

        def _copy_selected_paths(self) -> None:
            paths = self._selected_paths()
            if not paths:
                return
            text = "\n".join(str(path) for path in paths)
            app.clipboard().setText(text)
            if len(paths) == 1:
                self.statusBar().showMessage(f"Copied path: {paths[0]}")
            else:
                self.statusBar().showMessage(f"Copied {len(paths)} paths to clipboard")

        def _clear_pinned_paths(self) -> None:
            self.pinned_paths.clear()
            self._refresh_result_widgets()
            self.statusBar().showMessage("Cleared pinned selections")

        def _create_filelist(self) -> None:
            try:
                snapshot = build_index_with_metadata(
                    self.root,
                    use_filelist=False,
                    include_files=self.include_files_check.isChecked(),
                    include_dirs=self.include_dirs_check.isChecked(),
                )
                filelist_path = write_filelist(self.root, snapshot.entries)
                self.statusBar().showMessage(f"Created {filelist_path.name}: {len(snapshot.entries)} entries")
                if self.use_filelist_check.isChecked():
                    self._refresh_index()
            except Exception as exc:
                QMessageBox.critical(self, "Create File List failed", str(exc))

        def _refresh_index(self) -> None:
            try:
                self._rebuild_index()
                self._update_results()
            except Exception as exc:
                QMessageBox.critical(self, "Indexing failed", str(exc))

    win = MainWindow()
    win.show()
    return app.exec()


def main() -> None:
    raise SystemExit(run_gui(sys.argv[1:]))
