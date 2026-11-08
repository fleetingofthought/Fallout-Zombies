"""Serial comparisons of two supplied builds, using the ordinary scenario dispatch."""
from __future__ import annotations

import copy
import json
import math
import re
import statistics
import time
import uuid
from contextlib import nullcontext
from datetime import datetime, timezone
from pathlib import Path

from . import battle, lib, metadata, telemetry

# Relative threshold, absolute floor, unit. All listed costs are lower-is-better.
THRESHOLDS = {
    "mean_ms": (0.03, 0.05, "ms"), "p50_ms": (0.03, 0.05, "ms"),
    "p95_ms": (0.10, 0.10, "ms"), "p99_ms": (0.10, 0.10, "ms"),
    "cpu_ms": (0.05, 0.20, "ms/frame"),
    **{f"above_{t}_per_1000": (0.10, 1.0, "events/1000 frames") for t in telemetry.THRESHOLDS},
}
SPAN_THRESHOLD = (0.10, 0.02, "ms/frame")
PROFILE = re.compile(r"profile: ([\d.]+) s, (\d+) frames, frame mean [\d.]+ ms(?:, cpu ([\d.]+) ms a frame)?; (.*)$")
STAMP = re.compile(r"(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z)")


def utc():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def launch_plan(repetitions=4, warmup=1):
    """ABBA repeats in pairs; an odd repetition count keeps a partial final block."""
    rows = [{"side": side, "warmup": True, "repeat": n}
            for n in range(1, warmup + 1) for side in ("base", "candidate")]
    counts = {"base": 0, "candidate": 0}
    for i in range(2 * repetitions):
        side = ("base", "candidate", "candidate", "base")[i % 4]
        counts[side] += 1
        rows.append({"side": side, "warmup": False, "repeat": counts[side]})
    return [{"ordinal": i, "output": f"run-{i:03d}", **r} for i, r in enumerate(rows, 1)]


def summary(values):
    values = [v for v in values if telemetry.finite(v)]
    if not values:
        return {"values": [], "count": 0, "median": None, "min": None, "max": None, "spread": None}
    return {"values": values, "count": len(values), "median": statistics.median(values),
            "min": min(values), "max": max(values), "spread": max(values) - min(values)}


def judge(base, candidate, threshold):
    b, c = summary(base), summary(candidate)
    relative, floor, unit = threshold
    row = {"base": b, "candidate": c, "relative_threshold": relative, "absolute_floor": floor,
           "unit": unit, "verdict": "unavailable", "delta": None, "change_percent": None, "limit": None}
    if b["count"] < 3 or c["count"] < 3:
        return row
    delta = c["median"] - b["median"]
    limit = max(relative * abs(b["median"]), b["spread"], c["spread"], floor)
    # Equality belongs to the noise interval, including ordinary floating-point roundoff.
    changed = abs(delta) > limit and not math.isclose(abs(delta), limit, rel_tol=1e-12, abs_tol=1e-12)
    row.update(delta=delta, limit=limit, change_percent=100 * delta / b["median"] if b["median"] else None,
               verdict=("regression" if delta > 0 else "improvement") if changed else "no change beyond noise")
    return row


def profile_values(lines, capture, enabled):
    """Weight complete windows by frames, retaining missing top-list spans as unknown."""
    result = {"cpu_ms": None, "spans": {}, "windows": 0, "frames": 0,
              "unavailable_reason": "profile aid not enabled", "boundary_guard_seconds": 0.1}
    selected = capture.get("window", {})
    epoch = (capture.get("header") or {}).get("started_at_utc")
    if not enabled:
        return result
    if not selected.get("available") or not epoch:
        result["unavailable_reason"] = "capture window or epoch unavailable"
        return result
    start_epoch = datetime.fromisoformat(epoch.replace("Z", "+00:00")).timestamp()
    begin, end = selected["start_ns"] / 1e9, selected["end_ns"] / 1e9
    windows = []
    previous_end = None
    for line in lines:
        m, stamp = PROFILE.search(line), STAMP.search(line)
        if not m or not stamp:
            continue
        at = datetime.fromisoformat(stamp[1].replace("Z", "+00:00")).timestamp() - start_epoch
        # Printed durations are rounded to 0.1 s; adjacent report timestamps delimit
        # later windows more accurately. The first CPU baseline is unavailable anyway.
        since = previous_end
        previous_end = at
        if (since is None or since - 0.1 < begin or at + 0.1 > end or int(m[2]) <= 0
                or abs(at - since - float(m[1])) > 0.101):
            continue
        spans = {}
        if m[4].startswith("per frame ms: "):
            for part in m[4][len("per frame ms: "):].split(", "):
                name, sep, value = part.rpartition(" ")
                if sep:
                    try:
                        cost = float(value)
                    except ValueError:
                        continue
                    if telemetry.finite(cost) and cost >= 0:
                        spans[name] = cost
        windows.append((int(m[2]), float(m[3]) if m[3] else None, spans))
    total = sum(w[0] for w in windows)
    if not total:
        result["unavailable_reason"] = "no complete profile windows inside measurement interval"
        return result
    common = set.intersection(*(set(w[2]) for w in windows))
    result.update(windows=len(windows), frames=total, unavailable_reason=None,
                  cpu_ms=sum(n * cpu for n, cpu, _ in windows) / total if all(w[1] is not None for w in windows) else None,
                  spans={name: sum(n * spans[name] for n, _, spans in windows) / total for name in sorted(common)})
    return result


def _engine_conditions(header):
    keys = ("adapter", "backend", "driver", "driver_info", "width_px", "height_px", "display_sync", "cap_fps",
            "refresh_millihz", "window_present_mode", "build_features", "diagnostics")
    return {key: header.get(key) for key in keys}


def scenario_failures(args, result, lines):
    """A successful process exit alone does not establish scenario coverage."""
    if args.test in ("tour", "soak"):
        return ([] if result.get("laps_done") == args.laps else
                [f"scenario completed {result.get('laps_done', 0)} of {args.laps} requested laps"])
    if args.test != "battle":
        return []
    asked = [args.per_side, args.per_side] + ([args.third] if args.third else [])
    starts = [m for line in lines if (m := battle.STARTED.search(line))]
    placed = [m for line in lines if (m := battle.PLACED.search(line))]
    if (len(starts) != 1 or int(starts[0][1]) != len(asked) or int(starts[0][2]) != sum(asked)
            or len(placed) != len(asked)
            or sorted((int(m[1]), int(m[4]), int(m[5])) for m in placed)
            != [(i, count, count) for i, count in enumerate(asked, 1)]):
        return ["battle start and placement logs do not confirm every requested fighter"]
    return []


def build_index(plan, rows, test, labels, options, complete, artifact_checks=None, problems=None):
    """Keep every row while aggregating only eligible, comparable measurement windows."""
    problems = list(problems or [])
    measured = [r for r in rows if not r["warmup"]]
    headers = [r["telemetry"]["header"] for r in measured if r.get("included")]
    if headers and any(_engine_conditions(h) != _engine_conditions(headers[0]) for h in headers[1:]):
        problems.append("effective renderer, presentation, features or instrumentation differ between launches")
    scenario_files = [r.get("manifest", {}).get("scenario_files", {}) for r in measured]
    identities = [{name: info["sha256"] for name, info in files.items()} for files in scenario_files]
    if identities and any(value != identities[0] for value in identities[1:]):
        problems.append("scenario route or event schedule differs between launches")
    required = [k for k in THRESHOLDS if k != "cpu_ms"]
    if options.get("profile"):
        required.append("cpu_ms")
    span_names = sorted({k for r in measured for k in r.get("metrics", {}) if k.startswith("span:")})
    metrics = {}
    for key in required + span_names:
        values = {side: [r.get("metrics", {}).get(key) for r in measured if r["side"] == side and r.get("included")]
                  for side in ("base", "candidate")}
        metrics[key] = judge(values["base"], values["candidate"], SPAN_THRESHOLD if key.startswith("span:") else THRESHOLDS[key])
        metrics[key]["excluded"] = {side: sum(r["side"] == side for r in measured) - metrics[key][side]["count"]
                                    for side in ("base", "candidate")}
    if any(r.get("status") != "ok" or r.get("run_bad") for r in rows):
        problems.append("a launch failed, hung, crashed, failed checks or exceeded its legacy budget")
    verdicts = {m["verdict"] for m in metrics.values()}
    if not complete or problems or any(metrics[k]["verdict"] == "unavailable" for k in required):
        overall = "inconclusive"
    elif "regression" in verdicts:
        overall = "mixed" if "improvement" in verdicts else "regression"
    elif "improvement" in verdicts:
        overall = "mixed"
    else:
        overall = "no change"
    return {"schema_version": 2, "test": test, "status": "complete" if complete else "incomplete", "order": "ABBA",
            "labels": labels, "options": options, "plan": plan, "runs": rows, "metrics": metrics, "verdict": overall,
            "problems": problems, "artifact_checks": artifact_checks or {},
            "population": "settled early gameplay window; long-run coarse metrics retain their original populations",
            "p99_label": "median of per-run p99", "pooled_distribution": "not computed; raw captures retained"}


def _cell(value):
    return str(value).replace("|", "\\|").replace("\n", " ") if value is not None else "unavailable"


def report(index):
    out = [f"# Stress comparison: {index['test']}", "", f"Verdict: {index['verdict']}.", "",
           f"Order: ABBA; plan {index['status']}; labels {_cell(index['labels'])}.", "",
           index["population"] + ".", "", "The p99 summary is the median of per-run p99. No pooled percentile is computed.", "",
           f"Measurement options: {_cell(index['options'])}.", "",
           "| launch | side | repeat | warm-up | started UTC | status | capture | included | reasons |",
           "|---|---|---:|---|---|---|---|---|---|"]
    for r in index["runs"]:
        out.append("| " + " | ".join(_cell(v) for v in (r["output"], r["side"], r["repeat"], r["warmup"], r["started_at_utc"],
                   r["status"], r["telemetry"]["outcome"], r["included"], "; ".join(r["exclusions"]))) + " |")
    out += ["", "| metric | base median [min, max]; spread | candidate median [min, max]; spread | delta | limit | relative threshold | absolute floor | verdict |",
            "|---|---|---|---:|---:|---:|---:|---|"]
    for key, m in index["metrics"].items():
        sides = [f"{m[s]['median']} [{m[s]['min']}, {m[s]['max']}]; {m[s]['spread']}" if m[s]["count"] else "unavailable"
                 for s in ("base", "candidate")]
        out.append("| " + " | ".join(_cell(v) for v in (key, *sides, m["delta"], m["limit"], m["relative_threshold"],
                                                       m["absolute_floor"], m["verdict"])) + " |")
    out += ["", "| metric | base per-run values | candidate per-run values | excluded base / candidate |",
            "|---|---|---|---|"]
    for key, m in index["metrics"].items():
        out.append("| " + " | ".join(_cell(v) for v in
                   (key, m["base"]["values"], m["candidate"]["values"],
                    f"{m['excluded']['base']} / {m['excluded']['candidate']}")) + " |")
    for r in index["runs"]:
        out += ["", f"## {r['output']}: {r['side']}", "", "| value | recorded value |", "|---|---|"]
        details = {"started_at_utc": r["started_at_utc"], "finished_at_utc": r["finished_at_utc"], "elapsed_seconds": r["elapsed_seconds"],
                   "metrics": r["metrics"], "legacy coarse metrics": r["legacy_frames"], "profile": r["profile"],
                   "capture hash": r["telemetry"].get("sha256"), "capture phases": r["telemetry"].get("phases"),
                   "measurement window and transitions": r["telemetry"].get("window"), "manifest": r["manifest"]}
        out.extend(f"| {_cell(k)} | {_cell(json.dumps(v, ensure_ascii=True))} |" for k, v in details.items())
    out += ["", "## Sequence checks", "", _cell(index["problems"]), "", _cell(index["artifact_checks"])]
    return out


def write(out, index):
    for name, body in (("comparison.json", json.dumps(index, indent=1, allow_nan=False) + "\n"),
                       ("comparison.md", "\n".join(report(index)) + "\n")):
        temporary = out / (name + ".tmp")
        temporary.write_text(body, encoding="utf-8")
        temporary.replace(out / name)


def run(a, ctx, run_once, quality):
    """Own the existing locks once; child contexts must not reacquire them."""
    paths = {"base": Path(a.exe_base).resolve(), "candidate": Path(a.exe_candidate).resolve()}
    if paths["base"] == paths["candidate"]:
        raise SystemExit("base and candidate must use different executable paths")
    labels = {"base": a.base_label, "candidate": a.candidate_label}
    options = {key: getattr(a, key) for key in ("repetitions", "order", "warmup", "settle_seconds", "measure_seconds", "telemetry_seconds")}
    requested_aids = dict(kv.split("=", 1) for kv in a.aid)
    profile = requested_aids.get("NVF_PLAY_TEST_PROFILE", "0")
    try:
        profile = float(profile)
        if not math.isfinite(profile) or profile < 0:
            raise ValueError
        options["profile"] = profile > 0
    except ValueError:
        raise SystemExit("profile interval must be finite and nonnegative") from None
    plan = launch_plan(a.repetitions, a.warmup)
    rows, problems = [], []
    before = {side: metadata.file_info(path) for side, path in paths.items()}
    ini_before = metadata.file_info(ctx.ini) if ctx.ini else None
    if any(not v["available"] or v["unstable"] for v in before.values()):
        raise SystemExit("comparison executables must be readable and stable")
    token = uuid.uuid4().hex
    write(ctx.out, build_index(plan, rows, a.test, labels, options, False))
    try:
        with ctx.locks or nullcontext():
            for item in plan:
                child = lib.Ctx(ctx.repo, paths[item["side"]], ctx.data, ctx.fnvtool,
                                lib.new_folder(ctx.out, item["output"]), ctx.say, None, ctx.masters, ctx.ini)
                args = copy.deepcopy(a)
                args.exe, args.build_label = child.exe, labels[item["side"]]
                run_id, scenario_id = f"r_{token}_{item['ordinal']}", f"s_{token}_{item['ordinal']}"
                aids = {**requested_aids, "NVF_PLAY_TEST_FRAME_TELEMETRY": f"{a.telemetry_seconds:g}",
                        "NVF_PLAY_TEST_FRAME_TELEMETRY_RUN_ID": run_id,
                        "NVF_PLAY_TEST_FRAME_TELEMETRY_SCENARIO_ID": scenario_id}
                args.aid = [f"{key}={value}" for key, value in aids.items()]
                started, clock = utc(), time.monotonic()
                ctx.say(f"compare {a.test}: {item['output']} {item['side']}" + (" warm-up" if item["warmup"] else " measured"))
                res, lines, bad = run_once(args, child)
                finished, elapsed = utc(), time.monotonic() - clock
                log = Path(res["log"])
                timing_source = "scenario invocation"
                exit_file = log.with_suffix(".exit.json")
                if exit_file.is_file():
                    timing = json.loads(exit_file.read_text(encoding="utf-8"))
                    if timing.get("started_at_utc") and timing.get("finished_at_utc"):
                        started, finished, elapsed = timing["started_at_utc"], timing["finished_at_utc"], timing["seconds"]
                        timing_source = "launch exit record"
                log_lines = [lib.clean(line) for line in log.read_text(encoding="utf-8", errors="replace").splitlines()]
                checks = scenario_failures(args, res, log_lines)
                res["comparison_checks"] = checks
                capture = telemetry.consume(log.with_suffix(".home"), log_lines, run_id, scenario_id,
                                            a.settle_seconds, a.measure_seconds)
                (child.out / "telemetry-validation.json").write_text(json.dumps(capture, indent=1) + "\n", encoding="utf-8")
                manifest = res.get("manifest", {})
                header = capture.get("header")
                if header and capture.get("header_valid"):
                    manifest["engine"] = {"source_revision": header["engine_revision"], "features": header["build_features"],
                                          "provenance": "telemetry header", "build_label": labels[item["side"]],
                                          "dirty": "unknown", "header": header}
                manifest["scenario_files"] = {name: metadata.file_info(child.out / name)
                                              for name in ("route.txt", "events.json") if (child.out / name).is_file()}
                manifest["load_order"] = {"requested_masters": ctx.masters, "effective": "unknown",
                                          "identity": "unknown", "reason": "no resolved load-order export"}
                home_ini = log.with_suffix(".home") / "NVForever.ini"
                manifest["home_ini"] = {"initial_requested": manifest.get("ini", {}).get("file", "engine defaults"),
                                        "final": metadata.file_info(home_ini)}
                executable = manifest.get("artifacts", {}).get("executable")
                if executable and (executable.get("sha256") != before[item["side"]]["sha256"] or executable.get("unstable")):
                    problems.append(f"{item['side']} executable changed before {item['output']}")
                manifest["cache_state"] = {"process": "fresh", "home": "fresh", "warmup_launches_per_side": a.warmup,
                                           "os_file_cache": "unknown", "driver_shader_cache": "unknown",
                                           "generated_cache": "unknown", "process_assets": "not shared between launches"}
                manifest["comparison_ids"] = {"run_id": run_id, "scenario_id": scenario_id, "test": a.test}
                prof = profile_values(log_lines, capture, options["profile"])
                selected = capture.get("window") or {}
                metrics = {**(selected.get("statistics") or {}), **(selected.get("rates") or {})}
                if options["profile"]:
                    metrics["cpu_ms"] = prof["cpu_ms"]
                    metrics.update({f"span:{name}": value for name, value in prof["spans"].items()})
                cpu_quality, coarse, reason = quality(res)
                exclusions = (["discarded warm-up"] if item["warmup"] else [])
                if not coarse:
                    exclusions.append(reason or "coarse metrics unavailable")
                if capture["outcome"] != "valid":
                    exclusions.extend(capture["reasons"])
                if bad:
                    exclusions.append("scenario or legacy budget failed")
                exclusions.extend(checks)
                row = {**item, "started_at_utc": started, "finished_at_utc": finished, "elapsed_seconds": elapsed, "timing_source": timing_source,
                       "status": res.get("status", "unknown"), "run_bad": bad + len(checks), "quality": cpu_quality,
                       "included": not exclusions, "coarse_included": coarse and not item["warmup"], "exclusions": exclusions,
                       "telemetry": capture, "metrics": metrics, "legacy_frames": res.get("frames"), "manifest": manifest,
                   "profile": prof}
                res.update(telemetry=capture, manifest=manifest, comparison=row)
                lib.write_results(child.out, res, lines + ["", f"Capture: {capture['outcome']}."])
                rows.append(row)
                write(ctx.out, build_index(plan, rows, a.test, labels, options, False))
    except KeyboardInterrupt:
        problems.append("sequence interrupted")
    finally:
        after = {side: metadata.file_info(path) for side, path in paths.items()}
        checks = {side: {"before": before[side], "after": after[side]} for side in paths}
        for side in paths:
            if before[side]["sha256"] != after[side]["sha256"] or not after[side]["available"] or after[side]["unstable"]:
                problems.append(f"{side} executable changed or became unavailable")
        if ctx.ini:
            ini_after = metadata.file_info(ctx.ini)
            checks["ini"] = {"before": ini_before, "after": ini_after}
            if not ini_after["available"] or ini_before["sha256"] != ini_after["sha256"] or ini_after["unstable"]:
                problems.append("requested INI changed or became unavailable")
        index = build_index(plan, rows, a.test, labels, options, len(rows) == len(plan), checks, problems)
        write(ctx.out, index)
    ctx.say(f"Comparison: {index['verdict']}; report: {ctx.out / 'comparison.md'}")
    return 0 if index["verdict"] == "no change" else 1
