from build_release import patch_version, set_cargo_version, set_manifest_version, valid_version


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


def _tmpfile(tmpdir, name, text):
    import pathlib
    p = pathlib.Path(tmpdir) / name
    p.write_text(text, encoding="utf-8")
    return p


def test_patch_version_keeps_quotes(tmpdir):
    import json
    from build_release import patch_version
    import re
    man = _tmpfile(tmpdir, "manifest.json",
                   '{\n  "version": "0.1.0",\n  "manifest_version": 3\n}\n')
    patch_version(man, "0.2.0",
                  re.compile(r'("version"\s*:\s*")[^"]+("?)'))
    doc = json.loads(man.read_text(encoding="utf-8"))
    assert doc["version"] == "0.2.0"


def test_patch_cargo_keeps_quotes(tmpdir):
    from build_release import patch_version
    import re
    cargo = _tmpfile(tmpdir, "Cargo.toml",
                     '[workspace.package]\nversion = "0.1.0"\n')
    patch_version(cargo, "0.2.0",
                  re.compile(r'(?m)(^version\s*=\s*")[^"]+("?)'))
    assert 'version = "0.2.0"' in cargo.read_text(encoding="utf-8")


def test_overlay_versions_replaces_in_zip(tmpdir):
    import pathlib
    import zipfile
    from build_release import overlay_versions
    bundle = pathlib.Path(str(tmpdir)) / "src.zip"
    with zipfile.ZipFile(bundle, "w") as z:
        z.writestr("extension/manifest.json", '{"version": "0.1.0"}')
        z.writestr("README.md", "hi")
    overlay_versions(bundle, {"extension/manifest.json": '{"version": "0.2.0"}'})
    with zipfile.ZipFile(bundle) as z:
        assert len(z.namelist()) == 2
        assert z.read("extension/manifest.json") == b'{"version": "0.2.0"}'
        assert z.read("README.md") == b"hi"
