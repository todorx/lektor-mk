from build_release import valid_version


def test_valid_version():
    assert valid_version("0.1.0") == "0.1.0"
    assert valid_version("1.2.3") == "1.2.3"


def test_invalid_version():
    for bad in ("0.1", "v0.1.0", "0.1.0.0", "abc", ""):
        try:
            valid_version(bad)
        except SystemExit:
            continue
        raise AssertionError(f"accepted {bad!r}")
