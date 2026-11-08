"""`python tools/stress run <test> [options]`: run a stress test and write
target/stress/<test>/<run>/results.json and report.md (docs/design/stress-tests.md).

`python tools/stress list` names the tests. Each test is a module with `run(...)` and
`report(results)`. `compare <test>` uses the same dispatch under one lock sequence.
EXTRA_TESTS supplies the parser and dispatch hooks for additional scenarios.
"""
from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path

from . import battle, cells, comparison, lib, markers, soak, tour
from .metadata import collect_manifest

TESTS = {
    "cells": "every interior cell of the masters loads without crashing (exteriors: tour)",
    "markers": "fast travel to every map marker: refusals, arrivals, crashes, missing files and script errors",
    "tour": "laps of the Mojave, the engine's counts and the process's memory sampled for leaks",
    "battle": "NCR against the Legion in Goodsprings: frame time, memory, combat measures",
    "soak": "the tour's lap with containers, doors and fights mixed in: hitches put down to their cause",
}

COMPARISON_TESTS = {"tour", "battle", "soak"}
# Additional tests supply configure(parser) and run(args, context). The latter returns
# the ordinary (results, report lines, failure count) tuple used by run_once.
EXTRA_TESTS = {}


def repetitions(value):
    n = positive_int(value)
    if n < 3:
        raise argparse.ArgumentTypeError("a comparison needs at least three repetitions per side")
    return n


def nonnegative_int(value):
    try:
        n = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be a nonnegative integer") from None
    if n < 0:
        raise argparse.ArgumentTypeError("must be a nonnegative integer")
    return n


def seconds(value):
    try:
        n = float(value)
    except ValueError:
        raise argparse.ArgumentTypeError("must be finite nonnegative seconds") from None
    if not math.isfinite(n) or n < 0:
        raise argparse.ArgumentTypeError("must be finite nonnegative seconds")
    return n


def paired_options(p):
    p.add_argument("--exe-base", type=Path, required=True)
    p.add_argument("--exe-candidate", type=Path, required=True)
    p.add_argument("--base-label", default="base")
    p.add_argument("--candidate-label", default="candidate")
    p.add_argument("--repetitions", type=repetitions, default=4)
    p.add_argument("--order", choices=["ABBA"], default="ABBA")
    p.add_argument("--warmup", type=nonnegative_int, default=1)
    p.add_argument("--settle-seconds", type=seconds, default=10.0)
    p.add_argument("--measure-seconds", type=seconds, default=30.0)
    p.add_argument("--telemetry-seconds", type=seconds, default=60.0)
    p.set_defaults(exe=None, build_label=None)

def common(p: argparse.ArgumentParser, comparing=False) -> None:
    if not comparing:
        p.add_argument("--exe", type=Path, default=None, help="nvforever (default NVF_EXE or <repo>/engine/target/release)")
    p.add_argument("--data", default=None, help="game Data folder (default NVF_DATA or FNVLAB_DATA_DIR)")
    p.add_argument("--fnvtool", type=Path, default=None, help="fnvtool (default NVF_FNVTOOL or <repo>/target/release)")
    p.add_argument("--name", default=None, help="the run's folder name under target/stress/<test>/ (default: the time)")
    p.add_argument("--no-locks", action="store_true", help="do not take the game locks (a machine of your own)")
    p.add_argument("--ini", type=Path, default=None,
                   help="an NVForever.ini to play with (e.g. the player's, for their graphics settings); default: the engine's defaults")


def positive_int(value: str) -> int:
    try:
        n = int(value)
    except ValueError as e:
        raise argparse.ArgumentTypeError("must be a positive integer") from e
    if n < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return n


def comparison_options(p: argparse.ArgumentParser) -> None:
    p.add_argument("--repetitions", type=positive_int, default=1,
                   help="run this tour, battle or soak serially this many times and summarize the current metrics")
    p.add_argument("--build-label", default=None,
                   help="a caller-supplied build label recorded with each run (features are otherwise unknown)")


def configure_test(p, name):
    if name == "cells":
        p.add_argument("--kinds", choices=["all", "interior", "exterior"], default="interior",
                       help="interior by default: the tour streams the exteriors (owner, 2026-10-21)")
        p.add_argument("--worlds", nargs="*", default=None, help="only these worldspaces (Editor IDs or FormIDs)")
        p.add_argument("--stride", type=int, default=1, help="exteriors on every N-th grid line both ways (1: every cell)")
        p.add_argument("--limit", type=int, default=None, help="at most this many cells")
        p.add_argument("--batch", type=int, default=100, help="cells per launch (50 to 200 is usual)")
        p.add_argument("--dwell", type=float, default=2.0, help="seconds in each cell once it is in")
        p.add_argument("--hang", type=float, default=180.0, help="seconds without progress that count as a hang")
        p.add_argument("--start", default="GSGeneralStore", help="the cell each launch starts in (--cell)")
        p.add_argument("--abort-after", type=int, default=cells.ABORT_AFTER,
                       help="stop after this many launches in a row end before their first cell (the game does not start)")
        p.add_argument("--resume", default=None, metavar="RUN",
                       help="go on with an earlier run (its folder name under target/stress/cells/): cells it has done are skipped")
    elif name == "markers":
        p.add_argument("--worlds", nargs="*", default=None, help="only these worldspaces (Editor IDs or FormIDs)")
        p.add_argument("--limit", type=int, default=None, help="at most this many markers")
        p.add_argument("--batch", type=int, default=25, help="markers per launch")
        p.add_argument("--every", type=float, default=30.0, help="seconds between one marker's ask and the next's")
        p.add_argument("--hang", type=float, default=180.0, help="seconds without progress that count as a hang")
        p.add_argument("--start", default="GSGeneralStore", help="the cell each launch starts in (--cell)")
    elif name == "tour":
        p.add_argument("--world", default="WastelandNV")
        p.add_argument("--path", choices=["towns", "ring"], default="towns",
                       help="towns: Goodsprings, Primm, Mojave Outpost, Nipton, Novac, Camp McCarran, Nellis, Jacobstown "
                            "and back (--towns to change them); ring: a ring on the walkable cells")
        p.add_argument("--towns", nargs="+", default=None, metavar="CELL",
                       help="exterior cell Editor IDs to go through in order, back to the first (with --path towns)")
        p.add_argument("--laps", type=int, default=1)
        p.add_argument("--waypoints", type=int, default=12, help="waypoints on the ring (without --route)")
        p.add_argument("--inset", type=float, default=0.3,
                       help="how far in from the walkable world's edge (its largest navmesh area) the ring lies (0..1)")
        p.add_argument("--route", type=Path, default=None, help="a route file of your own (nvf-play stress.rs parse_route)")
        p.add_argument("--step", type=float, default=1024.0, help="units a step")
        p.add_argument("--dwell", type=float, default=1.0, help="seconds between steps")
        p.add_argument("--every", type=float, default=5.0, help="seconds between samples")
        p.add_argument("--slice", default=None, metavar="START:END",
                       help="only waypoints START to END (1-based) of the ring, to go over one stretch again "
                            "(the waypoint numbers are the tour log's `at waypoint i/n`)")
        p.add_argument("--hang", type=float, default=180.0)
        p.add_argument("--hour", type=float, default=12.0, help="the game hour the tour starts at (19: dusk, for what changes at night)")
        p.add_argument("--aid", action="append", default=[], metavar="NAME=VALUE",
                       help="a further test aid for the game, e.g. NVF_PLAY_TEST_NO_READ_AHEAD=1 to compare a run without "
                            "the actor read-ahead (repeatable)")
    elif name == "soak":
        p.add_argument("--world", default="WastelandNV")
        p.add_argument("--towns", nargs="+", default=None, metavar="CELL", help="the towns of the lap (default: the tour's)")
        p.add_argument("--laps", type=int, default=1)
        p.add_argument("--step", type=float, default=1024.0, help="units a step")
        p.add_argument("--dwell", type=float, default=1.0, help="seconds between steps")
        p.add_argument("--every", type=float, default=5.0, help="seconds between samples")
        p.add_argument("--event-every", type=float, default=20.0, help="seconds between events (container, door, fight in turn)")
        p.add_argument("--hang", type=float, default=180.0)
        p.add_argument("--aid", action="append", default=[], metavar="NAME=VALUE",
                       help="a further test aid for the game, e.g. NVF_PLAY_TEST_PROFILE=5 with a profile build (repeatable)")
    elif name == "battle":
        p.add_argument("--per-side", type=int, default=10, help="fighters a side (10, 25, 50)")
        p.add_argument("--aid", action="append", default=[], metavar="NAME=VALUE",
                       help="a further test aid for the game, e.g. NVF_PLAY_TEST_SIGHT_CHECK=1 to check the detection's "
                            "shared and kept lines of sight (repeatable)")
        p.add_argument("--third", type=int, default=0, help="fighters in a third group (creatures), 0 for none")
        p.add_argument("--seconds", type=float, default=180.0, help="how long the run lasts")
        p.add_argument("--every", type=float, default=2.0, help="seconds between samples")
        p.add_argument("--centre", default=f"{battle.CENTRE[0]:g},{battle.CENTRE[1]:g}", help="x,y the battle is round")
        p.add_argument("--gap", type=float, default=3000.0, help="units between the sides")
    else:
        EXTRA_TESTS[name]["configure"](p)


def parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="python tools/stress", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list", help="the tests")
    for command in ("run", "compare"):
        parent = sub.add_parser(command, help="run a test" if command == "run" else "compare two builds in ABBA order")
        tests = parent.add_subparsers(dest="test", required=True)
        for name in [*TESTS, *EXTRA_TESTS]:
            if command == "compare" and name not in COMPARISON_TESTS and name not in EXTRA_TESTS:
                continue
            p = tests.add_parser(name, help=TESTS.get(name, name))
            common(p, comparing=command == "compare")
            configure_test(p, name)
            if name in COMPARISON_TESTS or name in EXTRA_TESTS:
                if command == "compare":
                    paired_options(p)
                else:
                    comparison_options(p)
                g = p.add_argument_group("budget", "existing frame-time limits after start-up")
                g.add_argument("--budget-mean-ms", type=float, default=None)
                g.add_argument("--budget-low1-ms", type=float, default=None)
                g.add_argument("--budget-worst-ms", type=float, default=None)
    return ap


def context(a, test: str) -> lib.Ctx:
    repo = lib.REPO
    exe = a.exe or (Path(os.environ["NVF_EXE"]) if os.environ.get("NVF_EXE") else repo / "engine" / "target" / "release" / lib.EXE_NAME)
    tool = a.fnvtool or (Path(os.environ["NVF_FNVTOOL"]) if os.environ.get("NVF_FNVTOOL") else repo / "target" / "release" / lib.TOOL_NAME)
    data = a.data or os.environ.get("NVF_DATA") or os.environ.get("FNVLAB_DATA_DIR")
    if not data:
        raise SystemExit("no game data: pass --data or set NVF_DATA")
    if not Path(exe).exists():
        raise SystemExit(f"no nvforever at {exe}: build it (engine workspace) or pass --exe / NVF_EXE")
    resume = getattr(a, "resume", None)
    try:
        out = lib.existing_run_folder(repo, test, resume) if resume else lib.run_folder(repo, test, a.name)
    except (FileExistsError, FileNotFoundError, ValueError) as e:
        raise SystemExit(str(e)) from e
    locks = None if a.no_locks else lib.Locks(lib.default_lock_paths(repo))
    if a.ini and not a.ini.is_file():
        raise SystemExit(f"no settings file at {a.ini}")
    return lib.Ctx(repo, Path(exe), data, Path(tool), out, print, locks, lib.masters(repo), a.ini)


def requested_config(a) -> dict:
    """What the caller requested, without pretending saved settings took effect."""
    def plain(value):
        if isinstance(value, Path):
            return str(value)
        if isinstance(value, (list, tuple)):
            return [plain(v) for v in value]
        return value

    skip = {"cmd", "test", "repetitions", "build_label"}
    options = {key: plain(value) for key, value in vars(a).items() if key not in skip}
    aids = list(getattr(a, "aid", []))
    return {
        "options": options,
        "aids_requested": aids,
        "cache_state": {"requested": "unknown", "effective": "unknown"},
        "settings": {
            "ini_requested": str(a.ini) if a.ini else None,
            "effective": "unknown",
        },
    }


def run_once(a, ctx: lib.Ctx) -> tuple[dict, list[str], int]:
    """Run one selected stress test and return its existing result/report pair."""
    manifest = None
    if a.test in COMPARISON_TESTS or a.test in EXTRA_TESTS:
        manifest = collect_manifest(
            ctx.repo,
            ctx.exe,
            ctx.fnvtool,
            ctx.ini,
            test=a.test,
            requested=requested_config(a),
            masters=ctx.masters,
            build_label=getattr(a, "build_label", None),
        )
    if a.test == "cells":
        if not ctx.fnvtool.exists():
            raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace) or pass --fnvtool")
        everything = cells.enumerate_cells(ctx)
        chosen = cells.choose(everything, a.kinds, a.stride, a.worlds, a.limit)
        print(f"cells: {len(chosen)} of {len(everything)} cells, {a.batch} a launch")
        # The results and report are rewritten after every batch, so a long sweep can be
        # read while it runs and nothing is lost if the runner stops.
        res = cells.run(ctx, chosen, a.batch, a.dwell, a.hang, ["--cell", a.start],
                        on_batch=lambda r: lib.write_results(ctx.out, r, cells.report(r)), abort_after=a.abort_after)
        lines = cells.report(res)
        bad = res["crashed"] + res["hung"] + (1 if res["status"] == "aborted" else 0)
    elif a.test == "markers":
        if not ctx.fnvtool.exists():
            raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace) or pass --fnvtool")
        everything = markers.enumerate_markers(ctx)
        want = {w.lower() for w in a.worlds or []}
        chosen = [m for m in everything if not want or m.world.lower() in want or m.world_edid.lower() in want]
        chosen = chosen[:a.limit] if a.limit else chosen
        print(f"markers: {len(chosen)} of {len(everything)} map markers, {a.batch} a launch")
        res = markers.run(ctx, chosen, a.batch, a.every, a.hang, ["--cell", a.start],
                          on_batch=lambda r: lib.write_results(ctx.out, r, markers.report(r)))
        lines = markers.report(res)
        bad = res["crashed"] + res["hung"]
    elif a.test == "tour":
        if a.route:
            route = a.route.read_text(encoding="utf-8")
            if a.slice:
                route = tour.slice_route_text(route, a.slice)
            npts = sum(1 for ln in route.splitlines() if tour.is_route_point(ln))
            perimeter = None
        else:
            if not ctx.fnvtool.exists():
                raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace), or pass --fnvtool or --route")
            wid, _, _ = tour.world_bounds(ctx, a.world)
            # On the walkable world (the largest navmesh area), not the grid's bounds: the
            # grid runs on into empty landscape beyond the playable area.
            nav = tour.nav_cells(ctx, a.world)
            if a.path == "towns":
                pts, stops = tour.town_route(tour.world_cells(ctx, a.world), nav, a.towns or tour.TOWNS)
                print("tour: through " + ", ".join(stops))
            else:
                pts = tour.ring_on_cells(nav, a.waypoints, a.inset)
            if a.slice:
                pts = tour.slice_points(pts, a.slice)
            route = tour.route_text(wid, pts, a.step, a.dwell, a.laps)
            npts = len(pts)
            perimeter = sum(((pts[i][0] - pts[i - 1][0]) ** 2 + (pts[i][1] - pts[i - 1][1]) ** 2) ** 0.5 for i in range(len(pts)))
        steps = (perimeter / a.step if perimeter else npts * 20) + npts
        est = a.laps * steps * (a.dwell + 3.0) + 300
        # Start outdoors in the tour's worldspace; the first step goes to the first waypoint.
        start = ["--cell", "Goodsprings", "--hour", f"{a.hour:g}"]
        print(f"tour: {npts} waypoints, about {steps:.0f} steps a lap, {a.laps} laps; run limit {est:.0f} s")
        aids = dict(kv.split("=", 1) for kv in a.aid)
        res = tour.run(ctx, route, a.laps, a.every, a.hang, start, est, aids)
        lines = tour.report(res)
        bad = 0 if res["status"] == "ok" else 1
    elif a.test == "soak":
        if not ctx.fnvtool.exists():
            raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace) or pass --fnvtool")
        wid, _, _ = tour.world_bounds(ctx, a.world)
        pts, stops = tour.town_route(tour.world_cells(ctx, a.world), tour.nav_cells(ctx, a.world), a.towns or tour.TOWNS)
        print("soak: through " + ", ".join(stops))
        route = tour.route_text(wid, pts, a.step, a.dwell, a.laps)
        perimeter = sum(((pts[i][0] - pts[i - 1][0]) ** 2 + (pts[i][1] - pts[i - 1][1]) ** 2) ** 0.5 for i in range(len(pts)))
        steps = perimeter / a.step + len(pts)
        tour_seconds = a.laps * steps * a.dwell
        est = a.laps * steps * (a.dwell + 3.0) + 600
        res = soak.run(ctx, route, pts, a.world, a.laps, a.every, a.hang, ["--cell", "Goodsprings", "--hour", "12"], est,
                       tour_seconds, a.event_every, dict(kv.split("=", 1) for kv in a.aid))
        lines = soak.report(res)
        bad = 0 if res["status"] == "ok" else 1
    elif a.test == "battle":
        cx, cy = (float(v) for v in a.centre.split(","))
        res = battle.run(ctx, a.per_side, a.third, a.seconds, a.every, (cx, cy), a.gap, dict(kv.split("=", 1) for kv in a.aid))
        lines = battle.report(res)
        bad = 0 if res["status"] == "ok" else 1
    else:
        res, lines, bad = EXTRA_TESTS[a.test]["run"](a, ctx)
    if a.test in COMPARISON_TESTS or a.test in EXTRA_TESTS:
        budget = lib.Budget(a.budget_mean_ms, a.budget_low1_ms, a.budget_worst_ms)
        failed = lib.over_budget(res.get("frames"), budget)
        res["budget"] = {"mean_ms": budget.mean_ms, "low1_ms": budget.low1_ms, "worst_ms": budget.worst_ms, "failed": failed}
        # Right after the status line, so the first lines printed carry the verdict.
        lines[3:3] = lib.budget_report(budget, failed)
        bad += len(failed)
    if manifest is not None:
        res["manifest"] = manifest
    return res, lines, bad


def comparison_quality(res: dict) -> tuple[str, bool, str | None]:
    """Whether a repeat is usable for an aggregate of the current coarse metrics."""
    if res.get("status") != "ok":
        return "unknown", False, f"run status {res.get('status', 'unknown')}"
    machine = res.get("machine")
    if not isinstance(machine, dict):
        return "unknown", False, "no background-CPU samples"
    busy_fraction = machine.get("busy_fraction")
    if isinstance(busy_fraction, bool) or not isinstance(busy_fraction, (int, float)) or not math.isfinite(busy_fraction):
        return "unknown", False, "background-CPU samples have no finite busy fraction"
    if busy_fraction > 0.1:
        return "busy", False, "other programs exceeded the busy threshold"
    frames = res.get("frames")
    count = frames.get("count") if isinstance(frames, dict) else None
    if isinstance(count, bool) or not isinstance(count, (int, float)) or not math.isfinite(count) or count <= 0:
        return "below_busy_threshold", False, "no positive frame count"
    # This says only that the runner warning did not trigger; it is not a quiet-machine claim.
    return "below_busy_threshold", True, None


def comparison_row(index: int, out: Path, res: dict) -> dict:
    quality, included, reason = comparison_quality(res)
    frames = res.get("frames") or {}
    return {
        "repeat": index,
        "output": str(out.name),
        "status": res.get("status"),
        "quality": quality,
        "included": included,
        "exclusion": reason,
        "frames": {key: frames.get(key) for key in ("count", "mean_ms", "low1_ms", "worst_ms")},
        # Tour/soak retain their existing top-ten hitch list; battle has none. Do not turn it into a total.
        "reported_hitches": len(res["hitches"]) if "hitches" in res else None,
    }


def metric_summary(rows: list[dict], key: str) -> dict | None:
    values = [row["frames"][key] for row in rows if row["frames"].get(key) is not None]
    if not values:
        return None
    return {"mean": sum(values) / len(values), "min": min(values), "max": max(values)}


def comparison_index(test: str, label: str | None, rows: list[dict], requested_repetitions: int | None = None,
                     complete: bool = True) -> dict:
    included = [row for row in rows if row["included"]]
    aggregate = {
        "status": "available" if included else "unavailable",
        "included_repetitions": len(included),
        "excluded_repetitions": len(rows) - len(included),
        "metrics": None if not included else {
            key: metric_summary(included, key) for key in ("mean_ms", "low1_ms", "worst_ms")
        },
    }
    return {
        "test": test,
        "status": "complete" if complete else "incomplete",
        "repetitions": len(rows),
        "requested_repetitions": requested_repetitions if requested_repetitions is not None else len(rows),
        "build_label": label,
        "runs": rows,
        "aggregate": aggregate,
    }


def comparison_report(index: dict) -> list[str]:
    out = [f"# Stress comparison: {index['test']}", "",
           f"{index['repetitions']} of {index['requested_repetitions']} serial repetitions"
           + (f" ({index['build_label']})" if index["build_label"] else "") + ".",
           "" if index["status"] == "complete" else "This index is incomplete; it contains only repetitions that finished before the runner stopped.",
           "", "Each row retains the existing whole-run frame metrics after start-up. `1% low` is the mean of the slowest 1% of frames, not p99.",
           "", "| repeat | status | CPU quality | used | mean ms | 1% low ms | worst ms | reported hitches | exclusion |",
           "|---|---|---|---|---:|---:|---:|---:|---|"]
    for row in index["runs"]:
        f = row["frames"]
        out.append(f"| {row['repeat']} | {row['status']} | {row['quality']} | {'yes' if row['included'] else 'no'} | "
                   f"{f['mean_ms'] if f['mean_ms'] is not None else ''} | {f['low1_ms'] if f['low1_ms'] is not None else ''} | "
                   f"{f['worst_ms'] if f['worst_ms'] is not None else ''} | "
                   f"{row['reported_hitches'] if row['reported_hitches'] is not None else ''} | {row['exclusion'] or ''} |")
    aggregate = index["aggregate"]
    out += ["", "## Aggregate", ""]
    if aggregate["status"] == "unavailable":
        out.append("Unavailable: no repetitions met the current status and background-CPU criteria.")
        return out
    out += [f"{aggregate['included_repetitions']} repetitions included; {aggregate['excluded_repetitions']} excluded.", "",
            "| metric | mean | min | max |", "|---|---:|---:|---:|"]
    for key, label in (("mean_ms", "mean frame ms"), ("low1_ms", "slowest-1% mean ms"), ("worst_ms", "worst frame ms")):
        value = aggregate["metrics"][key]
        out.append(f"| {label} | {value['mean']:.3f} | {value['min']:.3f} | {value['max']:.3f} |" if value else f"| {label} |  |  |  |")
    return out


def repeated_context(ctx: lib.Ctx, out: Path) -> lib.Ctx:
    return lib.Ctx(ctx.repo, ctx.exe, ctx.data, ctx.fnvtool, out, ctx.say, ctx.locks, ctx.masters, ctx.ini)


def write_comparison(out: Path, index: dict) -> None:
    (out / "comparison.json").write_text(json.dumps(index, indent=1) + "\n", encoding="utf-8")
    (out / "comparison.md").write_text("\n".join(comparison_report(index)) + "\n", encoding="utf-8")


def main(argv=None) -> int:
    a = parser().parse_args(argv)
    if a.cmd == "list":
        for k, v in {**TESTS, **{name: "additional stress test" for name in EXTRA_TESTS}}.items():
            print(f"{k:<8} {v}")
        return 0
    if a.cmd == "compare":
        if a.exe_base.resolve() == a.exe_candidate.resolve():
            raise SystemExit("base and candidate must use different executable paths")
        for exe in (a.exe_base, a.exe_candidate):
            if not exe.is_file():
                raise SystemExit(f"no executable at {exe}")
        if a.measure_seconds <= 0 or not 0 < a.telemetry_seconds <= 60:
            raise SystemExit("measurement must be positive and telemetry duration must be in (0, 60] seconds")
        if a.settle_seconds + a.measure_seconds > a.telemetry_seconds:
            raise SystemExit("settle and measurement exceed the capture duration before loading")
        for aid in a.aid:
            if "=" not in aid or not aid.split("=", 1)[0]:
                raise SystemExit("test aids must use NAME=VALUE")
        a.exe = a.exe_base
        ctx = context(a, a.test)
        return comparison.run(a, ctx, run_once, comparison_quality)
    ctx = context(a, a.test)
    repetitions = getattr(a, "repetitions", 1)
    if repetitions == 1:
        res, lines, bad = run_once(a, ctx)
        lib.write_results(ctx.out, res, lines)
        print("\n".join(lines[:12]))
        print(f"\nresults: {ctx.out / 'results.json'}\nreport:  {ctx.out / 'report.md'}")
        return 1 if bad else 0

    rows, bad = [], 0
    for repeat in range(1, repetitions + 1):
        try:
            out = lib.new_folder(ctx.out, f"run-{repeat:03d}")
        except (FileExistsError, ValueError) as e:
            raise SystemExit(str(e)) from e
        res, lines, run_bad = run_once(a, repeated_context(ctx, out))
        lib.write_results(out, res, lines)
        rows.append(comparison_row(repeat, out, res))
        bad += run_bad
        write_comparison(ctx.out, comparison_index(a.test, a.build_label, rows, repetitions, complete=False))
    index = comparison_index(a.test, a.build_label, rows, repetitions)
    # The top-level files are an index; the old per-run names remain inside every run-NNN folder.
    write_comparison(ctx.out, index)
    print(f"\ncomparison: {ctx.out / 'comparison.json'}\nreport:     {ctx.out / 'comparison.md'}")
    return 1 if bad else 0
