//! xNVSE's delayed calls (task D3's rest; docs/handoff/xnvse-remaining.md §4):
//! CallAfterSeconds, CallAfterFrames, CallForSeconds, CallWhile, CallWhen and
//! CallWhilePerSeconds [documented: their pages]. Not saved: "Every queued call is removed
//! when loading the game" [documented], so a load starts with none. Run from
//! `Game::tick`; seconds are game seconds (`Game::time`), which the time mult affects
//! [documented: "not counted in real-life seconds"].

use crate::game::{Game, RunCtx};
use crate::trace::EventKind;
use fnv_script::vm::{Value, XVal};
use fnv_sim::FormId;

/// What decides when a queued call runs.
#[derive(Debug, Clone, PartialEq)]
enum When {
    /// Once, when the game time reaches the value.
    AfterSeconds(f64),
    /// Once, at that frame.
    AfterFrames(u64),
    /// Every frame until the game time reaches the value.
    ForSeconds(f64),
    /// Every frame while the condition gives true; dropped the first time it does not.
    While(XVal),
    /// Once, the first frame the condition gives true.
    Once(XVal),
    /// Every `interval` seconds while the condition gives true.
    WhilePerSeconds { cond: XVal, interval: f64, next: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Delayed {
    when: When,
    func: XVal,
    on: Option<FormId>,
    args: Vec<XVal>,
    /// For the timers: runWhen 0 game mode only, 1 also menu mode, 2 also menu mode but
    /// paused in the pause and console menus [documented]. For the conditions: the flags
    /// (1 args to the function, 2 args to the condition, 4 not in menu mode, 8 not in game
    /// mode, 16 not while paused) [documented].
    mode: u32,
    condition_flags: bool,
}

impl Delayed {
    /// Whether it may run now.
    fn runs_now(&self, menu: bool) -> bool {
        if self.condition_flags {
            !(menu && self.mode & 4 != 0) && !(!menu && self.mode & 8 != 0)
        } else {
            !menu || self.mode >= 1
        }
    }
}

/// The delayed-call functions; `None` for any other name.
pub(crate) fn call(g: &mut Game, ctx: &RunCtx, name: &str, on: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let on = on.or(ctx.self_ref);
    let num = |k: usize| match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => 0.0,
    };
    let item = |when: When, func: usize, mode: u32, rest: usize, condition_flags: bool| Delayed {
        when,
        func: args.get(func).cloned().unwrap_or(XVal::Num(0.0)),
        on,
        args: args.iter().skip(rest).cloned().collect(),
        mode,
        condition_flags,
    };
    let d = match name {
        // `seconds udf [runWhen] args…`; 0 seconds runs next frame [documented].
        "callafterseconds" => item(When::AfterSeconds(g.time + num(0).max(0.0)), 1, num(2) as u32, 3, false),
        // [hypothesis: no page in the export; the catalogue gives frames, udf, runWhen,
        // args, so it is CallAfterSeconds counted in frames].
        "callafterframes" => item(When::AfterFrames(g.frame + num(0).max(0.0) as u64), 1, num(2) as u32, 3, false),
        "callforseconds" => item(When::ForSeconds(g.time + num(0).max(0.0)), 1, num(2) as u32, 3, false),
        // `udf condition [flags] args…`.
        "callwhile" => item(When::While(args.get(1).cloned().unwrap_or(XVal::Num(0.0))), 0, num(2) as u32, 3, true),
        "callwhen" => item(When::Once(args.get(1).cloned().unwrap_or(XVal::Num(0.0))), 0, num(2) as u32, 3, true),
        // `interval udf condition [flags] args…`.
        "callwhileperseconds" => {
            let interval = num(0).max(0.0);
            item(When::WhilePerSeconds { cond: args.get(2).cloned().unwrap_or(XVal::Num(0.0)), interval, next: g.time + interval }, 1, num(3) as u32, 4, true)
        }
        _ => return None,
    };
    // An array argument lives until the calls stop [documented].
    for a in &d.args {
        g.ext.retain(a);
    }
    g.delayed.push(d);
    Some(Ok(XVal::Num(0.0)))
}

/// Call the function `func` with `args` next frame, in a menu too (SUP's dialog boxes
/// answer their handler this way, `crate::nvse_sup5`).
pub(crate) fn call_next_frame(g: &mut Game, func: XVal, on: Option<FormId>, args: Vec<XVal>) {
    for a in &args {
        g.ext.retain(a);
    }
    g.delayed.push(Delayed { when: When::AfterFrames(g.frame + 1), func, on, args, mode: 1, condition_flags: false });
}

impl Game<'_> {
    /// The queued calls due this frame.
    pub(crate) fn run_delayed(&mut self) {
        if self.delayed.is_empty() {
            return;
        }
        let menu = self.menu.is_some() || self.dialogue.is_some();
        let queued = std::mem::take(&mut self.delayed);
        let mut kept = Vec::new();
        for mut d in queued {
            if !d.runs_now(menu) {
                kept.push(d);
                continue;
            }
            let to_func = !d.condition_flags || d.mode & 1 != 0;
            let to_cond = d.condition_flags && d.mode & 2 != 0;
            let (run, keep) = match d.when.clone() {
                When::AfterSeconds(t) => (self.time >= t, self.time < t),
                When::AfterFrames(f) => (self.frame >= f, self.frame < f),
                When::ForSeconds(t) => (self.time < t, self.time < t),
                When::While(c) => {
                    let ok = self.delayed_truth(&c, &d, to_cond);
                    (ok, ok)
                }
                When::Once(c) => {
                    let ok = self.delayed_truth(&c, &d, to_cond);
                    (ok, !ok)
                }
                When::WhilePerSeconds { cond, interval, next } => {
                    if self.time < next {
                        (false, true)
                    } else {
                        let ok = self.delayed_truth(&cond, &d, to_cond);
                        d.when = When::WhilePerSeconds { cond, interval, next: self.time + interval };
                        (ok, ok)
                    }
                }
            };
            if run {
                let args = if to_func { d.args.clone() } else { Vec::new() };
                self.delayed_run(&d.func, d.on, args, "delayed call");
            }
            if keep {
                kept.push(d);
            } else {
                for a in &d.args {
                    self.ext.release(a);
                }
            }
        }
        // Calls queued while these ran come after.
        kept.append(&mut self.delayed);
        self.delayed = kept;
    }

    fn delayed_truth(&mut self, cond: &XVal, d: &Delayed, with_args: bool) -> bool {
        let args = if with_args { d.args.clone() } else { Vec::new() };
        self.delayed_run(cond, d.on, args, "delayed call's condition").is_some_and(|v| v.truthy())
    }

    fn delayed_run(&mut self, func: &XVal, on: Option<FormId>, args: Vec<XVal>, what: &str) -> Option<XVal> {
        let ctx = RunCtx { self_ref: on, ..RunCtx::default() };
        match self.run_function(func, on.map(|f| Value::Ref(f.0)), args, None, ctx) {
            Ok(v) => Some(v),
            Err(error) => {
                self.emit(EventKind::ScriptError { unit: what.into(), error });
                None
            }
        }
    }
}
