import importlib.util
import os
from pathlib import Path
import socket
import sys
import tempfile
import unittest
from unittest import mock


MODULE_PATH = Path(__file__).with_name("profile.py")
SPEC = importlib.util.spec_from_file_location("typokat_library_profile", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
profile = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = profile
SPEC.loader.exec_module(profile)


def source(*references: str) -> bytes:
    directives = "".join(f'/// <reference lib="{name}" />\n' for name in references)
    return (directives + "interface Marker {}\n").encode()


class ReferenceGraphTests(unittest.TestCase):
    def test_resolves_ordered_closure(self):
        sources = {
            "lib.root.d.ts": source("a", "b"),
            "lib.a.d.ts": source("leaf"),
            "lib.b.d.ts": source(),
            "lib.leaf.d.ts": source(),
        }
        lib_map = {
            "a": "lib.a.d.ts",
            "b": "lib.b.d.ts",
            "leaf": "lib.leaf.d.ts",
        }
        loaded, graph = profile.resolve_closure("lib.root.d.ts", lib_map, sources.__getitem__)
        self.assertEqual(set(loaded), set(sources))
        self.assertEqual(
            graph["lib.root.d.ts"],
            (
                profile.ReferenceEdge("a", "lib.a.d.ts"),
                profile.ReferenceEdge("b", "lib.b.d.ts"),
            ),
        )

    def test_rejects_unknown_reference(self):
        with self.assertRaisesRegex(profile.ProfileError, "unknown reference-lib"):
            profile.resolve_closure(
                "lib.root.d.ts", {}, lambda _name: source("missing")
            )

    def test_rejects_missing_file(self):
        with self.assertRaisesRegex(profile.ProfileError, "missing referenced library"):
            profile.resolve_closure(
                "lib.root.d.ts", {"missing": "lib.missing.d.ts"},
                lambda name: source("missing") if name == "lib.root.d.ts" else (_ for _ in ()).throw(FileNotFoundError(name)),
            )

    def test_rejects_cycle(self):
        sources = {
            "lib.root.d.ts": source("a"),
            "lib.a.d.ts": source("root"),
        }
        with self.assertRaisesRegex(profile.ProfileError, "cyclic reference-lib graph"):
            profile.resolve_closure(
                "lib.root.d.ts",
                {"a": "lib.a.d.ts", "root": "lib.root.d.ts"},
                sources.__getitem__,
            )

    def test_rejects_duplicate_reference(self):
        duplicate = source("a", "a")
        with self.assertRaisesRegex(profile.ProfileError, "duplicate reference-lib edge"):
            profile.parse_reference_libs("lib.root.d.ts", duplicate)

    def test_rejects_malformed_reference(self):
        for malformed in (b'/// <reference lib="a">\n', b'/// <reference lib "a" />\n'):
            with self.subTest(malformed=malformed), self.assertRaisesRegex(
                profile.ProfileError, "malformed reference-lib"
            ):
                profile.parse_reference_libs("lib.root.d.ts", malformed)

    def test_rejects_manifest_file_outside_root_closure(self):
        sources = {
            profile.ROOT_FILE: source(),
            "lib.extra.d.ts": source(),
        }
        graph = {name: () for name in sources}
        records = tuple(
            profile.FileRecord(
                ordinal=ordinal,
                name=name,
                npm_path=f"lib/{name}",
                git_path=f"lib/{name}",
                git_blob="0" * 40,
                sha256="0" * 64,
                bytes=len(sources[name]),
                lf=1,
                cr=0,
                final_lf=True,
                source_kind="script",
                references=(),
            )
            for ordinal, name in enumerate(sources)
        )
        with self.assertRaisesRegex(profile.ProfileError, "outside the recursive root closure"):
            profile.validate_record_graph(records, tuple(sources), sources, graph)


class OrderingTests(unittest.TestCase):
    def test_uses_lib_name_priority_and_puts_root_last(self):
        entries = (
            ("alias", "lib.a.d.ts"),
            ("a", "lib.a.d.ts"),
            ("b", "lib.b.d.ts"),
        )
        ordered = profile.canonical_order(
            ("lib.b.d.ts", "lib.root.d.ts", "lib.a.d.ts"), entries, "lib.root.d.ts"
        )
        self.assertEqual(ordered, ("lib.a.d.ts", "lib.b.d.ts", "lib.root.d.ts"))

    def test_rejects_non_root_without_priority(self):
        with self.assertRaisesRegex(profile.ProfileError, "no getDefaultLibFilePriority"):
            profile.canonical_order(
                ("lib.unknown.d.ts", "lib.root.d.ts"), (), "lib.root.d.ts"
            )

    def test_parse_lib_entries_is_strict(self):
        javascript = b'''var libEntries = [\n  // ["fake", "lib.fake.d.ts"],\n  ["a", "lib.a.d.ts"],\n  ["b", "lib.b.d.ts"]\n];\nvar libs = libEntries.map((entry) => entry[0]);\n'''
        self.assertEqual(
            profile.parse_lib_entries(javascript),
            (("a", "lib.a.d.ts"), ("b", "lib.b.d.ts")),
        )
        with self.assertRaisesRegex(profile.ProfileError, "unparsed libEntries syntax"):
            profile.parse_lib_entries(javascript.replace(b"]\n];", b"], unexpected\n];"))

    def test_lib_entries_digest_rejects_same_cardinality_alias_mutation(self):
        entries = (
            ("a", "lib.a.d.ts"),
            ("alias", "lib.a.d.ts"),
            ("unused.one", "lib.unused.one.d.ts"),
            ("unused.two", "lib.unused.two.d.ts"),
        )
        expected_digest = profile.framed_registry_digest(
            (lib, filename.encode()) for lib, filename in entries
        )
        mutated = entries[:2] + (
            ("unused.one", "lib.unused.two.d.ts"),
            ("unused.two", "lib.unused.one.d.ts"),
        )
        self.assertEqual(len(entries), len(mutated))
        self.assertEqual(
            len({filename for _, filename in entries}),
            len({filename for _, filename in mutated}),
        )
        with mock.patch.multiple(
            profile,
            EXPECTED_LIB_ENTRY_COUNT=len(entries),
            EXPECTED_LIB_ENTRY_FILENAME_COUNT=len({filename for _, filename in entries}),
            EXPECTED_LIB_ENTRIES_FRAMED_SHA256=expected_digest,
        ):
            profile.validate_lib_entries_fingerprint(entries)
            with self.assertRaisesRegex(profile.ProfileError, "libEntries fingerprint drift"):
                profile.validate_lib_entries_fingerprint(mutated)


class OutputTests(unittest.TestCase):
    def test_tampered_bootstrap_stops_before_parse_or_tsc(self):
        canonical = {
            "bin/tsc": b"canonical launcher",
            "lib/tsc.js": b"canonical tsc shim",
            "lib/_tsc.js": b"canonical tsc implementation",
            "lib/typescript.js": b"canonical TypeScript API",
        }
        modes = dict(profile.BOOTSTRAP_FILES)

        class FakeGit:
            def blob(self, path, *, expected_mode="100644"):
                if expected_mode != modes[path]:
                    raise AssertionError(f"unexpected mode for {path}: {expected_mode}")
                return "0" * 40, canonical[path]

        for tampered_path in canonical:
            with self.subTest(path=tampered_path), tempfile.TemporaryDirectory() as raw:
                root = Path(raw)
                package = root / "package"
                (package / "bin").mkdir(parents=True)
                (package / "lib").mkdir()
                (package / "package.json").write_text(
                    '{"name":"typescript","version":"6.0.3"}', encoding="utf-8"
                )
                for path, contents in canonical.items():
                    package_path = package / path
                    package_path.write_bytes(
                        b"tampered" if path == tampered_path else contents
                    )
                (package / profile.LICENSE_NAME).write_bytes(b"license")
                (package / profile.NOTICE_NAME).write_bytes(b"notice")

                with (
                    mock.patch.object(profile, "GitSource", return_value=FakeGit()),
                    mock.patch.object(profile, "parse_lib_entries") as parse_entries,
                    mock.patch.object(profile, "tsc_default_library_order") as tsc_order,
                    self.assertRaisesRegex(profile.ProfileError, "differs from pinned Git"),
                ):
                    profile.generate_tree(package, root / "git", root / "output")

                parse_entries.assert_not_called()
                tsc_order.assert_not_called()

    def test_tree_comparison_rejects_tamper_missing_and_extra(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            expected = root / "expected"
            actual = root / "actual"
            expected.mkdir()
            actual.mkdir()
            (expected / "a").write_bytes(b"same")
            (actual / "a").write_bytes(b"same")
            profile.compare_trees(expected, actual)

            (actual / "a").write_bytes(b"tampered")
            with self.assertRaisesRegex(profile.ProfileError, r"changed=\['a'\]"):
                profile.compare_trees(expected, actual)

            (actual / "a").unlink()
            with self.assertRaisesRegex(profile.ProfileError, r"missing=\['a'\]"):
                profile.compare_trees(expected, actual)

            (actual / "a").write_bytes(b"same")
            (actual / "extra").write_bytes(b"extra")
            with self.assertRaisesRegex(profile.ProfileError, r"extra=\['extra'\]"):
                profile.compare_trees(expected, actual)

    def test_collect_tree_rejects_fifo_without_opening_it(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            os.mkfifo(root / "fifo")
            with self.assertRaisesRegex(profile.ProfileError, "non-regular file"):
                profile.collect_tree(root)

    @unittest.skipUnless(hasattr(socket, "AF_UNIX"), "Unix sockets are unavailable")
    def test_collect_tree_rejects_unix_socket_without_opening_it(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            socket_path = root / "socket"
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
                listener.bind(str(socket_path))
                with self.assertRaisesRegex(profile.ProfileError, "non-regular file"):
                    profile.collect_tree(root)

    def test_framing_is_name_and_length_sensitive(self):
        first = profile.framed_registry_digest((("ab", b"c"),))
        second = profile.framed_registry_digest((("a", b"bc"),))
        self.assertNotEqual(first, second)

    def test_unsafe_names_are_rejected(self):
        for name in ("../lib.es5.d.ts", "/lib.es5.d.ts", "lib.ES5.d.ts", "es5.d.ts"):
            with self.subTest(name=name), self.assertRaises(profile.ProfileError):
                profile.validate_lib_file_name(name)

    def test_profile_schema_rejects_unknown_keys(self):
        auxiliary = profile.AuxiliaryRecord(
            name="notice",
            npm_path="notice",
            git_path="notice",
            git_blob="0" * 40,
            sha256="0" * 64,
            bytes=0,
            lf=0,
            cr=0,
            final_lf=False,
        )
        record = profile.FileRecord(
            ordinal=0,
            name=profile.ROOT_FILE,
            npm_path=f"lib/{profile.ROOT_FILE}",
            git_path=f"lib/{profile.ROOT_FILE}",
            git_blob="0" * 40,
            sha256="0" * 64,
            bytes=0,
            lf=0,
            cr=0,
            final_lf=False,
            source_kind="script",
            references=(),
        )
        rendered = profile.render_profile(
            (record,), "0" * 64, "0" * 64, "0" * 64, (), auxiliary, auxiliary
        )
        profile.validate_profile_schema(rendered, 1)
        with_unknown = rendered.replace(b"\n[license]\n", b"\nunknown = true\n\n[license]\n")
        with self.assertRaisesRegex(profile.ProfileError, "unexpected or missing root keys"):
            profile.validate_profile_schema(with_unknown, 1)

    def test_regular_input_rejects_symlink(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            target = root / "target"
            target.write_bytes(b"data")
            link = root / "link"
            link.symlink_to(target)
            with self.assertRaisesRegex(profile.ProfileError, "regular non-symlink"):
                profile.require_regular_file(link, "test input")

    def test_safe_replace_requires_expected_basename(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            with self.assertRaisesRegex(profile.ProfileError, "unexpected basename"):
                profile.safe_replace(root / "staged", root / "wrong-name")

    def test_safe_replace_rejects_output_symlink_without_touching_victim(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            victim = root / "victim" / "typescript-6.0.3"
            victim.mkdir(parents=True)
            sentinel = victim / "profile.toml"
            sentinel.write_bytes(b"victim")
            output = root / "typescript-6.0.3"
            output.symlink_to(victim, target_is_directory=True)
            staged = root / "staged"
            staged.mkdir()

            with self.assertRaisesRegex(profile.ProfileError, "symlink in output path"):
                profile.safe_replace(staged, output)

            self.assertTrue(output.is_symlink())
            self.assertEqual(sentinel.read_bytes(), b"victim")

    def test_output_path_rejects_symlink_parent_without_touching_victim(self):
        with tempfile.TemporaryDirectory() as raw:
            root = Path(raw)
            victim_parent = root / "victim-parent"
            victim_parent.mkdir()
            sentinel = victim_parent / "sentinel"
            sentinel.write_bytes(b"victim")
            linked_parent = root / "linked-parent"
            linked_parent.symlink_to(victim_parent, target_is_directory=True)
            output = linked_parent / "typescript-6.0.3"

            with self.assertRaisesRegex(profile.ProfileError, "symlink in output path"):
                profile.validated_output_path(output)

            self.assertEqual(sentinel.read_bytes(), b"victim")

    def test_output_path_rejects_broken_symlink(self):
        with tempfile.TemporaryDirectory() as raw:
            output = Path(raw) / "typescript-6.0.3"
            output.symlink_to(Path(raw) / "missing", target_is_directory=True)
            with self.assertRaisesRegex(profile.ProfileError, "symlink in output path"):
                profile.validated_output_path(output)


if __name__ == "__main__":
    unittest.main()
