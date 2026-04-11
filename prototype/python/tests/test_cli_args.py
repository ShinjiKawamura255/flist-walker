from fast_file_finder.cli import _limit_was_specified, parse_args


def test_parse_args_defaults() -> None:
    args = parse_args([])

    assert args.query == ""
    assert args.root == "."
    assert args.limit == 20
    assert args.gui is False


def test_parse_args_gui_mode() -> None:
    args = parse_args(["alpha", "--root", "./tmp", "--limit", "30", "--gui"])

    assert args.query == "alpha"
    assert args.root == "./tmp"
    assert args.limit == 30
    assert args.gui is True


def test_limit_was_specified() -> None:
    assert _limit_was_specified(["--gui", "--limit", "40"]) is True
    assert _limit_was_specified(["--gui", "abc"]) is False
