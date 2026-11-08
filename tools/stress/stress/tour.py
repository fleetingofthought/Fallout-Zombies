"""tour: laps of the Mojave with the engine's counts and the process's memory sampled
(docs/design/stress-tests.md §4).

The route is a ring of waypoints inside the worldspace's grid (fnvtool `world list`),
or a route file of your own; the `NVF_PLAY_TEST_TOUR` aid (nvf-play stress.rs) steps
the player along it, and `NVF_PLAY_TEST_SAMPLE` (nvforever stress_sample.rs) logs what
the engine holds every few seconds. The runner adds the process's working set and
private bytes, and reports counts that grow lap over lap or stay up when the tour
comes back to a waypoint.
"""
from __future__ import annotations

import json
import math
import re
import subprocess

from . import lib

CELL = 4096.0
SAMPLE = re.compile(r"stress sample: t ([\d.]+) s, (.*), at (.*)$")
FIELD = re.compile(r"^(?P<name>[a-z %]+?) (?P<value>-?[\d.]+(?:/[\d.]+)*)(?: ms)?$")
LAP_START = re.compile(r"stress tour: lap (\d+) starts")
WAYPOINT = re.compile(r"stress tour: lap (\d+) at waypoint (\d+)/(\d+)")
FRAMES = re.compile(r"stress frames: (\d+) frames, mean ([\d.]+) ms, 1% low ([\d.]+) ms \(([\d.]+) fps\), worst ([\d.]+) ms")
ACTORS = re.compile(r"stress actors: (\d+) drawn in (\d+) frames \(([\d.]+) ms, most in a frame ([\d.]+) ms\); "
                    r"those frames mean ([\d.]+) ms, worst ([\d.]+) ms, (\d+) over 100 ms")
STARTUP = re.compile(r"stress start-up: (\d+) frames, ([\d.]+) ms, slowest ([\d.]+) ms")
HITCH = re.compile(r"stress hitch: ([\d.]+) ms at t ([\d.]+) s, (\d+) cells loaded in it \(([\d.]+) ms\),"
                   r"(?: (\d+) actors drawn in it \(([\d.]+) ms\),)? at (.*)$")
# A frame a loading screen went up or came down in (nvforever stress_sample.rs, issue #1415):
# its raw time, left out of the frame times, with the place at its start and at its end.
CROSSING = re.compile(r"stress hitch \(across a phase change, (\S+) to (\S+)\): ([\d.]+) ms at t ([\d.]+) s, "
                      r"(\d+) cells loaded in it \(([-\d.]+) ms\), (\d+) actors drawn in it \(([-\d.]+) ms\), from (.*?) to (.*)$")
LEFT_OUT = re.compile(r"stress frames: .*; (\d+) behind loading screens left out(?:, (\d+) across a phase change left out)?")
PROGRESS = re.compile(r"stress (tour|sample)")
# Counts a leak would show in (the engine's sample fields, and the process's memory).
WATCHED = ["entities", "refs drawn", "meshes", "images", "materials", "models cached", "textures cached", "audio",
           "cells", "bodies", "actors", "ragdolls", "working set", "private bytes", "gpu allocated", "gpu reserved"]
# GPU assets and the caches that hold them: back near where they were once the tour has
# left a place behind (the caches keep what nothing uses for a minute, nvf_assets::release).
ASSETS = ["meshes", "images", "models cached", "textures cached"]


def ring(grid_min, grid_max, n: int = 12, inset: float = 0.3) -> list[tuple[float, float]]:
    """`n` waypoints on an ellipse inside the grid bounds (cells), `inset` of the way in
    from each edge, game units, counter-clockwise from the east."""
    (x0, y0), (x1, y1) = grid_min, grid_max
    cx, cy = (x0 + x1 + 1) / 2 * CELL, (y0 + y1 + 1) / 2 * CELL
    rx, ry = (x1 + 1 - x0) * CELL / 2 * (1 - inset), (y1 + 1 - y0) * CELL / 2 * (1 - inset)
    return [(round(cx + rx * math.cos(2 * math.pi * k / n)), round(cy + ry * math.sin(2 * math.pi * k / n))) for k in range(n)]


def ring_on_cells(cells, n: int = 12, inset: float = 0.3, fill: float = 2.0) -> list[tuple[float, float]]:
    """A ring inside the walkable world: `cells` are the exterior cells of the
    worldspace's largest navmesh area (`fnvtool nav cells`, [x, y, triangles] each).
    From the cells' middle, `n` directions; in each, the farthest navmesh cell within
    that direction's sector, `inset` of the way back towards the middle, snapped onto the
    nearest navmesh cell's centre. Each leg follows the walkable cells between those
    ([`cell_path`]), a point every `fill` cells, so the route stays on the walkable world
    where the area bends (the grid's bounds took the ring into the empty landscape
    beyond it, 2026-10-18). Game units, in order counter-clockwise from the east,
    without repeats."""
    centres = [((x + 0.5) * CELL, (y + 0.5) * CELL) for x, y, *_ in cells]
    if not centres:
        raise ValueError("no navmesh cells")
    cx = sum(p[0] for p in centres) / len(centres)
    cy = sum(p[1] for p in centres) / len(centres)

    def nearest(p):
        return min(centres, key=lambda c: (c[0] - p[0]) ** 2 + (c[1] - p[1]) ** 2)

    sector = math.pi / n
    corners = []
    for k in range(n):
        a = 2 * math.pi * k / n
        best = 0.0
        for x, y in centres:
            d = math.hypot(x - cx, y - cy)
            off = abs((math.atan2(y - cy, x - cx) - a + math.pi) % (2 * math.pi) - math.pi)
            if d > best and off <= sector:
                best = d
        r = best * (1 - inset)
        corners.append(nearest((cx + r * math.cos(a), cy + r * math.sin(a))))
    # Each leg along the walkable cells (a shortest path over the cell grid, eight
    # neighbours), every `fill`-th cell of it kept: a straight leg could cross the empty
    # land in a bend of the area.
    on = {(x, y) for x, y, *_ in cells}
    cell_of = lambda p: (math.floor(p[0] / CELL), math.floor(p[1] / CELL))  # noqa: E731
    centre = lambda c: ((c[0] + 0.5) * CELL, (c[1] + 0.5) * CELL)  # noqa: E731
    out: list[tuple[float, float]] = []
    every = max(1, round(fill))
    for i, a in enumerate(corners):
        path = cell_path(on, cell_of(a), cell_of(corners[(i + 1) % len(corners)]))
        for c in path[:-1][::every]:
            p = centre(c)
            if not out or out[-1] != p:
                out.append(p)
    while len(out) > 1 and out[-1] == out[0]:
        out.pop()
    return [(round(x), round(y)) for x, y in out]


def cell_path(on: set, start: tuple[int, int], goal: tuple[int, int]) -> list[tuple[int, int]]:
    """The shortest way from cell `start` to cell `goal` over the cells in `on` (eight
    neighbours, breadth first), both ends included; just the two ends when there is none."""
    if start == goal:
        return [start]
    prev = {start: None}
    frontier = [start]
    while frontier and goal not in prev:
        nxt = []
        for c in frontier:
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    n = (c[0] + dx, c[1] + dy)
                    if n in on and n not in prev:
                        prev[n] = c
                        nxt.append(n)
        frontier = nxt
    if goal not in prev:
        return [start, goal]
    path = [goal]
    while path[-1] != start:
        path.append(prev[path[-1]])
    return path[::-1]


# The owner's lap of the Mojave (2026-10-18): Goodsprings, Primm, Mojave Outpost, Nipton,
# Novac, Camp McCarran, Nellis, Jacobstown and back to Goodsprings, by the exterior cells'
# Editor IDs [observed: fnvtool world cells, WastelandNV: Goodsprings -18,0, Primm
# -14,-14, MojaveOutpost -21,-25, Nipton -8,-23, Novac 5,-9, CampMcCarranGate -5,18,
# NellisAFBEntrance 6,29, Jacobstown -27,22].
TOWNS = ["Goodsprings", "Primm", "MojaveOutpost", "Nipton", "Novac", "CampMcCarranGate", "NellisAFBEntrance", "Jacobstown"]


def town_route(world_cells: list[dict], nav: list, towns: list[str] = TOWNS, fill: int = 2) -> tuple[list[tuple[float, float]], list[str]]:
    """A closed route through `towns` (exterior cell Editor IDs, in order) over the
    walkable cells: each town's cell (or the navmesh cell nearest it, when its own cell
    has none), joined to the next along the navmesh cells ([`cell_path`]), a point every
    `fill` cells. `world_cells`: fnvtool `world cells` rows (edid, grid). Returns the
    points (game units) and, for each town, the index of its point."""
    by_edid = {c["edid"].lower(): tuple(c["grid"]) for c in world_cells if c.get("edid") and c.get("grid")}
    missing = [t for t in towns if t.lower() not in by_edid]
    if missing:
        raise ValueError(f"no exterior cell named {', '.join(missing)}")
    on = {(x, y) for x, y, *_ in nav}
    if not on:
        raise ValueError("no navmesh cells")

    def snap(c):
        return c if c in on else min(on, key=lambda o: (o[0] - c[0]) ** 2 + (o[1] - c[1]) ** 2)

    stops = [snap(by_edid[t.lower()]) for t in towns]
    out: list[tuple[int, int]] = []
    marks = []
    for i, a in enumerate(stops):
        marks.append(len(out))
        path = cell_path(on, a, stops[(i + 1) % len(stops)])
        keep = path[:-1][::fill]
        for c in keep:
            if not out or out[-1] != c:
                out.append(c)
    return [((x + 0.5) * CELL, (y + 0.5) * CELL) for x, y in out], [f"{t} at point {m + 1}" for t, m in zip(towns, marks)]


def slice_points(points: list, spec: str) -> list:
    """Waypoints `START:END` (1-based, both included) of a ring: a short route over one
    stretch of it, to go over a place again (a crash's)."""
    a, _, b = spec.partition(":")
    lo, hi = max(1, int(a)), min(len(points), int(b) if b else len(points))
    if lo > hi:
        raise ValueError(f"empty slice {spec} of {len(points)} waypoints")
    return points[lo - 1:hi]


ROUTE_KEYS = ("world", "step", "dwell", "laps")


def is_route_point(line: str) -> bool:
    """A waypoint line of a route file (`x y`), not a setting or a comment."""
    words = line.split("#")[0].split()
    return len(words) == 2 and words[0] not in ROUTE_KEYS


def slice_route_text(text: str, spec: str) -> str:
    """A route file with only waypoints `START:END` of it (as `slice_points`), its settings
    and comments kept."""
    lines = text.splitlines()
    points = [i for i, ln in enumerate(lines) if is_route_point(ln)]
    keep = set(slice_points(points, spec))
    return "".join(ln + "\n" for i, ln in enumerate(lines) if i in keep or not is_route_point(ln))


def route_text(world: str, points, step: float, dwell: float, laps: int) -> str:
    return (f"# tools/stress tour\nworld {world}\nstep {step:g}\ndwell {dwell:g}\nlaps {laps}\n"
            + "".join(f"{x:g} {y:g}\n" for x, y in points))


def parse_sample(line: str) -> dict | None:
    m = SAMPLE.search(line)
    if not m:
        return None
    s = {"t": float(m.group(1)), "at": m.group(3)}
    for part in m.group(2).split(", "):
        f = FIELD.match(part.strip())
        if not f:
            continue
        name, value = f.group("name").strip(), f.group("value")
        if "/" in value:
            s[name] = sum(float(v) for v in value.split("/"))
        else:
            s[name] = float(value)
    return s


def tag_laps(lines: list[str]) -> list[dict]:
    """The samples of a log, each with the lap and waypoint the tour was on."""
    lap, wp, out = 0, 0, []
    for ln in lines:
        if m := LAP_START.search(ln):
            lap, wp = int(m.group(1)), 0
        elif m := WAYPOINT.search(ln):
            lap, wp = int(m.group(1)), int(m.group(2))
        elif s := parse_sample(ln):
            s["lap"], s["waypoint"] = lap, wp
            out.append(s)
    return out


def add_memory(samples: list[dict], memory: list[tuple[float, int, int]], started_at: float = 0.0) -> None:
    """The nearest memory reading (by run seconds) on each sample."""
    if not memory:
        return
    for s in samples:
        t = s["t"] + started_at
        _, ws, pb = min(memory, key=lambda m: abs(m[0] - t))
        s["working set"], s["private bytes"] = ws, pb


def growth(samples: list[dict], fields=WATCHED, tolerance: float = 0.05) -> dict:
    """Possible leaks: for each count, its value at each waypoint lap after lap. A count
    that is higher at the same waypoint on every later lap than on the first (by more than
    `tolerance` of it) is reported with the waypoints where it grew, and its lap-end values."""
    laps = sorted({s["lap"] for s in samples if s["lap"] > 0})
    out = {}
    for f in fields:
        at: dict[tuple[int, int], tuple[float, str]] = {}
        for s in samples:
            if f in s and s["lap"] > 0:
                at[(s["lap"], s["waypoint"])] = (s[f], s.get("at", ""))  # the last sample there
        ends = []
        for lap in laps:
            vals = [v for (l, _), (v, _) in sorted(at.items()) if l == lap]
            if vals:
                ends.append(vals[-1])
        grew = []
        for (l, wp), (v, place) in sorted(at.items()):
            if not laps or l == laps[0]:
                continue
            first = at.get((laps[0], wp))
            if first is not None and v > first[0] * (1 + tolerance) + 1:
                grew.append({"lap": l, "waypoint": wp, "first": first[0], "now": v, "at": place})
        rising = len(ends) >= 2 and all(b > a * (1 + tolerance) for a, b in zip(ends, ends[1:]))
        if grew or rising:
            out[f] = {"lap_ends": ends, "rising_every_lap": rising, "grew_at": grew[:30]}
    return out


def not_released(samples: list[dict], fields=ASSETS, part: float = 0.1, tolerance: float = 0.25, slack: float = 100) -> dict:
    """Assets not let go within a lap: a lap ends where it began (the ring closes), so a
    count whose mean over the lap's last `part` of samples is above its mean over the
    first `part` by more than `tolerance` of it plus `slack` was kept after the places
    that loaded it were left. Per count: the laps with (start, end, peak)."""
    out = {}
    for f in fields:
        bad = []
        for lap in sorted({s["lap"] for s in samples if s["lap"] > 0}):
            vals = [s[f] for s in samples if s["lap"] == lap and f in s]
            if len(vals) < 4:
                continue
            n = max(1, round(len(vals) * part))
            start, end = sum(vals[:n]) / n, sum(vals[-n:]) / n
            if end > start * (1 + tolerance) + slack:
                bad.append({"lap": lap, "start": round(start, 1), "end": round(end, 1), "peak": max(vals)})
        if bad:
            out[f] = bad
    return out


def startup_and_hitches(lines: list[str], top: int = 10) -> tuple[dict | None, list[dict]]:
    """The engine's start-up line, and its slowest `top` hitches after start-up."""
    startup = None
    hitches = []
    for ln in lines:
        if m := STARTUP.search(ln):
            startup = {"frames": int(m.group(1)), "ms": float(m.group(2)), "slowest_ms": float(m.group(3))}
        elif m := HITCH.search(ln):
            hitches.append({"ms": float(m.group(1)), "t": float(m.group(2)), "cells": int(m.group(3)),
                            "cell_ms": float(m.group(4)), "actors": int(m.group(5) or 0),
                            "actor_ms": float(m.group(6) or 0), "at": m.group(7)})
    hitches.sort(key=lambda h: -h["ms"])
    return startup, hitches[:top]


def crossing_hitches(lines: list[str], top: int = 10) -> list[dict]:
    """The slowest `top` frames over the hitch line that a loading screen went up or came
    down in (`stress hitch (across a phase change, …)`): kept apart from the seen hitches,
    since which phase took their time is not known."""
    out = []
    for ln in lines:
        if m := CROSSING.search(ln):
            out.append({"from": m.group(1), "to": m.group(2), "ms": float(m.group(3)), "t": float(m.group(4)),
                        "cells": int(m.group(5)), "cell_ms": float(m.group(6)), "actors": int(m.group(7)),
                        "actor_ms": float(m.group(8)), "from_at": m.group(9), "at": m.group(10)})
    out.sort(key=lambda h: -h["ms"])
    return out[:top]


def windows_across_changes(samples: list[dict]) -> int:
    """Sample windows a loading screen went up or came down in (`phase changes` over 0):
    their frame mean and worst cover only the seen frames, their place is where they ended."""
    return sum(1 for s in samples if s.get("phase changes", 0) > 0)


def write_csv(path, samples: list[dict]) -> None:
    keys = ["t", "lap", "waypoint"] + sorted({k for s in samples for k in s} - {"t", "lap", "waypoint", "at"}) + ["at"]
    with open(path, "w", encoding="utf-8") as f:
        f.write(",".join(keys) + "\n")
        for s in samples:
            f.write(",".join('"' + str(s.get(k, "")).replace('"', "'") + '"' if k == "at" else str(s.get(k, "")) for k in keys) + "\n")


def world_cells(ctx: lib.Ctx, world: str) -> list[dict]:
    """The worldspace's cells (`fnvtool world cells`, fnv.world.cells/1)."""
    cmd = [str(ctx.fnvtool), "world", "cells", str(ctx.data), *ctx.masters, "--schema", "schemas/fnv", "--world", world]
    return json.loads(subprocess.run(cmd, capture_output=True, text=True, cwd=ctx.repo, check=True).stdout)["cells"]


def nav_cells(ctx: lib.Ctx, world: str) -> list:
    """The worldspace's walkable cells (`fnvtool nav cells`, fnv.nav.cells/1)."""
    cmd = [str(ctx.fnvtool), "nav", "cells", str(ctx.data), *ctx.masters, "--schema", "schemas/fnv", "--space", world]
    return json.loads(subprocess.run(cmd, capture_output=True, text=True, cwd=ctx.repo, check=True).stdout)["cells"]


def world_bounds(ctx: lib.Ctx, world: str):
    cmd = [str(ctx.fnvtool), "world", "list", str(ctx.data), *ctx.masters, "--schema", "schemas/fnv"]
    wl = json.loads(subprocess.run(cmd, capture_output=True, text=True, cwd=ctx.repo, check=True).stdout)
    for w in wl["worldspaces"]:
        if (w.get("edid") or "").lower() == world.lower() or w["formid"].lower() == world.lower():
            return w["formid"], w["grid_min"], w["grid_max"]
    raise SystemExit(f"no worldspace {world} in fnvtool's world list")


def run(ctx: lib.Ctx, route: str, laps: int, every: float, hang: float, start_args: list[str], est_seconds: float,
        aids: dict[str, str] | None = None) -> dict:
    """`aids`: further test aids for the game (`--aid`)."""
    path = ctx.out / "route.txt"
    path.write_text(route, encoding="utf-8")
    env = {"NVF_PLAY_TEST_TOUR": str(path), "NVF_PLAY_TEST_SAMPLE": f"{every:g}", **(aids or {})}
    launch = lib.Launch("tour", start_args, env,
                        run_seconds=est_seconds, progress=PROGRESS, hang_seconds=hang, memory_every=every)
    r = lib.run(ctx, launch)
    samples = tag_laps(r.lines)
    # The sample's t is the engine's real clock since its start; the memory's seconds count
    # from the launch: the first sample's line time against the launch's start aligns them.
    first = next((ln for ln in r.lines if "stress sample:" in ln), None)
    start_line = next((ln for ln in r.lines if lib.stamp_secs(ln) is not None), None)
    offset = 0.0
    if first and start_line:
        offset = (lib.stamp_secs(first) - lib.stamp_secs(start_line)) - (parse_sample(first) or {"t": 0})["t"]
    add_memory(samples, r.memory, max(offset, 0.0))
    write_csv(ctx.out / "samples.csv", samples)
    frames = next((FRAMES.search(ln) for ln in reversed(r.lines) if FRAMES.search(ln)), None)
    actors = next((ACTORS.search(ln) for ln in reversed(r.lines) if ACTORS.search(ln)), None)
    startup, hitches = startup_and_hitches(r.lines)
    left_out = next((LEFT_OUT.search(ln) for ln in reversed(r.lines) if LEFT_OUT.search(ln)), None)
    laps_done = max((int(m.group(1)) for ln in r.lines if (m := re.search(r"stress tour: lap (\d+) done", ln))), default=0)
    return {
        "test": "tour",
        "status": r.status,
        "laps_asked": laps,
        "laps_done": laps_done,
        "samples": len(samples),
        "frames": None if not frames else {"count": int(frames.group(1)), "mean_ms": float(frames.group(2)),
                                           "low1_ms": float(frames.group(3)), "low1_fps": float(frames.group(4)),
                                           "worst_ms": float(frames.group(5)),
                                           "hidden": int(left_out.group(1)) if left_out else None,
                                           "crossed": int(left_out.group(2)) if left_out and left_out.group(2) else None},
        "actors": None if not actors else {"drawn": int(actors.group(1)), "frames": int(actors.group(2)),
                                           "ms": float(actors.group(3)), "most_ms": float(actors.group(4)),
                                           "frame_mean_ms": float(actors.group(5)), "frame_worst_ms": float(actors.group(6)),
                                           "over_100": int(actors.group(7))},
        "peak": {f: max((s[f] for s in samples if f in s), default=None) for f in WATCHED},
        "possible_leaks": growth(samples),
        "not_released": not_released(samples),
        "startup": startup,
        "hitches": hitches,
        "crossing_hitches": crossing_hitches(r.lines),
        "windows_across_changes": windows_across_changes(samples),
        "machine": lib.machine_load(r.cpu, r.cpu_skipped),
        "profile_hitches": lib.profile_hitches(r.lines),
        "crash_groups": lib.group_crashes([("tour", c) for c in r.crashes]),
        "stray_commands": lib.stray_commands(r.lines),
        "log": str(r.log),
        "tail": r.tail(15) if r.status != "ok" else [],
    }


def report(res: dict) -> list[str]:
    out = ["# Stress test: tour", "", f"Status {res['status']}: {res['laps_done']} of {res['laps_asked']} laps, "
           f"{res['samples']} samples (samples.csv)."]
    if res["frames"]:
        f = res["frames"]
        out += ["", f"Frames: {f['count']}, mean {f['mean_ms']} ms, 1% low {f['low1_ms']} ms ({f['low1_fps']} fps), worst {f['worst_ms']} ms"
                " (after start-up)"]
        if f.get("crossed") is not None:
            out.append(f"Left out: {f['hidden']} frames behind loading screens, {f['crossed']} that a loading screen "
                       "went up or came down in.")
    if res.get("actors"):
        a = res["actors"]
        out.append(f"Actors drawn: {a['drawn']} in {a['frames']} frames, {a['ms']:g} ms (most in a frame {a['most_ms']:g} ms); "
                   f"those frames mean {a['frame_mean_ms']:g} ms, worst {a['frame_worst_ms']:g} ms, {a['over_100']} over 100 ms")
    if res.get("startup"):
        st = res["startup"]
        out.append(f"Start-up: {st['frames']} frames, {st['ms']:g} ms, slowest {st['slowest_ms']:g} ms (left out of the frames above)")
    if res.get("hitches"):
        out += ["", "## Slowest hitches", "", "| ms | t | cells loaded in it | actors drawn in it | at |", "|---|---|---|---|---|"]
        out += [f"| {h['ms']:g} | {h['t']:g} | {h['cells']} ({h['cell_ms']:g} ms) | {h.get('actors', 0)} ({h.get('actor_ms', 0):g} ms) "
                f"| {h['at']} |" for h in res["hitches"]]
    if res.get("crossing_hitches"):
        out += ["", "## Hitches across a phase change (left out of the frame times)", "",
                "A loading screen went up or came down in these frames: their time is real, but whether the "
                "loading or the game took it is not known.", "",
                "| ms | t | change | cells loaded in it | actors drawn in it | from | to |", "|---|---|---|---|---|---|---|"]
        out += [f"| {h['ms']:g} | {h['t']:g} | {h['from']} to {h['to']} | {h['cells']} ({h['cell_ms']:g} ms) "
                f"| {h['actors']} ({h['actor_ms']:g} ms) | {h['from_at']} | {h['at']} |" for h in res["crossing_hitches"]]
    if res.get("windows_across_changes"):
        out += ["", f"{res['windows_across_changes']} of {res['samples']} sample windows held a loading screen's coming "
                "or going (`phase changes`): their frame mean and worst cover only the seen frames, and their place "
                "is where the window ended."]
    out += lib.load_report(res.get("machine"))
    out += lib.profile_hitch_report(res.get("profile_hitches", []))
    out += ["", "## Peaks", "", "| count | peak |", "|---|---|"]
    for k, v in res["peak"].items():
        if v is not None:
            out.append(f"| {k} | {lib.mb(v) if k in ('working set', 'private bytes') else f'{v:g}'} |")
    out += ["", "## Possible leaks", ""]
    if not res["possible_leaks"]:
        out.append("None: no count grew lap over lap or stayed up on coming back to a waypoint.")
    for k, v in res["possible_leaks"].items():
        out.append(f"- **{k}**: lap ends {v['lap_ends']}" + (" (rising every lap)" if v["rising_every_lap"] else ""))
        for g in v["grew_at"][:10]:
            out.append(f"  - lap {g['lap']} waypoint {g['waypoint']} ({g['at']}): {g['first']:g} on lap 1, {g['now']:g} now")
    out += ["", "## Assets not released", ""]
    if not res.get("not_released"):
        out.append("None: meshes, images and the caches came back near their lap-start counts by each lap's end.")
    for k, laps in res.get("not_released", {}).items():
        for b in laps:
            out.append(f"- **{k}**: lap {b['lap']} started near {b['start']:g}, ended near {b['end']:g} (peak {b['peak']:g})")
    if res["crash_groups"]:
        out += ["", "## Crashes", ""] + [f"- {g['message']} at {g['location']}" for g in res["crash_groups"]]
    out += lib.stray_report(res.get("stray_commands", {"count": 0}))
    if res["tail"]:
        out += ["", "## Last log lines", "", "```", *res["tail"], "```"]
    return out
