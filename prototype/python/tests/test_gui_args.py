from fast_file_finder.gui import parse_gui_args


def test_parse_gui_args_defaults() -> None:
    args = parse_gui_args([])

    assert args.root == "."
    assert args.limit == 1000
    assert args.query == ""


def test_parse_gui_args_values() -> None:
    args = parse_gui_args(["--root", "./docs", "--limit", "25", "--query", "readme"])

    assert args.root == "./docs"
    assert args.limit == 25
    assert args.query == "readme"
