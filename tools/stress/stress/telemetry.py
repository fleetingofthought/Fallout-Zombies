"""Read and validate schema-1 frame captures without launching the engine."""
from __future__ import annotations

import hashlib
import json
import math
import re
from datetime import datetime
from pathlib import Path

PHASES = ("startup", "loading", "gameplay")
THRESHOLDS = (33, 50, 100, 250)
MAX_BYTES = 67_108_864
MAX_FRAMES = 100_000
ID = re.compile(r"[A-Za-z0-9_-]{1,64}")


class Invalid(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise Invalid(message)


def uint(value, bits=64):
    return type(value) is int and 0 <= value < 2 ** bits


def finite(value):
    return type(value) in (int, float) and math.isfinite(value)


def string(value, limit=1024):
    return isinstance(value, str) and len(value.encode("utf-8")) <= limit


def statistics(frames):
    """Match the exporter's integer-nanosecond populations and rank rules."""
    values = sorted(f["duration_ns"] for f in frames if f["duration_ns"] is not None)
    n = len(values)
    result = {"frames_written": len(frames), "samples": n}
    result.update({f"above_{t}_ms": sum(v > t * 1_000_000 for v in values) for t in THRESHOLDS})
    result.update({key: None for key in ("mean_ms", "worst_ms", "p50_ms", "p95_ms", "p99_ms", "slowest_1pct_mean_ms")})
    if values:
        result.update(mean_ms=float(sum(values)) / n / 1e6, worst_ms=values[-1] / 1e6,
                      slowest_1pct_mean_ms=float(sum(values[-max(1, n // 100):])) / max(1, n // 100) / 1e6)
        result.update({f"p{p}_ms": values[(p * n + 99) // 100 - 1] / 1e6 for p in (50, 95, 99)})
    return result


def rates(stats):
    n = stats["samples"]
    return {f"above_{t}_per_1000": stats[f"above_{t}_ms"] * 1000 / n if n else None for t in THRESHOLDS}


def window(frames, stop_ns, settle_seconds, measure_seconds):
    """Keep the first complete settled interval, with both cadence endpoints inside it."""
    settle = round(settle_seconds * 1e9)
    measure = round(measure_seconds * 1e9)
    transitions = []
    begin = None
    previous = None
    segments = []
    for frame in frames:
        if frame["phase"] == "gameplay":
            if previous is None or previous["phase"] != "gameplay":
                transitions.append(frame)
                begin = frame["t_ns"]
        elif begin is not None:
            segments.append((begin, frame["t_ns"]))
            begin = None
        previous = frame
    if begin is not None:
        segments.append((begin, stop_ns))
    transition_ids = {id(f) for f in transitions}
    for begin, end in segments:
        start = begin + settle
        until = start + measure
        if end < start:
            continue
        if end < until:
            continue
        selected = []
        previous = None
        for frame in frames:
            if (frame["phase"] == "gameplay" and previous is not None and previous["phase"] == "gameplay"
                    and start <= previous["t_ns"] <= frame["t_ns"] < until
                    and id(frame) not in transition_ids):
                selected.append(frame)
            previous = frame
        stats = statistics(selected)
        if not stats["samples"]:
            break
        return {"available": True, "start_ns": start, "end_ns": until, "seconds": measure_seconds,
                "statistics": stats, "rates": rates(stats), "excluded_frames": len(frames) - len(selected),
                "transitions": statistics(transitions)}
    return {"available": False, "reason": "capture does not cover an uninterrupted settle and measurement window",
            "statistics": None, "rates": None, "transitions": statistics(transitions)}


def _pairs(items):
    result = {}
    for key, value in items:
        require(key not in result, f"duplicate JSON key {key}")
        result[key] = value
    return result


def _json(line):
    def number(value):
        result = float(value)
        require(math.isfinite(result), "non-finite JSON number")
        return result
    return json.loads(line, object_pairs_hook=_pairs,
                      parse_float=number,
                      parse_constant=lambda value: require(False, f"non-finite JSON number {value}"))


def _header(h, path):
    require(uint(h["schema_version"], 32) and h["schema_version"] == 1, "unsupported schema version")
    require(h["seq"] == 0 and h["t_ns"] == 0, "header must have seq and time zero")
    require(isinstance(h["capture_id"], str) and ID.fullmatch(h["capture_id"]), "invalid capture ID")
    require(path.name == f"frame-telemetry-{h['capture_id']}.jsonl", "filename does not match capture ID")
    for key in ("run_id", "scenario_id"):
        value = h[key]
        require(value is None or isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_-]{0,64}", value), f"invalid {key}")
    missing = h["unavailable"]
    require(isinstance(missing, dict) and all(string(k) and string(v) and v for k, v in missing.items()), "invalid unavailable metadata")
    text = ("started_at_utc", "engine_revision", "adapter", "backend", "driver", "driver_info", "window_present_mode", "display_sync")
    numbers = ("width_px", "height_px", "cap_fps", "refresh_millihz")
    for key in text + numbers:
        value = h[key]
        require(value is None or (string(value) and bool(value) if key in text else uint(value, 32)), f"invalid header {key}")
        require((key in missing) == (value is None), f"inconsistent unavailable reason for {key}")
    require("diagnostics_reason" not in missing, "diagnostics_reason belongs outside unavailable")
    if h["started_at_utc"] is not None:
        require(h["started_at_utc"].endswith("Z"), "epoch must use UTC")
        datetime.fromisoformat(h["started_at_utc"].replace("Z", "+00:00"))
    require(h["display_sync"] in (None, "off", "vsync", "variable"), "invalid display sync")
    features = h["build_features"]
    require(isinstance(features, list) and all(string(v) for v in features), "invalid build features")
    require(features == sorted(set(features)), "build features must be sorted and unique")
    require(uint(h["max_duration_ns"]) and 0 < h["max_duration_ns"] <= 60_000_000_000, "invalid duration limit")
    require(h["max_frames"] == MAX_FRAMES and type(h["max_frames"]) is int, "invalid frame limit")
    require(h["max_bytes"] == MAX_BYTES and type(h["max_bytes"]) is int, "invalid byte limit")
    require(h["diagnostics"] in ("enabled", "disabled", "unavailable"), "invalid diagnostics mode")
    reason = {"enabled": None, "disabled": "not_requested", "unavailable": "no_renderer"}[h["diagnostics"]]
    require(h["diagnostics_reason"] == reason, "invalid diagnostics reason")
    require(h["gpu_origin"] == "unavailable_from_bevy_0_20_1", "unsupported GPU origin")
    require(h["render_cpu_origin"] == "source_frame_unavailable_from_bevy_0_20_1", "unsupported render CPU origin")


def _frame(f, previous):
    require(uint(f["main_frame"], 32), "invalid main frame")
    require(f["phase"] in PHASES, "invalid phase")
    value = f["duration_ns"]
    require(value is None or uint(value), "invalid frame duration")
    require(f["duration_reason"] == ("no_previous_time_update" if value is None else None), "invalid duration reason")
    require(value is not None or previous is None and f["main_frame"] == 0, "null duration after first update")
    markers = f["markers"]
    require(isinstance(markers, list) and len(markers) <= 8, "invalid marker count")
    for m in markers:
        require(isinstance(m, dict), "invalid marker")
        require(m["kind"] in ("phase_enter", "scenario_begin", "scenario_end", "waypoint", "event"), "invalid marker kind")
        require(string(m["id"], 64) and m["id"].isascii(), "invalid marker ID")
        require(uint(m["t_ns"]) and m["t_ns"] <= f["t_ns"], "invalid marker timestamp")
        if m["kind"] == "phase_enter":
            require(m["id"] == f["phase"], "phase marker disagrees with frame")
    if previous is None or previous["phase"] != f["phase"]:
        require(any(m["kind"] == "phase_enter" for m in markers), "missing phase entry marker")


def _diagnostic(d, h):
    require(h["diagnostics"] != "disabled", "diagnostic in frames-only capture")
    require(string(d["path"]) and d["path"].startswith("render/"), "invalid diagnostic path")
    require(d["unit"] == "ms", "invalid diagnostic unit")
    require(d["kind"] in ("render_cpu_elapsed", "gpu_elapsed"), "invalid diagnostic kind")
    require(uint(d["observation_t_ns"]) and d["observation_t_ns"] <= d["t_ns"], "invalid observation time")
    require(uint(d["observation_main_frame"], 32), "invalid observation frame")
    reasons = {"timestamp_queries_unsupported": "unavailable", "no_sample_observed": "unavailable",
               "non_finite_value": "invalid", "unit_mismatch": "invalid"}
    if d["status"] == "available":
        require(finite(d["value"]) and d["reason"] is None, "invalid available diagnostic")
    else:
        require(d["reason"] in reasons and d["status"] == reasons[d["reason"]] and d["value"] is None,
                "invalid diagnostic absence")
    suffix = "elapsed_gpu" if d["kind"] == "gpu_elapsed" else "elapsed_cpu"
    require(d["reason"] == "unit_mismatch" or d["path"].endswith(suffix), "diagnostic suffix mismatch")


def validate(path):
    """Validate records and recompute statistics before trusting the trailer."""
    path = Path(path)
    result = {"path": str(path), "sha256": None, "header": None, "trailer": None}
    frames = []
    try:
        require(path.stat().st_size <= MAX_BYTES, "capture exceeds byte limit")
        with path.open("rb") as source:
            result["sha256"] = hashlib.file_digest(source, "sha256").hexdigest()
        records = []
        with path.open("rb") as source:
            while raw := source.readline(16 * 1024 + 1):
                require(len(raw) <= 16 * 1024, "line exceeds 16 KiB")
                if not raw.endswith(b"\n"):
                    return {**result, "outcome": "truncated", "reasons": ["partial final line"]}, frames
                record = _json(raw.decode("utf-8"))
                require(isinstance(record, dict), "record must be an object")
                require(record.get("type") in ("header", "frame", "diagnostic", "trailer"), "unsupported record type")
                require(record["type"] not in ("frame", "diagnostic") or len(raw) - 1 <= 8192, "payload exceeds 8 KiB")
                records.append(record)
        require(records and records[0]["type"] == "header", "missing first header")
        require(sum(r["type"] == "header" for r in records) == 1, "duplicate header")
        h = result["header"] = records[0]
        _header(h, path)
        result["header_valid"] = True
        require(all(uint(r["seq"]) and uint(r["t_ns"]) for r in records), "invalid seq or timestamp")
        require(all(a["seq"] < b["seq"] and a["t_ns"] <= b["t_ns"] for a, b in zip(records, records[1:])),
                "records are out of order")
        require(sum(r["type"] == "trailer" for r in records) <= 1, "duplicate trailer")
        require(not any(r["type"] == "trailer" for r in records[:-1]), "trailer is not last")
        for r in records[1:]:
            if r["type"] == "frame":
                _frame(r, frames[-1] if frames else None)
                frames.append(r)
            elif r["type"] == "diagnostic":
                _diagnostic(r, h)
        if records[-1]["type"] != "trailer":
            return {**result, "outcome": "truncated", "reasons": ["missing trailer"]}, frames
        t = result["trailer"] = records[-1]
        counters = ("frames_attempted", "frames_written", "dropped_frames", "dropped_diagnostics", "dropped_markers",
                    "gpu_observations_written")
        require(all(uint(t[k]) for k in counters), "invalid trailer counters")
        require(t["status"] in ("complete", "truncated"), "invalid trailer status")
        require(t["stop_reason"] in ("duration_limit", "frame_limit", "byte_limit", "normal_exit", "settings_changed",
                                      "writer_error", "shutdown_timeout"), "invalid stop reason")
        cut = t["cut_seq"]
        require(cut is None or uint(cut), "invalid cut seq")
        require(cut is None or cut == records[-2]["seq"], "cut does not name last written payload")
        require(t["stop_reason"] != "byte_limit" or cut is not None, "byte limit has no cut")
        require(cut is None or t["t_ns"] == records[-2]["t_ns"], "byte cut timestamp disagrees with last payload")
        require(t["frames_attempted"] == len(frames) + t["dropped_frames"] <= MAX_FRAMES, "attempted frame count mismatch")
        require(t["frames_written"] == len(frames), "written frame count mismatch")
        interval_end = t["seq"] - 1 if cut is None else cut
        require(interval_end - len(records[1:-1]) == t["dropped_frames"] + t["dropped_diagnostics"], "seq gap count mismatch")
        gpu = sum(r["type"] == "diagnostic" and r["kind"] == "gpu_elapsed" and r["status"] == "available" for r in records)
        require(t["gpu_observations_written"] == gpu, "GPU observation count mismatch")
        require(isinstance(t["phases"], dict) and set(t["phases"]) == set(PHASES), "invalid phases")
        for phase in PHASES:
            expected = statistics([f for f in frames if f["phase"] == phase])
            actual = t["phases"][phase]
            for key, value in expected.items():
                got = actual[key]
                if type(value) is int:
                    require(uint(got) and got == value, f"{phase}.{key} count mismatch")
                else:
                    require(got is None if value is None else finite(got) and abs(got - value) <= 0.000001,
                            f"{phase}.{key} statistic mismatch")
        lossy = any(t[k] for k in ("dropped_frames", "dropped_diagnostics", "dropped_markers"))
        complete = not lossy and t["stop_reason"] in ("duration_limit", "frame_limit", "byte_limit", "normal_exit")
        require(not complete or cut is None or t["stop_reason"] == "byte_limit", "complete cut without byte-limit stop")
        require((t["status"] == "complete") == complete, "trailer completion contradicts counters or stop reason")
        result.update(outcome="valid" if complete else "truncated", reasons=[] if complete else ["exporter loss or abnormal stop"])
    except (OSError, UnicodeError, ValueError, KeyError, TypeError, OverflowError, RecursionError) as error:
        result.update(outcome="invalid", reasons=[str(error)])
    return result, frames


def _same_path(value, path, home):
    p = Path(value)
    if not p.is_absolute():
        p = home.parent / p
    return p.resolve() == path.resolve()


def consume(home, lines, run_id, scenario_id, settle_seconds=10, measure_seconds=30, requested=True):
    """Match one launch's file and both log lines, then select its gameplay window."""
    home = Path(home)
    paths = sorted((home / "logs").glob("frame-telemetry-*.jsonl"))
    base = {"outcome": "missing", "reasons": ["no matching capture"], "candidates": [str(p) for p in paths],
            "percentiles_available": False, "requested": requested}
    if not requested:
        base["reasons"] = ["capture not requested; only coarse metrics are available"]
    matches, malformed = [], []
    for p in paths:
        try:
            with p.open("rb") as source:
                h = _json(source.readline(16 * 1024 + 1).decode("utf-8"))
            if isinstance(h, dict) and h.get("run_id") == run_id and h.get("scenario_id") == scenario_id:
                matches.append(p)
        except (OSError, ValueError, UnicodeError, RecursionError):
            malformed.append(p)
    if len(matches) > 1:
        return {**base, "outcome": "ambiguous", "reasons": ["multiple captures match launch IDs"]}
    if not matches:
        if malformed:
            return {**base, "outcome": "invalid", "reasons": ["unreadable capture header"]}
        return base
    p = matches[0]
    result, frames = validate(p)
    result.update(candidates=base["candidates"], requested=requested, percentiles_available=False)
    if result["outcome"] != "valid":
        return result
    starts, summaries = [], []
    uncertain = False
    for line in lines:
        if m := re.search(r"frame telemetry: (.+) (?:trailer in progress, final status unavailable|truncated \([^)]*\), trailer unavailable.*)$", line):
            uncertain |= _same_path(m[1], p, home)
        if m := re.search(r"frame telemetry: capturing ([\d.]+) s to (.+) \(frames(?: \+ render timings)?\)$", line):
            if _same_path(m[2], p, home):
                starts.append(m)
        if m := re.search(r"frame telemetry: (.+) (complete|truncated) \((\w+)\), (\d+) frames, "
                          r"gameplay p50 (\S+) ms p95 (\S+) ms p99 (\S+) ms, dropped (\d+), (\d+) bytes(.*)$", line):
            if _same_path(m[1], p, home):
                summaries.append(m)
    t = result["trailer"]
    if uncertain or len(starts) != 1 or len(summaries) != 1:
        return {**result, "outcome": "flush_unconfirmed", "reasons": ["matching start or terminal summary unavailable or ambiguous"]}
    m = summaries[0]
    if "flush unconfirmed" in m[10] or "writer error:" in m[10]:
        return {**result, "outcome": "flush_unconfirmed", "reasons": ["terminal summary does not confirm final flush"]}
    quantiles = t["phases"]["gameplay"]
    gpu = re.search(r", gpu observations (\d+)", m[10])
    if (result["header"]["diagnostics"] != "disabled" and (gpu is None or int(gpu[1]) != t["gpu_observations_written"])):
        return {**result, "outcome": "invalid", "reasons": ["GPU summary count disagrees with capture"]}
    numbers_match = all(m[i] == ("none" if quantiles[key] is None else f"{quantiles[key]:.3f}")
                        for i, key in ((5, "p50_ms"), (6, "p95_ms"), (7, "p99_ms")))
    if not (m[2] == t["status"] and m[3] == t["stop_reason"] and int(m[4]) == t["frames_written"]
            and int(m[8]) == 0 and int(m[9]) == p.stat().st_size and numbers_match
            and abs(float(starts[0][1]) * 1e9 - result["header"]["max_duration_ns"]) <= 1):
        return {**result, "outcome": "invalid", "reasons": ["capture and log summary disagree"]}
    selected = window(frames, t["t_ns"], settle_seconds, measure_seconds)
    result.update(window=selected, phases=t["phases"], gameplay_rates=rates(quantiles))
    if not selected["available"]:
        result.update(outcome="insufficient_window", reasons=[selected["reason"]])
    else:
        result["percentiles_available"] = True
    return result
