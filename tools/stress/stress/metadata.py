"""Reproducibility metadata for a stress-test result.

The collector deliberately records only the runner checkout and files passed to the
runner.  It does not inspect game data or try to infer an engine revision from an
executable name.
"""
from __future__ import annotations

import hashlib
import json
import math
import os
import platform
import subprocess
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


UNKNOWN = "unknown"
_CHUNK_SIZE = 1024 * 1024


def _absolute(path: Path) -> str:
    """Return an absolute, resolved path without requiring that it exists."""
    try:
        return str(path.resolve(strict=False))
    except OSError:
        return str(path.absolute())


def _hash_stream(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(_CHUNK_SIZE):
            digest.update(chunk)
    return digest.hexdigest()


def _stat_signature(path: Path) -> tuple[int, int]:
    stat = path.stat()
    return stat.st_size, stat.st_mtime_ns


def _file_info(path: Path, cache: dict[str, dict[str, Any]]) -> dict[str, Any]:
    """Describe one supplied file, retaining failures as metadata rather than errors."""
    resolved = _absolute(path)
    if resolved in cache:
        return dict(cache[resolved])

    result: dict[str, Any] = {
        "path": resolved,
        "size_bytes": None,
        "sha256": None,
        "available": False,
        "unavailable_reason": None,
        "unstable": False,
    }
    try:
        before = _stat_signature(path)
    except FileNotFoundError:
        result["unavailable_reason"] = "file does not exist"
    except OSError:
        result["unavailable_reason"] = "file cannot be statted"
    else:
        result["size_bytes"] = before[0]
        if not path.is_file():
            result["unavailable_reason"] = "path is not a regular file"
        else:
            try:
                result["sha256"] = _hash_stream(path)
                after = _stat_signature(path)
            except OSError:
                result["unavailable_reason"] = "file cannot be read"
            else:
                result["available"] = True
                result["unstable"] = before != after

    cache[resolved] = dict(result)
    return result


def file_info(path: Path) -> dict[str, Any]:
    """Take a fresh file snapshot, including a hash and mutation detection."""
    return _file_info(Path(path), {})


def _runner_git(repo: Path) -> dict[str, Any]:
    """Read the runner checkout identity, without allowing Git failures to stop a run."""
    result: dict[str, Any] = {
        "sha": UNKNOWN,
        "dirty": UNKNOWN,
        "unavailable_reason": None,
    }
    try:
        sha = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repo,
            capture_output=True,
            text=True,
            check=False,
            timeout=5,
        )
        dirty = subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=repo,
            capture_output=True,
            text=True,
            check=False,
            timeout=5,
        )
    except (OSError, subprocess.TimeoutExpired):
        result["unavailable_reason"] = "Git metadata could not be collected"
        return result

    failures = []
    if sha.returncode == 0 and sha.stdout.strip():
        result["sha"] = sha.stdout.strip()
    else:
        failures.append("revision")
    if dirty.returncode == 0:
        result["dirty"] = bool(dirty.stdout.strip())
    else:
        failures.append("worktree status")
    if failures:
        result["unavailable_reason"] = "Git " + " and ".join(failures) + " could not be collected"
    return result


def _json_safe(value: Any) -> Any:
    """Normalize common runner option values without serializing arbitrary objects."""
    if value is None or isinstance(value, (str, bool, int)):
        return value
    if isinstance(value, float):
        return value if math.isfinite(value) else UNKNOWN
    if isinstance(value, Path):
        return _absolute(value)
    if isinstance(value, (list, tuple)):
        return [_json_safe(item) for item in value]
    if isinstance(value, dict):
        return {str(key): _json_safe(item) for key, item in value.items()}
    return {"unavailable_type": type(value).__name__}


def collect_manifest(
    repo: Path,
    exe: Path,
    fnvtool: Path,
    ini: Path | None,
    *,
    test: str,
    requested: dict,
    masters: list[str],
    build_label: str | None = None,
) -> dict:
    """Collect a version-1, JSON-safe stress-run provenance manifest.

    This function is intentionally best-effort: missing optional files and a checkout
    without Git metadata are recorded explicitly and do not prevent a stress run.
    """
    repo_path = Path(repo)
    cache: dict[str, dict[str, Any]] = {}
    manifest = {
        "schema_version": 1,
        "collected_at_utc": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "test": str(test),
        "runner": {"source": {"git": _runner_git(repo_path)}},
        "platform": {
            "system": platform.system() or UNKNOWN,
            "release": platform.release() or UNKNOWN,
            "machine": platform.machine() or UNKNOWN,
            "python": {
                "implementation": platform.python_implementation() or UNKNOWN,
                "version": platform.python_version() or UNKNOWN,
            },
            "cpu": {
                "logical_cores": os.cpu_count(),
                "processor": platform.processor() or UNKNOWN,
            },
            "graphics_driver": UNKNOWN,
            "graphics_backend": UNKNOWN,
            "effective_settings": UNKNOWN,
        },
        "engine": {
            "source_revision": UNKNOWN,
            "features": UNKNOWN,
            "provenance": UNKNOWN,
            "build_label": build_label,
        },
        "artifacts": {
            "executable": _file_info(Path(exe), cache),
            "fnvtool": _file_info(Path(fnvtool), cache),
        },
        "ini": {"mode": "engine-defaults"} if ini is None else {
            "mode": "requested",
            "file": _file_info(Path(ini), cache),
        },
        "requested": _json_safe(requested),
        "masters": [str(master) for master in masters],
    }
    # Keep the contract honest if this module gains a new value type in the future.
    json.dumps(manifest, allow_nan=False)
    return manifest
