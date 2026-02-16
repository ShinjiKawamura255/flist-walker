# FastFileFinder

`fzf --walker` 風の高速ファジー検索ツールの Python 試作です。

## セットアップ

```bash
python -m venv .venv
source .venv/bin/activate
pip install -e .[dev]
```

## 使い方

```bash
fast-file-finder --root . --limit 20
fast-file-finder "main" --root .
```

- `FileList.txt` または `filelist.txt` がルート直下にある場合はそれを優先。
- なければ walker 方式で再帰走査。
- 検索結果の番号を選ぶと、ファイルは実行/オープン、フォルダはオープン。

## テスト

```bash
pytest
```
