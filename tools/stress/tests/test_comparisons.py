"""Generated comparison fixtures, with scenario execution replaced by pure fakes."""
from __future__ import annotations

import copy
import io
import json
import re
import sys
import tempfile
import unittest
from contextlib import redirect_stderr
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from stress import cli, comparison, lib, telemetry  # noqa: E402


def records(run_id="run", scenario_id="scenario"):
    header = {
        "type": "header", "seq": 0, "t_ns": 0, "schema_version": 1, "capture_id": "synthetic",
        "run_id": run_id, "scenario_id": scenario_id, "started_at_utc": "2026-11-01T12:00:00Z",
        "engine_revision": "engine-commit", "build_features": [], "adapter": "synthetic adapter", "backend": "synthetic",
        "driver": "fixture driver", "driver_info": "fixture", "width_px": 1280, "height_px": 720, "display_sync": "off",
        "cap_fps": 0, "refresh_millihz": 60000, "window_present_mode": "AutoNoVsync", "unavailable": {},
        "max_duration_ns": 60_000_000_000, "max_frames": 100000, "max_bytes": 67108864,
        "diagnostics": "disabled", "diagnostics_reason": "not_requested",
        "gpu_origin": "unavailable_from_bevy_0_20_1", "render_cpu_origin": "source_frame_unavailable_from_bevy_0_20_1",
    }
    result = [header]
    def frame(t, duration, phase, enter=False):
        result.append({"type": "frame", "seq": len(result), "t_ns": round(t * 1e9), "main_frame": len(result) - 1,
                       "duration_ns": None if duration is None else round(duration * 1e6),
                       "duration_reason": "no_previous_time_update" if duration is None else None, "phase": phase,
                       "markers": [{"kind": "phase_enter", "id": phase, "t_ns": round(t * 1e9)}] if enter else []})
    frame(0, None, "startup", True)
    frame(1, 1000, "loading", True)
    frame(2, 240, "gameplay", True)
    for i in range(1, 451):
        frame(2 + i / 10, 300 if i == 200 else 10, "gameplay")
    result.append({"type": "trailer", "seq": len(result), "t_ns": 48_000_000_000, "status": "complete",
                   "stop_reason": "normal_exit", "cut_seq": None, "frames_attempted": len(result) - 1,
                   "frames_written": len(result) - 1, "dropped_frames": 0, "dropped_diagnostics": 0,
                   "dropped_markers": 0, "gpu_observations_written": 0,
                   "phases": {phase: telemetry.statistics([f for f in result if f.get("phase") == phase]) for phase in telemetry.PHASES}})
    return result


def save_capture(home, data, filename=None):
    logs = home / "logs"
    logs.mkdir(parents=True, exist_ok=True)
    p = logs / (filename or f"frame-telemetry-{data[0]['capture_id']}.jsonl")
    p.write_text("".join(json.dumps(r, separators=(",", ":")) + "\n" for r in data), encoding="utf-8")
    t, g = data[-1], data[-1].get("phases", {}).get("gameplay", {})
    quantiles = " ".join(f"p{q} {'none' if g.get(f'p{q}_ms') is None else format(g[f'p{q}_ms'], '.3f')} ms" for q in (50, 95, 99))
    start = f"frame telemetry: capturing 60 s to {p} (frames)"
    if data[0]["diagnostics"] != "disabled":
        start = start.replace("(frames)", "(frames + render timings)")
    end = (f"frame telemetry: {p} {t.get('status')} ({t.get('stop_reason')}), {t.get('frames_written')} frames, gameplay "
           f"{quantiles}, dropped {sum(t.get(k, 0) for k in ('dropped_frames', 'dropped_diagnostics', 'dropped_markers'))}, {p.stat().st_size} bytes")
    if data[0]["diagnostics"] != "disabled":
        end += f", gpu observations {t.get('gpu_observations_written', 0)}"
    return p, [start, end]


class CaptureTest(unittest.TestCase):
    def consume(self, transform=None, log_transform=None):
        with tempfile.TemporaryDirectory() as td:
            home = Path(td)
            data = records()
            if transform:
                transform(data)
            p, lines = save_capture(home, data)
            if log_transform:
                lines = log_transform(lines)
            return telemetry.consume(home, lines, "run", "scenario")

    def test_valid_capture_and_reveal_excluded_but_real_hitch_retained(self):
        result = self.consume()
        self.assertEqual(result["outcome"], "valid")
        self.assertEqual(len(result["sha256"]), 64)
        self.assertEqual(result["window"]["statistics"]["worst_ms"], 300)
        self.assertEqual(result["window"]["transitions"]["worst_ms"], 240)
        self.assertEqual(result["window"]["statistics"]["above_100_ms"], 1)
        self.assertEqual(result["phases"]["gameplay"]["above_100_ms"], 2)
        self.assertEqual(result["window"]["statistics"]["samples"], 299)

    def test_missing_and_old_runs_have_explicitly_unavailable_percentiles(self):
        with tempfile.TemporaryDirectory() as td:
            for requested in (True, False):
                result = telemetry.consume(Path(td), [], "run", "scenario", requested=requested)
                self.assertEqual(result["outcome"], "missing")
                self.assertFalse(result["percentiles_available"])
                self.assertEqual(result["requested"], requested)

    def test_ambiguous_matching_captures_are_retained(self):
        with tempfile.TemporaryDirectory() as td:
            home = Path(td)
            save_capture(home, records())
            other = records()
            other[0]["capture_id"] = "second"
            save_capture(home, other)
            result = telemetry.consume(home, [], "run", "scenario")
            self.assertEqual(result["outcome"], "ambiguous")
            self.assertEqual(len(result["candidates"]), 2)

    def test_wrong_ids_are_not_borrowed_from_another_launch(self):
        self.assertEqual(self.consume(lambda r: r[0].update(run_id="other"))["outcome"], "missing")

    def test_flush_unconfirmed_and_missing_summaries(self):
        for change in (lambda lines: [lines[0]], lambda lines: [lines[0], lines[1] + ", flush unconfirmed"],
                       lambda lines: [lines[0], lines[1] + ", writer error: synthetic"], lambda lines: [*lines, lines[-1]]):
            self.assertEqual(self.consume(log_transform=change)["outcome"], "flush_unconfirmed")

    def test_summary_path_or_statistics_mismatch(self):
        self.assertEqual(self.consume(log_transform=lambda lines: [lines[0].replace("synthetic.jsonl", "other.jsonl"), lines[1]])["outcome"],
                         "flush_unconfirmed")
        self.assertEqual(self.consume(log_transform=lambda lines: [lines[0], lines[1].replace("p50 10.000", "p50 99.000")])["outcome"], "invalid")

    def test_partial_line_and_missing_trailer_are_truncated(self):
        with tempfile.TemporaryDirectory() as td:
            p, lines = save_capture(Path(td), records())
            p.write_bytes(p.read_bytes()[:-2])
            self.assertEqual(telemetry.consume(Path(td), lines, "run", "scenario")["outcome"], "truncated")
        self.assertEqual(self.consume(lambda r: r.pop())["outcome"], "truncated")

    def test_all_loss_types_are_truncated(self):
        for loss in ("dropped_frames", "dropped_diagnostics", "dropped_markers"):
            def change(r):
                r[-1].update(status="truncated", **{loss: 1})
                if loss != "dropped_markers":
                    r[-1]["seq"] += 1
                if loss == "dropped_frames":
                    r[-1]["frames_attempted"] += 1
            self.assertEqual(self.consume(change)["outcome"], "truncated")

    def test_abnormal_stops_are_truncated(self):
        for reason in ("writer_error", "shutdown_timeout", "settings_changed"):
            self.assertEqual(self.consume(lambda r: r[-1].update(status="truncated", stop_reason=reason))["outcome"], "truncated")

    def test_byte_cut_excludes_later_trailer_seq(self):
        def cut(r):
            r[-1].update(stop_reason="byte_limit", cut_seq=r[-2]["seq"], seq=r[-1]["seq"] + 123, t_ns=r[-2]["t_ns"])
        self.assertEqual(self.consume(cut)["outcome"], "valid")

    def test_loss_before_byte_cut_is_still_truncated(self):
        def cut(r):
            for record in r[2:]:
                record["seq"] += 1
            r[-1].update(status="truncated", stop_reason="byte_limit", cut_seq=r[-2]["seq"],
                         dropped_diagnostics=1, seq=r[-1]["seq"] + 10, t_ns=r[-2]["t_ns"])
        self.assertEqual(self.consume(cut)["outcome"], "truncated")

    def test_forged_statistics_and_bad_counts_are_invalid(self):
        for change in (lambda r: r[-1]["phases"]["gameplay"].update(p99_ms=999),
                       lambda r: r[-1].update(frames_written=1), lambda r: r[-1].update(seq=r[-1]["seq"] + 1),
                       lambda r: r[-1].update(gpu_observations_written=1), lambda r: r[-1].update(cut_seq=2)):
            self.assertEqual(self.consume(change)["outcome"], "invalid")

    def test_bad_envelope_order_types_and_required_fields(self):
        changes = [lambda r: r[0].update(schema_version=2), lambda r: r[0].update(max_frames=3),
                   lambda r: r[3].update(duration_ns=True), lambda r: r[3].update(duration_ns=float("inf")),
                   lambda r: r[3].update(type="unknown"), lambda r: r[3].update(t_ns=0),
                   lambda r: r[3].update(duration_reason="invented"), lambda r: r[3].pop("markers"),
                   lambda r: r.insert(1, copy.deepcopy(r[0])), lambda r: r.append(copy.deepcopy(r[-1])),
                   lambda r: r[0].update(capture_id="wrong-name"),
                   lambda r: r[3].update(markers=[{"kind": "invented", "id": "x", "t_ns": 0}]),
                   lambda r: r[3].update(duration_ns=None, duration_reason="no_previous_time_update")]
        # The filename test changes the file after saving, since save_capture names it from the header.
        changes.pop(10)
        for change in changes:
            with self.subTest(change=change):
                self.assertEqual(self.consume(change)["outcome"], "invalid")

    def test_filename_bom_and_duplicate_keys_are_invalid(self):
        with tempfile.TemporaryDirectory() as td:
            home = Path(td)
            p, lines = save_capture(home, records(), "frame-telemetry-wrong.jsonl")
            self.assertEqual(telemetry.consume(home, lines, "run", "scenario")["outcome"], "invalid")
            p.unlink()
            for mutate in (lambda b: b"\xef\xbb\xbf" + b, lambda b: b.replace(b'"seq":0', b'"seq":0,"seq":0', 1)):
                p, lines = save_capture(home, records())
                p.write_bytes(mutate(p.read_bytes()))
                self.assertEqual(telemetry.consume(home, lines, "run", "scenario")["outcome"], "invalid")

    def test_unavailable_header_requires_reason_and_does_not_infer_revision(self):
        def missing(r):
            r[0].update(engine_revision=None)
            r[0]["unavailable"]["engine_revision"] = "build_git_unavailable"
        result = self.consume(missing)
        self.assertEqual(result["outcome"], "valid")
        self.assertIsNone(result["header"]["engine_revision"])
        self.assertEqual(self.consume(lambda r: r[0].update(engine_revision=None))["outcome"], "invalid")

    def test_complete_capture_can_have_insufficient_window(self):
        def shorten(r):
            del r[100:-1]
            r[-1].update(seq=r[-2]["seq"] + 1, t_ns=r[-2]["t_ns"], frames_written=len(r) - 2, frames_attempted=len(r) - 2)
            r[-1]["phases"] = {p: telemetry.statistics([f for f in r if f.get("phase") == p]) for p in telemetry.PHASES}
        self.assertEqual(self.consume(shorten)["outcome"], "insufficient_window")

    def test_main_frame_wrap_and_earlier_diagnostic_import_are_allowed(self):
        def diagnostic(r):
            r[0].update(diagnostics="enabled", diagnostics_reason=None)
            r[-2]["main_frame"] = 0
            r.insert(-1, {"type": "diagnostic", "seq": r[-1]["seq"], "t_ns": r[-2]["t_ns"], "path": "render/elapsed_gpu",
                          "unit": "ms", "kind": "gpu_elapsed", "value": None, "status": "unavailable",
                          "reason": "no_sample_observed", "observation_t_ns": 5, "observation_main_frame": 0})
            r[-1]["seq"] += 1
        self.assertEqual(self.consume(diagnostic)["outcome"], "valid")

    def test_bounds_header_reasons_and_unexpected_nonfinite_fields(self):
        for change in (lambda r: r[0].update(width_px=True), lambda r: r[0].update(build_features=["x", "x"]),
                       lambda r: r[0].update(adapter="x" * 1025), lambda r: r[3].update(extra=float("nan")),
                       lambda r: r[3].update(extra="x" * 17000),
                       lambda r: r[3].update(markers=[{"kind": "event", "id": "x", "t_ns": 0}] * 9)):
            self.assertEqual(self.consume(change)["outcome"], "invalid")

    def test_uncertain_terminal_message_cannot_be_overridden_by_complete_trailer(self):
        def uncertain(lines):
            path = re.search(r"to (.+) \(frames\)", lines[0])[1]
            return [*lines, f"frame telemetry: {path} trailer in progress, final status unavailable"]
        self.assertEqual(self.consume(log_transform=uncertain)["outcome"], "flush_unconfirmed")

    def test_runtime_revision_absence_reasons_remain_explicit(self):
        for reason in ("checkout_changed_since_build", "source_checkout_unavailable", "git_unavailable"):
            def change(r):
                r[0]["engine_revision"] = None
                r[0]["unavailable"]["engine_revision"] = reason
            result = self.consume(change)
            self.assertEqual(result["outcome"], "valid")
            self.assertEqual(result["header"]["unavailable"]["engine_revision"], reason)

    def test_terminal_trailer_unavailable_forms(self):
        for ending in ("truncated (shutdown_timeout), trailer unavailable",
                       "truncated (writer_error), trailer unavailable: synthetic failure"):
            def change(lines):
                path = re.search(r"to (.+) \(frames\)", lines[0])[1]
                return [*lines, f"frame telemetry: {path} {ending}"]
            self.assertEqual(self.consume(log_transform=change)["outcome"], "flush_unconfirmed")


class StatisticsTest(unittest.TestCase):
    def test_known_distribution_ranks_and_strict_threshold_equality(self):
        stats = telemetry.statistics([{"duration_ns": n * 1000000} for n in (10, 20, 30, 40, 100)])
        self.assertEqual((stats["p50_ms"], stats["p95_ms"], stats["p99_ms"]), (30, 100, 100))
        self.assertEqual((stats["above_33_ms"], stats["above_50_ms"], stats["above_100_ms"]), (2, 1, 0))
        equal = telemetry.statistics([{"duration_ns": n * 1000000} for n in (33, 50, 100, 250)])
        self.assertEqual([equal[f"above_{n}_ms"] for n in (33, 50, 100, 250)], [3, 2, 1, 0])

    def test_n_99_100_199_200_preserve_floor_tail_and_nearest_rank(self):
        for n in (99, 100, 199, 200):
            stats = telemetry.statistics([{"duration_ns": x * 1000000} for x in range(1, n + 1)])
            self.assertEqual(stats["p99_ms"], (99 * n + 99) // 100)
            self.assertEqual(stats["slowest_1pct_mean_ms"], n if n < 200 else 199.5)

    def test_empty_null_and_later_zero_durations(self):
        self.assertIsNone(telemetry.statistics([])["p99_ms"])
        stats = telemetry.statistics([{"duration_ns": None}, {"duration_ns": 0}])
        self.assertEqual((stats["samples"], stats["frames_written"], stats["p99_ms"]), (1, 2, 0))

    def test_interruption_during_measurement_is_insufficient(self):
        frames = [r for r in records() if r["type"] == "frame"]
        frames[200]["phase"] = "loading"
        self.assertFalse(telemetry.window(frames, 48_000_000_000, 10, 30)["available"])

    def test_settle_restarts_before_measurement_and_first_gameplay_is_skipped(self):
        frames = [r for r in records() if r["type"] == "frame"]
        frames[20]["phase"] = "loading"
        result = telemetry.window(frames, 60_000_000_000, 10, 30)
        self.assertTrue(result["available"])
        self.assertEqual(result["start_ns"], frames[21]["t_ns"] + 10_000_000_000)

    def test_short_first_gameplay_segment_does_not_hide_a_later_complete_window(self):
        frames = [r for r in records() if r["type"] == "frame"]
        frames[100]["phase"] = "loading"
        result = telemetry.window(frames, 60_000_000_000, 10, 30)
        self.assertTrue(result["available"])
        self.assertEqual(result["start_ns"], frames[101]["t_ns"] + 10_000_000_000)


class VerdictTest(unittest.TestCase):
    def test_threshold_and_absolute_floor_equality_are_noise(self):
        for base, candidate, rule in ((100, 110, (0.10, 0.1, "ms")), (0, 0.05, (0.03, 0.05, "ms"))):
            self.assertEqual(comparison.judge([base] * 3, [candidate] * 3, rule)["verdict"], "no change beyond noise")

    def test_spread_from_both_sides_suppresses_change(self):
        for base, candidate in (([8, 10, 12], [11, 11, 11]), ([10] * 3, [10, 14, 18])):
            self.assertEqual(comparison.judge(base, candidate, (0.03, 0.05, "ms"))["verdict"], "no change beyond noise")

    def test_zero_baseline_and_direction(self):
        row = comparison.judge([0] * 3, [1] * 3, (0.03, 0.05, "ms"))
        self.assertEqual(row["verdict"], "regression")
        self.assertIsNone(row["change_percent"])
        self.assertEqual(comparison.judge([1] * 3, [0] * 3, (0.03, 0.05, "ms"))["verdict"], "improvement")

    def test_fewer_than_three_or_missing_values_are_unavailable(self):
        self.assertEqual(comparison.judge([1, 1], [1] * 3, (0.03, 0.05, "ms"))["verdict"], "unavailable")
        self.assertEqual(comparison.summary([None, float("nan")])["count"], 0)

    def test_abba_three_and_four_repetitions_and_warmups(self):
        for n, expected in ((3, "ABBAAB"), (4, "ABBAABBA")):
            plan = comparison.launch_plan(n)
            self.assertEqual([p["side"] for p in plan[:2]], ["base", "candidate"])
            self.assertTrue(all(p["warmup"] for p in plan[:2]))
            self.assertEqual("".join("A" if p["side"] == "base" else "B" for p in plan[2:]), expected)
            self.assertEqual(len({p["output"] for p in plan}), 2 + n * 2)

    def rows(self):
        metrics = {key: 1 for key in comparison.THRESHOLDS}
        return [{**p, "included": not p["warmup"], "status": "ok", "metrics": metrics,
                 "telemetry": {"header": {}}, "run_bad": 0} for p in comparison.launch_plan()]

    def test_overall_no_change_inconclusive_mixed_and_regression(self):
        rows = self.rows()
        def index():
            return comparison.build_index([], rows, "battle", {}, {}, True)
        self.assertEqual(index()["verdict"], "no change")
        rows[2]["included"] = False
        rows[5]["included"] = False
        self.assertEqual(index()["verdict"], "inconclusive")
        rows = self.rows()
        for r in rows:
            r["metrics"] = {**r["metrics"], "mean_ms": 2 if r["side"] == "candidate" else 1}
        self.assertEqual(index()["verdict"], "regression")
        for r in rows:
            r["metrics"] = {**r["metrics"], "p99_ms": 0 if r["side"] == "candidate" else 1}
        self.assertEqual(index()["verdict"], "mixed")
        rows[0]["status"] = "hung"
        self.assertEqual(index()["verdict"], "inconclusive")

    def test_effective_metadata_mismatch_and_failed_checks_are_inconclusive(self):
        rows = self.rows()
        rows[2]["telemetry"] = {"header": {"width_px": 2000}}
        index = comparison.build_index([], rows, "battle", {}, {}, True)
        self.assertEqual(index["verdict"], "inconclusive")
        self.assertIn("presentation", index["problems"][0])
        rows = self.rows()
        rows[2]["run_bad"] = 1
        self.assertEqual(comparison.build_index([], rows, "battle", {}, {}, True)["verdict"], "inconclusive")

    def test_cpu_is_required_only_for_a_profile_comparison(self):
        rows = self.rows()
        for row in rows:
            row["metrics"] = {k: v for k, v in row["metrics"].items() if k != "cpu_ms"}
        self.assertEqual(comparison.build_index([], rows, "battle", {}, {}, True)["verdict"], "no change")
        self.assertEqual(comparison.build_index([], rows, "battle", {}, {"profile": True}, True)["verdict"], "inconclusive")


class IntegrationTest(unittest.TestCase):
    def arguments(self, root, extra=()):
        base, candidate = root / "base.bin", root / "candidate.bin"
        base.write_bytes(b"identical synthetic bytes")
        candidate.write_bytes(base.read_bytes())
        return cli.parser().parse_args(["compare", "battle", "--exe-base", str(base), "--exe-candidate", str(candidate), *extra])

    def fake_run(self, args, ctx):
        self.assertIsNone(ctx.locks)
        aids = dict(kv.split("=", 1) for kv in args.aid)
        self.assertEqual(aids["NVF_PLAY_TEST_FRAME_TELEMETRY"], "60")
        run_id, scenario_id = (aids[f"NVF_PLAY_TEST_FRAME_TELEMETRY_{suffix}"] for suffix in ("RUN_ID", "SCENARIO_ID"))
        self.assertRegex(run_id, r"^[A-Za-z0-9_-]{1,64}$")
        _, lines = save_capture(ctx.out / "battle-10.home", records(run_id, scenario_id))
        lines += ["stress battle: group 1 fixture-a (0x00000001): 10 of 10 placed around (0, 0)",
                  "stress battle: group 2 fixture-b (0x00000002): 10 of 10 placed around (0, 0)",
                  "stress battle: 2 groups, 20 fighters, 20 set on the nearest enemy"]
        log = ctx.out / "battle-10.log"
        log.write_text("\n".join(f"\x1b[32m{line}\x1b[0m" for line in lines), encoding="utf-8")
        log.with_suffix(".exit.json").write_text(json.dumps({"started_at_utc": "2026-11-01T12:00:00Z",
                                                             "finished_at_utc": "2026-11-01T12:00:48Z", "seconds": 48}), encoding="utf-8")
        return {"status": "ok", "frames": {"count": 500, "mean_ms": 10, "low1_ms": 20, "worst_ms": 300},
                "machine": {"busy_fraction": 0}, "log": str(log), "manifest": {"runner": {"source": "runner identity"}}}, ["# synthetic"], 0

    def test_sequence_holds_outer_lock_discards_warmups_and_writes_reports(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            args = self.arguments(root)
            lock = mock.MagicMock()
            ctx = lib.Ctx(root, args.exe_base, "Data", root / "unused", root, lambda *_: None, lock)
            with mock.patch.object(comparison, "write", wraps=comparison.write) as written:
                self.assertEqual(comparison.run(args, ctx, self.fake_run, cli.comparison_quality), 0)
            lock.__enter__.assert_called_once()
            lock.__exit__.assert_called_once()
            index = json.loads((root / "comparison.json").read_text())
            self.assertEqual(index["schema_version"], 2)
            self.assertEqual(index["verdict"], "no change")
            self.assertEqual(len(index["runs"]), 10)
            self.assertEqual(sum(r["included"] for r in index["runs"]), 8)
            self.assertTrue(all(call.args[1]["status"] == "incomplete" for call in written.call_args_list[:-1]))
            self.assertEqual(index["runs"][2]["manifest"]["engine"]["source_revision"], "engine-commit")
            self.assertEqual(index["runs"][2]["timing_source"], "launch exit record")
            self.assertEqual(index["runs"][2]["elapsed_seconds"], 48)
            self.assertEqual(index["runs"][2]["manifest"]["cache_state"]["os_file_cache"], "unknown")
            self.assertIn("median of per-run p99", (root / "comparison.md").read_text())
            self.assertTrue((root / "run-001" / "telemetry-validation.json").exists())

    def test_interruption_retains_rows_and_incomplete_plan(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            args = self.arguments(root)
            ctx = lib.Ctx(root, args.exe_base, "Data", root / "unused", root, lambda *_: None)
            calls = 0
            def stop(a, child):
                nonlocal calls
                calls += 1
                if calls == 3:
                    raise KeyboardInterrupt
                return self.fake_run(a, child)
            self.assertEqual(comparison.run(args, ctx, stop, cli.comparison_quality), 1)
            index = json.loads((root / "comparison.json").read_text())
            self.assertEqual((index["status"], len(index["runs"]), len(index["plan"])), ("incomplete", 2, 10))

    def test_changed_executable_makes_comparison_inconclusive(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            args = self.arguments(root)
            ctx = lib.Ctx(root, args.exe_base, "Data", root / "unused", root, lambda *_: None)
            def mutate(a, child):
                answer = self.fake_run(a, child)
                if child.out.name == "run-010":
                    args.exe_base.write_bytes(b"changed")
                return answer
            self.assertEqual(comparison.run(args, ctx, mutate, cli.comparison_quality), 1)
            self.assertIn("base executable changed", " ".join(json.loads((root / "comparison.json").read_text())["problems"]))

    def test_busy_unknown_failed_and_missing_captures_retain_coarse_rows(self):
        for kind in ("busy", "unknown", "failed", "missing"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                args = self.arguments(root)
                ctx = lib.Ctx(root, args.exe_base, "Data", root / "unused", root, lambda *_: None)
                def unfit(a, child):
                    result, lines, bad = self.fake_run(a, child)
                    if kind == "busy":
                        result["machine"] = {"busy_fraction": 0.5}
                    elif kind == "unknown":
                        result["machine"] = None
                    elif kind == "failed":
                        result["status"] = "crashed"
                        bad = 1
                    else:
                        for p in (child.out / "battle-10.home" / "logs").glob("*.jsonl"):
                            p.unlink()
                    return result, lines, bad
                self.assertEqual(comparison.run(args, ctx, unfit, cli.comparison_quality), 1)
                index = json.loads((root / "comparison.json").read_text())
                self.assertEqual(index["verdict"], "inconclusive")
                self.assertEqual(len(index["runs"]), 10)
                self.assertTrue(all(r["exclusions"] for r in index["runs"]))
                self.assertTrue(all(r["legacy_frames"]["low1_ms"] == 20 for r in index["runs"]))

    def test_changed_ini_and_different_route_are_reported(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            ini = root / "settings.ini"
            ini.write_text("synthetic settings")
            args = self.arguments(root, ("--ini", str(ini)))
            ctx = lib.Ctx(root, args.exe_base, "Data", root / "unused", root, lambda *_: None, ini=ini)
            def mutate(a, child):
                result = self.fake_run(a, child)
                (child.out / "route.txt").write_text("different" if child.out.name == "run-010" else "same")
                if child.out.name == "run-010":
                    ini.write_text("changed")
                return result
            self.assertEqual(comparison.run(args, ctx, mutate, cli.comparison_quality), 1)
            problems = " ".join(json.loads((root / "comparison.json").read_text())["problems"])
            self.assertIn("INI changed", problems)
            self.assertIn("route or event schedule differs", problems)

    def test_parser_defaults_and_rejections(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            args = self.arguments(root)
            self.assertEqual((args.repetitions, args.warmup, args.order), (4, 1, "ABBA"))
            self.assertEqual((args.settle_seconds, args.measure_seconds, args.telemetry_seconds), (10, 30, 60))
            for extra in (("--repetitions", "2"), ("--order", "AB"), ("--settle-seconds", "nan"), ("--warmup", "-1")):
                with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                    self.arguments(root, extra)

    def test_missing_battle_placement_prevents_a_no_change_verdict(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            args = self.arguments(root)
            ctx = lib.Ctx(root, args.exe_base, "Data", root / "unused", root, lambda *_: None)
            def incomplete(a, child):
                result = self.fake_run(a, child)
                log = Path(result[0]["log"])
                log.write_text("\n".join(line for line in log.read_text().splitlines()
                                         if "stress battle:" not in line), encoding="utf-8")
                return result
            self.assertEqual(comparison.run(args, ctx, incomplete, cli.comparison_quality), 1)
            index = json.loads((root / "comparison.json").read_text())
            self.assertEqual(index["verdict"], "inconclusive")
            self.assertTrue(all(r["run_bad"] and not r["included"] for r in index["runs"]))
            self.assertEqual(index["metrics"]["p99_ms"]["excluded"], {"base": 4, "candidate": 4})

    def test_scenario_completion_requires_full_laps_and_placement_counts(self):
        a = cli.parser().parse_args(["compare", "tour", "--exe-base", "a", "--exe-candidate", "b", "--laps", "3"])
        self.assertEqual(comparison.scenario_failures(a, {"laps_done": 3}, []), [])
        self.assertTrue(comparison.scenario_failures(a, {"laps_done": 2}, []))
        a = cli.parser().parse_args(["compare", "battle", "--exe-base", "a", "--exe-candidate", "b"])
        lines = ["stress battle: group 1 fixture-a (0x00000001): 9 of 10 placed around (0, 0)",
                 "stress battle: group 2 fixture-b (0x00000002): 10 of 10 placed around (0, 0)",
                 "stress battle: 2 groups, 19 fighters, 19 set on the nearest enemy"]
        self.assertTrue(comparison.scenario_failures(a, {}, lines))

    def test_same_path_is_refused_before_launch(self):
        with tempfile.TemporaryDirectory() as td:
            root = Path(td)
            args = self.arguments(root)
            args.exe_candidate = args.exe_base
            with self.assertRaises(SystemExit), mock.patch.object(cli, "context", side_effect=AssertionError("must not create output")):
                cli.main(["compare", "battle", "--exe-base", str(args.exe_base), "--exe-candidate", str(args.exe_base)])

    def test_registered_future_scene_uses_generic_parser_and_dispatch(self):
        configured = {"configure": lambda p: p.add_argument("--aid", action="append", default=[]),
                      "run": lambda a, ctx: ({"status": "ok", "frames": None}, ["# fake scene"], 0)}
        with mock.patch.dict(cli.EXTRA_TESTS, {"synthetic-scene": configured}), mock.patch.object(cli, "collect_manifest", return_value={}):
            args = cli.parser().parse_args(["compare", "synthetic-scene", "--exe-base", "a", "--exe-candidate", "b"])
            ctx = lib.Ctx(Path("."), Path("a"), "Data", Path("tool"), Path("."), lambda *_: None)
            self.assertEqual(cli.run_once(args, ctx)[0]["status"], "ok")


class ProfileTest(unittest.TestCase):
    def test_profile_windows_weight_frames_and_omit_missing_spans(self):
        capture = {"header": {"started_at_utc": "2026-11-01T12:00:00Z"},
                   "window": {"available": True, "start_ns": 10_000_000_000, "end_ns": 40_000_000_000}}
        lines = [f"2026-11-01T12:00:{s:02d}Z INFO profile: 5.0 s, {n} frames, frame mean 10.0 ms, cpu {cpu} ms a frame; per frame ms: {spans}"
                 for s, n, cpu, spans in ((15, 10, 1, "a 1.0"), (20, 10, 2, "a 2.0, b 3.0"), (25, 30, 4, "a 4.0"),
                                          (40, 10, 9, "a 9.0"))]
        result = comparison.profile_values(lines, capture, True)
        self.assertEqual(result["cpu_ms"], 3.5)
        self.assertEqual(result["spans"], {"a": 3.5})
        self.assertEqual(result["frames"], 40)
        self.assertIsNone(comparison.profile_values(lines, capture, False)["cpu_ms"])


if __name__ == "__main__":
    unittest.main()
