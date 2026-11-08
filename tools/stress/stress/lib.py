"""What every stress test shares: launching nvforever with test aids under the game
locks, watching it for progress, its memory, crash collection, and the result folder.

The locks, the aid-free environment and the thread-stack dump are tools/smoke's
(imported, not copied): a stress run waits for a smoke run and the other way round.
"""
from __future__ import annotations

import ctypes
import json
import os
import re
import subprocess
import sys
import threading
import time
import tomllib
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "tools" / "smoke"))

from smoke.expect import clean  # noqa: E402
from smoke.runner import Locks, aid_free_environ, default_lock_paths  # noqa: E402
from smoke.stacks import dump as dump_stacks  # noqa: E402

EXE_NAME = "nvforever.exe" if os.name == "nt" else "nvforever"
TOOL_NAME = "fnvtool.exe" if os.name == "nt" else "fnvtool"

# Every stress launch: invulnerable, unseen, script boxes closed, nothing that stops the
# run (nvf-play stress.rs), and no message boxes.
BASE_AIDS = {
    "NVF_PLAY_TEST_GOD": "1",
    "NVF_PLAY_TEST_UNSEEN": "1",
    "NVF_PLAY_TEST_SCRIPT_MENUS": "0.05",
    "NVF_PLAY_TEST_STRESS": "1",
}
BASE_ARGS = ["--play", "--no-messages"]


def masters(repo: Path = REPO) -> list[str]:
    """The ten masters in load order, from tools/smoke/scenarios.toml (one list for both)."""
    with open(repo / "tools" / "smoke" / "scenarios.toml", "rb") as f:
        return list(tomllib.load(f)["masters"])


@dataclass
class Ctx:
    repo: Path
    exe: Path
    data: str
    fnvtool: Path
    out: Path  # this run's folder: target/stress/<test>/<run>
    say: object = print
    locks: Locks | None = None
    masters: list[str] = field(default_factory=list)
    ini: Path | None = None  # an NVForever.ini copied into each launch's home (the player's settings)


def folder_name(name: str) -> str:
    """A single output-directory component, never a path supplied by the caller."""
    path = Path(name)
    if not name or name in {".", ".."} or "/" in name or "\\" in name or ":" in name or path.drive or path.name != name:
        raise ValueError(f"invalid run name {name!r}: use one directory name")
    return name


def new_folder(parent: Path, name: str) -> Path:
    """Make one new named output folder without reusing a previous run's files."""
    d = Path(parent) / folder_name(name)
    try:
        d.mkdir(parents=True, exist_ok=False)
    except FileExistsError as e:
        raise FileExistsError(f"run output already exists: {d}") from e
    return d


def run_folder(repo: Path, test: str, name: str | None = None) -> Path:
    """Make a fresh run folder below this test's output root."""
    root = repo / "target" / "stress" / folder_name(test)
    return new_folder(root, name or time.strftime("%Y%m%d-%H%M%S"))


def existing_run_folder(repo: Path, test: str, name: str) -> Path:
    """Find a named prior run for an explicit resume, without accepting traversal."""
    d = repo / "target" / "stress" / folder_name(test) / folder_name(name)
    if not d.is_dir():
        raise FileNotFoundError(f"no run {name} under target/stress/{test}/")
    return d


# ---------------------------------------------------------------- memory

def process_memory(pid: int) -> tuple[int, int] | None:
    """(working set, private bytes) of a process, Windows only; None elsewhere or on error."""
    if os.name != "nt":
        return None

    class PMC(ctypes.Structure):
        _fields_ = [("cb", ctypes.c_ulong), ("PageFaultCount", ctypes.c_ulong)] + [
            (n, ctypes.c_size_t) for n in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                                           "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage",
                                           "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage",
                                           "PrivateUsage")]

    k32 = ctypes.windll.kernel32
    h = k32.OpenProcess(0x1000 | 0x0010, False, pid)  # QUERY_LIMITED_INFORMATION | VM_READ
    if not h:
        return None
    try:
        c = PMC()
        c.cb = ctypes.sizeof(PMC)
        if not k32.K32GetProcessMemoryInfo(h, ctypes.byref(c), c.cb):
            return None
        return int(c.WorkingSetSize), int(c.PrivateUsage)
    finally:
        k32.CloseHandle(h)


def system_busy_secs() -> float | None:
    """Processor time every core has spent busy since boot (kernel less idle, plus user),
    seconds summed over the cores; Windows only."""
    if os.name != "nt":
        return None
    ft = [ctypes.c_ulonglong() for _ in range(3)]
    if not ctypes.windll.kernel32.GetSystemTimes(*(ctypes.byref(x) for x in ft)):
        return None
    idle, kernel, user = (x.value * 1e-7 for x in ft)
    return kernel - idle + user


def process_busy_secs(pid: int) -> float | None:
    """Processor time a process has used (kernel and user), seconds; Windows only."""
    if os.name != "nt":
        return None
    k32 = ctypes.windll.kernel32
    h = k32.OpenProcess(0x1000, False, pid)  # QUERY_LIMITED_INFORMATION
    if not h:
        return None
    try:
        ft = [ctypes.c_ulonglong() for _ in range(4)]
        if not k32.GetProcessTimes(h, *(ctypes.byref(x) for x in ft)):
            return None
        return (ft[2].value + ft[3].value) * 1e-7
    finally:
        k32.CloseHandle(h)


# Other programs over this share of the machine's cores: the game's timings are not its own.
BUSY = 0.25
# An interval whose game or whole-machine share is over this is impossible and left out
# ([`CpuLoad`]): the system and process counters are read one after the other, not as
# one snapshot, so a share can overshoot 1 by a little [engineering choice: 5 %].
SHARE_LIMIT = 1.05


class CpuLoad:
    """The machine's load between counter reads: the game's share of all cores and the
    other programs' share, each interval's processor time over its elapsed time and the
    core count (issue #1412).

    The first successful read is only a baseline: it starts the first interval and gives
    no share (a share over the moment between the counters' creation and their first read
    came out at 195 % and 390 % of the machine [observed, #1412]). A read that fails keeps
    the last baseline, so the next interval is longer, not lost; so does a read with no
    elapsed time. An interval with an impossible share is left out and counted in
    `skipped` (its end becomes the new baseline), not turned into a zero load. Each kept
    sample is (seconds since the launch at its end, game's share, others' share, its
    elapsed seconds), so the summary can weight it by its time."""

    def __init__(self, cores: int, started: float):
        self.cores = max(cores, 1)
        self.started = started
        self.base: tuple[float, float, float] | None = None
        self.samples: list[tuple[float, float, float, float]] = []
        self.skipped = 0

    def read(self, now: float, system: float | None, process: float | None) -> tuple[float, float, float, float] | None:
        """One read of (monotonic seconds, system busy seconds, the game's busy seconds);
        the interval's sample when it ends a valid one."""
        if system is None or process is None:
            return None
        if self.base is None:
            self.base = (now, system, process)
            return None
        t0, s0, p0 = self.base
        elapsed = now - t0
        if elapsed <= 0:
            # No time has passed: keep the baseline, the next read ends a longer interval.
            # Its time is not lost, so it is not counted in `skipped`.
            return None
        self.base = (now, system, process)
        wall = elapsed * self.cores
        game = max(process - p0, 0.0) / wall
        total = max(system - s0, 0.0) / wall
        if game > SHARE_LIMIT or total > SHARE_LIMIT:
            self.skipped += 1
            return None
        sample = (round(now - self.started, 1), round(game, 3), round(max(total - game, 0.0), 3), round(elapsed, 3))
        self.samples.append(sample)
        return sample


def machine_load(cpu: list[tuple[float, float, float, float]], skipped: int = 0) -> dict | None:
    """From the [`CpuLoad`] samples (seconds, game's share, others' share, elapsed
    seconds): the mean shares and the fraction of the time other programs held over
    [`BUSY`], each interval weighted by its elapsed time. The figures cover the launch's
    kept intervals (`seconds` of them, `intervals` in number; `skipped` left out), not a
    steady-state window a trace may have been cut from."""
    weights = [max(c[3], 0.0) for c in cpu]
    total = sum(weights)
    if not cpu or total <= 0:
        return None
    mean = lambda k: round(sum(w * c[k] for w, c in zip(weights, cpu)) / total, 3)  # noqa: E731
    busy = sum(w for w, c in zip(weights, cpu) if c[2] > BUSY)
    return {"game_mean": mean(1), "others_mean": mean(2), "busy_fraction": round(busy / total, 3), "busy_over": BUSY,
            "scope": "launch intervals", "seconds": round(total, 1), "intervals": len(cpu), "skipped": skipped}


def load_report(load: dict | None) -> list[str]:
    if not load:
        return []
    over = (f" (over {load['seconds']:g} s of the launch's sampled intervals, {load['intervals']} kept"
            + (f", {load['skipped']} left out" if load.get("skipped") else "") + "; not a trace's steady-state window)"
            if "seconds" in load else "")
    out = ["", f"Machine{over}: the game used {load['game_mean']:.0%} of the cores on average, other programs "
           f"{load['others_mean']:.0%}; others held over {load['busy_over']:.0%} for {load['busy_fraction']:.0%} of the time."]
    if load["busy_fraction"] > 0.1:
        out.append("**The machine was busy (builds, other games): hitches here may be other programs', not the "
                   "engine's. Run it again on an idle machine before acting on them.**")
    return out


# ---------------------------------------------------------------- crash reports

FRAME = re.compile(r"^\s*\d+:\s+(?P<fn>\S.*?)\s*$")
NOISE = ("std::", "core::", "alloc::", "backtrace::", "rust_begin_unwind", "<alloc::", "<core::", "<std::",
         "nvforever::crash::", "BaseThreadInitThunk", "RtlUserThreadStart", "__scrt_common_main")
HASHES = re.compile(r"::h[0-9a-f]{16}$")
NUMBERS = re.compile(r"0x[0-9a-fA-F]+|\d+")


@dataclass
class Crash:
    message: str
    location: str
    frames: list[str]
    text: str
    path: str = ""

    def signature(self, top: int = 5) -> str:
        """The panic's cause as a key: its message with numbers and addresses blanked, where
        it was raised (without the line), and the top frames that are the engine's own."""
        loc = re.sub(r":\d+(:\d+)?$", "", self.location)
        return " | ".join([NUMBERS.sub("#", self.message)[:160], loc, *self.frames[:top]])


def parse_crash(text: str, path: str = "") -> Crash:
    """A crash report (engine/nvforever/src/crash.rs report_text)."""
    message = location = ""
    frames: list[str] = []
    in_bt = False
    for line in text.splitlines():
        if line.startswith("message: ") and not message:
            message = line[len("message: "):].strip()
        elif line.startswith("at: ") and not location:
            location = line[len("at: "):].strip()
        elif line.strip() == "backtrace:":
            in_bt = True
        elif in_bt:
            m = FRAME.match(line)
            if m:
                fn = HASHES.sub("", m.group("fn"))
                if not fn.startswith(NOISE) and fn not in frames:
                    frames.append(fn)
    return Crash(message, location, frames, text, path)


def group_crashes(items: list[tuple[str, Crash]], top: int = 5) -> list[dict]:
    """(what crashed, crash) pairs grouped by signature, the biggest group first."""
    groups: dict[str, dict] = {}
    for what, c in items:
        g = groups.setdefault(c.signature(top), {"signature": c.signature(top), "message": c.message,
                                                  "location": c.location, "frames": c.frames[:top], "where": [],
                                                  "report": c.path})
        g["where"].append(what)
    return sorted(groups.values(), key=lambda g: (-len(g["where"]), g["signature"]))


# ---------------------------------------------------------------- launching

def kill_tree(p: subprocess.Popen) -> None:
    """The process and whatever it started (a wrapper's child too)."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)], capture_output=True)
    p.kill()


@dataclass
class Launch:
    name: str
    args: list[str]
    aids: dict[str, str]
    run_seconds: float
    progress: re.Pattern | None = None  # lines that count as progress
    hang_seconds: float = 180.0  # no progress this long: a hang
    memory_every: float = 0.0  # seconds between memory samples (0: none)


@dataclass
class Result:
    name: str
    exit: int | None
    seconds: float
    status: str  # ok, crashed, hung, failed (non-zero exit without a report)
    log: Path
    lines: list[str]
    crashes: list[Crash]
    memory: list[tuple[float, int, int]]  # (seconds, working set, private bytes)
    stacks: Path | None = None
    # (seconds at its end, game's share, others' share, elapsed seconds) per kept interval (CpuLoad)
    cpu: list[tuple[float, float, float, float]] = field(default_factory=list)
    cpu_skipped: int = 0  # intervals left out (impossible share)

    def tail(self, n: int = 40) -> list[str]:
        return self.lines[-n:]


def collect_crashes(home: Path) -> list[Crash]:
    logs = home / "logs"
    out = []
    for p in sorted(logs.glob("crash-*.txt")) if logs.is_dir() else []:
        out.append(parse_crash(p.read_text(encoding="utf-8", errors="replace"), str(p)))
    return out


def run(ctx: Ctx, launch: Launch) -> Result:
    """One game launch under the locks. Its own NVForever home (`<out>/<name>.home`) keeps
    its run log and crash reports apart; its console output goes to `<out>/<name>.log`."""
    home = ctx.out / f"{launch.name}.home"
    home.mkdir(parents=True, exist_ok=True)
    if ctx.ini:
        # The run plays with these settings (graphics, controls) instead of the defaults.
        (home / "NVForever.ini").write_bytes(Path(ctx.ini).read_bytes())
    log = ctx.out / f"{launch.name}.log"
    cmd = [str(ctx.exe), "--data", str(ctx.data), *BASE_ARGS, *launch.args, "--run-seconds", f"{launch.run_seconds:g}"]
    aids = {**BASE_AIDS, **launch.aids, "NVFOREVER_HOME": str(home)}
    (ctx.out / f"{launch.name}.cmd.txt").write_text(
        "".join(f"{k}={v}\n" for k, v in sorted(aids.items())) + subprocess.list2cmdline(cmd) + "\n", encoding="utf-8")
    if ctx.locks:
        ctx.locks.acquire()
    memory: list[tuple[float, int, int]] = []
    status, stacks = "ok", None
    started = time.monotonic()
    load = CpuLoad(os.cpu_count() or 1, started)
    started_at_utc = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    try:
        with open(log, "w", encoding="utf-8", errors="replace") as f:
            p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, cwd=ctx.out,
                                 env=aid_free_environ(aids), text=True, encoding="utf-8", errors="replace")
            last = [time.monotonic()]

            def pump():
                for raw in p.stdout:
                    f.write(raw)
                    f.flush()
                    if launch.progress is None or launch.progress.search(raw):
                        last[0] = time.monotonic()

            t = threading.Thread(target=pump, daemon=True)
            t.start()
            next_mem = 0.0
            limit = launch.run_seconds * 2 + 300
            while p.poll() is None:
                now = time.monotonic()
                if launch.memory_every and now - started >= next_mem:
                    m = process_memory(p.pid)
                    if m:
                        memory.append((round(now - started, 1), *m))
                    # The machine's load since the last read: the game's and the rest's (the
                    # first read is the baseline only).
                    load.read(now, system_busy_secs(), process_busy_secs(p.pid))
                    next_mem = now - started + launch.memory_every
                if now - last[0] > launch.hang_seconds or now - started > limit:
                    stacks = ctx.out / f"{launch.name}.stacks.txt"
                    stacks.write_text(dump_stacks(p.pid, Path(ctx.exe).parent), encoding="utf-8")
                    kill_tree(p)
                    status = "hung"
                    break
                time.sleep(0.5)
            p.wait()
            t.join(timeout=10)
            p.stdout.close()
    finally:
        if ctx.locks:
            ctx.locks.release()
    secs = time.monotonic() - started
    finished_at_utc = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    lines = [clean(x) for x in log.read_text(encoding="utf-8", errors="replace").splitlines()]
    crashes = collect_crashes(home)
    code = p.returncode
    if status != "hung":
        if crashes:
            status = "crashed"
        elif code != 0:
            status = "failed"
    if memory:
        with open(ctx.out / f"{launch.name}.memory.csv", "w", encoding="utf-8") as m:
            m.write("seconds,working_set,private_bytes\n")
            m.writelines(f"{s},{w},{pb}\n" for s, w, pb in memory)
    if load.samples:
        with open(ctx.out / f"{launch.name}.cpu.csv", "w", encoding="utf-8") as c:
            c.write("seconds,game_share,others_share,elapsed\n")
            c.writelines(f"{s},{g},{o},{e}\n" for s, g, o, e in load.samples)
    r = Result(launch.name, code, round(secs, 1), status, log, lines, crashes, memory, stacks, load.samples, load.skipped)
    (ctx.out / f"{launch.name}.exit.json").write_text(json.dumps(
        {"exit": code, "status": status, "seconds": r.seconds, "crashes": [c.path for c in crashes],
         "started_at_utc": started_at_utc, "finished_at_utc": finished_at_utc}), encoding="utf-8")
    return r


# ---------------------------------------------------------------- results

STRAY = re.compile(r"stray entity command #(\d+) \(ignored; the entity is gone\): (.*?)(?:; where: (.*))?$")


def stray_commands(lines: list[str]) -> dict:
    """Commands the engine sent to entities that were gone (nvforever stray_commands.rs):
    how many warnings, and each distinct command or system with its first place."""
    count, first = 0, {}
    for ln in lines:
        if m := STRAY.search(ln):
            count += 1
            what = m.group(2)
            key = what.split(": ", 1)[0]
            first.setdefault(key, {"what": what, "where": m.group(3) or ""})
    return {"count": count, "distinct": list(first.values())}


def stray_report(s: dict) -> list[str]:
    if not s["count"]:
        return []
    out = ["", f"## Stray entity commands ({s['count']} logged, ignored)", "",
           "Commands for entities that were gone; each names what failed (stray_commands.rs).", ""]
    out += [f"- `{d['what']}`" + (f" at {d['where']}" if d["where"] else "") for d in s["distinct"]]
    return out


PROFILE_HITCH = re.compile(r"profile hitch: ([\d.]+) ms; spans ms: (.*?)(?:; pipelines .*)?\s*$")
HITCH_SPAN = re.compile(r"^(.+) ([\d.]+) \((\d+)\)$")


def profile_hitches(lines: list[str], top: int = 10, spans: int = 8) -> list[dict]:
    """The slowest `top` frames the span profile logged on their own (nvforever
    profile.rs, a build with `--features profile` and NVF_PLAY_TEST_PROFILE), each with its
    `spans` costliest spans: what each stall was spent on."""
    out = []
    for ln in lines:
        if not (m := PROFILE_HITCH.search(clean(ln))):
            continue
        listed = []
        for part in m.group(2).split(", "):
            if s := HITCH_SPAN.match(part.strip()):
                listed.append({"span": s.group(1), "ms": float(s.group(2)), "entries": int(s.group(3))})
        out.append({"ms": float(m.group(1)), "at_s": stamp_secs(clean(ln)), "spans": listed[:spans]})
    out.sort(key=lambda h: -h["ms"])
    return out[:top]


def profile_hitch_report(items: list[dict]) -> list[str]:
    if not items:
        return []
    t0 = min((h["at_s"] for h in items if h["at_s"] is not None), default=None)
    out = ["", "## Slowest frames by span (profile build)", "",
           "Spans nest (a schedule holds its systems), so read down to the first one that explains the time.", "",
           "| ms | log s | spans (ms, entries) |", "|---|---|---|"]
    for h in items:
        at = f"{h['at_s'] - t0:.1f}" if t0 is not None and h["at_s"] is not None else ""
        out.append(f"| {h['ms']:g} | {at} | " + "; ".join(f"{s['span']} {s['ms']:g} ({s['entries']})" for s in h["spans"]) + " |")
    return out


@dataclass
class Budget:
    """Frame-time limits a run must keep (after start-up); None: not checked."""
    mean_ms: float | None = None
    low1_ms: float | None = None
    worst_ms: float | None = None

    def asked(self) -> bool:
        return any(v is not None for v in (self.mean_ms, self.low1_ms, self.worst_ms))


def over_budget(frames: dict | None, b: Budget) -> list[str]:
    """What broke the budget, in words; empty when the run kept it."""
    if not b.asked():
        return []
    if not frames:
        return ["no frame statistics in the log (the run ended early?)"]
    out = []
    for key, limit, name in (("mean_ms", b.mean_ms, "mean"), ("low1_ms", b.low1_ms, "1% low"), ("worst_ms", b.worst_ms, "worst")):
        if limit is not None and frames[key] > limit:
            out.append(f"{name} frame {frames[key]:g} ms over the budget's {limit:g} ms")
    return out


def budget_report(b: Budget, failed: list[str]) -> list[str]:
    if not b.asked():
        return []
    limits = ", ".join(f"{n} {v:g} ms" for n, v in (("mean", b.mean_ms), ("1% low", b.low1_ms), ("worst", b.worst_ms)) if v is not None)
    if not failed:
        return ["", f"Budget: PASS ({limits})."]
    return ["", f"Budget: FAIL ({limits})", ""] + [f"- {f}" for f in failed]


def write_results(out: Path, results: dict, report: list[str]) -> None:
    (out / "results.json").write_text(json.dumps(results, indent=1), encoding="utf-8")
    (out / "report.md").write_text("\n".join(report) + "\n", encoding="utf-8")


def mb(n: float) -> str:
    return f"{n / 2**20:.0f} MB"


STAMP = re.compile(r"^\s*(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?)Z?\b")


def stamp_secs(line: str) -> float | None:
    """A log line's time, seconds since the epoch (UTC)."""
    m = STAMP.match(line)
    if not m:
        return None
    s = m.group(1)
    whole, _, frac = s.partition(".")
    t = time.mktime(time.strptime(whole, "%Y-%m-%dT%H:%M:%S"))
    return t + (float("0." + frac) if frac else 0.0)
