"""Synthetic tests for the stress provenance manifest (no game or game data needed)."""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from stress import metadata  # noqa: E402


class ManifestTest(unittest.TestCase):
    def git(self, sha="a" * 40, dirty=False):
        return mock.patch.object(metadata, "_runner_git", return_value={
            "sha": sha,
            "dirty": dirty,
            "unavailable_reason": None,
        })

    def test_collects_json_safe_metadata_for_supplied_files(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            exe = root / "runner.bin"
            tool = root / "fnvtool.bin"
            ini = root / "NVForever.ini"
            exe.write_bytes(b"engine")
            tool.write_bytes(b"tool")
            ini.write_bytes(b"[Settings]\nvsync=0\n")
            with self.git():
                manifest = metadata.collect_manifest(
                    root, exe, tool, ini, test="tour",
                    requested={"route": Path("route.txt"), "limits": (1, 2), "nan": float("nan")},
                    masters=["FalloutNV.esm", "DeadMoney.esm"], build_label="profile build",
                )

            self.assertEqual(manifest["schema_version"], 1)
            self.assertTrue(manifest["collected_at_utc"].endswith("Z"))
            self.assertEqual(manifest["runner"]["source"]["git"]["sha"], "a" * 40)
            self.assertEqual(manifest["artifacts"]["executable"]["sha256"], hashlib.sha256(b"engine").hexdigest())
            self.assertEqual(manifest["artifacts"]["fnvtool"]["size_bytes"], 4)
            self.assertEqual(manifest["ini"]["mode"], "requested")
            self.assertEqual(manifest["requested"]["limits"], [1, 2])
            self.assertEqual(manifest["requested"]["nan"], "unknown")
            self.assertEqual(manifest["masters"], ["FalloutNV.esm", "DeadMoney.esm"])
            self.assertEqual(manifest["engine"]["source_revision"], "unknown")
            json.dumps(manifest, allow_nan=False)

    def test_uses_engine_defaults_and_records_missing_artifacts(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            with self.git():
                manifest = metadata.collect_manifest(
                    root, root / "missing-engine", root / "missing-tool", None,
                    test="cells", requested={}, masters=[],
                )

            self.assertEqual(manifest["ini"], {"mode": "engine-defaults"})
            for artifact in manifest["artifacts"].values():
                self.assertFalse(artifact["available"])
                self.assertEqual(artifact["unavailable_reason"], "file does not exist")
                self.assertIsNone(artifact["sha256"])

    def test_hashes_a_shared_file_once_per_manifest(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            binary = root / "shared.bin"
            binary.write_bytes(b"shared")
            with self.git(), mock.patch.object(metadata, "_hash_stream", wraps=metadata._hash_stream) as hashed:
                manifest = metadata.collect_manifest(root, binary, binary, None, test="battle", requested={}, masters=[])

            self.assertEqual(hashed.call_count, 1)
            self.assertEqual(manifest["artifacts"]["executable"], manifest["artifacts"]["fnvtool"])

    def test_marks_a_file_unstable_when_it_changes_while_hashed(self):
        with tempfile.TemporaryDirectory() as td:
            path = Path(td) / "changing.bin"
            path.write_bytes(b"before")

            def hash_and_change(target):
                digest = metadata.hashlib.sha256(target.read_bytes()).hexdigest()
                target.write_bytes(b"after and larger")
                return digest

            with mock.patch.object(metadata, "_hash_stream", side_effect=hash_and_change):
                info = metadata._file_info(path, {})

            self.assertTrue(info["available"])
            self.assertTrue(info["unstable"])
            self.assertEqual(info["size_bytes"], len(b"before"))

    def test_git_failure_is_explicitly_unknown(self):
        with mock.patch.object(metadata.subprocess, "run", side_effect=OSError):
            git = metadata._runner_git(Path("not-a-repository"))

        self.assertEqual(git["sha"], "unknown")
        self.assertEqual(git["dirty"], "unknown")
        self.assertEqual(git["unavailable_reason"], "Git metadata could not be collected")

    def test_git_success_uses_sha_and_worktree_status_without_a_shell(self):
        completed = [
            subprocess.CompletedProcess(["git"], 0, "b" * 40 + "\n", ""),
            subprocess.CompletedProcess(["git"], 0, " M tools/stress/stress/lib.py\n", ""),
        ]
        with mock.patch.object(metadata.subprocess, "run", side_effect=completed) as run:
            git = metadata._runner_git(Path("repo"))

        self.assertEqual(git, {"sha": "b" * 40, "dirty": True, "unavailable_reason": None})
        self.assertEqual(run.call_args_list[0].args[0], ["git", "rev-parse", "HEAD"])
        self.assertNotIn("shell", run.call_args_list[0].kwargs)


if __name__ == "__main__":
    unittest.main()
