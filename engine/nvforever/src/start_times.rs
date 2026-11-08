//! Where a start's first seconds go (#1732): what the main world's Startup schedules and its
//! first frame take, and when the first render frame ends. The log's times give the rest.
//!
//! Log, every run: `start: Startup <s> s; frame 1 <s> s: <s> s before Update, <s> s in
//! Update, <s> s after it` and `start: first render frame finished <s> s after frame 1
//! ended` (with a pipeline gate: the runs that have one).
//!
//! **What this cannot say** is which system inside took the time: systems run side by side,
//! and Bevy's per-system spans need its `trace` feature (`--features profile`,
//! `NVF_PLAY_TEST_PROFILE=1`, profile.rs). One log has about 1 s in frame 1 with no line at
//! all, between `grab:` and `imposters:` [observed: Goodsprings start, #1716's oscap runs].
//!
//! Marks are small schedules around the main ones, as handoff.rs's `EndNow` is. After the
//! report each is one early return a frame.

use crate::pipeline_gate::PipelineGate;
use bevy::ecs::schedule::{ScheduleLabel, SingleThreadedExecutor};
use bevy::prelude::*;
use std::time::Instant;

#[derive(ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Mark {
    StartupBegins,
    StartupEnds,
    UpdateBegins,
    UpdateEnds,
    FrameEnds,
}

/// When each mark was passed in the first frame.
#[derive(Resource, Default)]
struct Times {
    startup: Option<Instant>,
    frame: Option<Instant>,
    update: Option<Instant>,
    update_end: Option<Instant>,
    frame_end: Option<Instant>,
    done: bool,
}

/// The line for Startup and frame 1, from seconds.
fn line(startup: f64, before: f64, update: f64, after: f64) -> String {
    format!("start: Startup {startup:.2} s; frame 1 {:.2} s: {before:.2} s before Update, {update:.2} s in Update, {after:.2} s after it", before + update + after)
}

fn secs(from: Option<Instant>, to: Option<Instant>) -> f64 {
    from.zip(to).map_or(0.0, |(a, b)| b.duration_since(a).as_secs_f64())
}

fn startup_begins(mut t: ResMut<Times>) {
    t.startup = Some(Instant::now());
}

fn startup_ends(mut t: ResMut<Times>) {
    t.frame = Some(Instant::now());
}

fn update_begins(mut t: ResMut<Times>) {
    if t.update.is_none() {
        t.update = Some(Instant::now());
    }
}

fn update_ends(mut t: ResMut<Times>) {
    if t.update_end.is_none() {
        t.update_end = Some(Instant::now());
    }
}

fn frame_ends(mut t: ResMut<Times>, gate: Option<Res<PipelineGate>>) {
    if t.done {
        return;
    }
    let now = Instant::now();
    if t.frame_end.is_none() {
        t.frame_end = Some(now);
        info!("{}", line(secs(t.startup, t.frame), secs(t.frame, t.update), secs(t.update, t.update_end), secs(t.update_end, Some(now))));
    }
    match gate {
        Some(gate) if gate.frames() == 0 => {}
        Some(_) => {
            info!("start: first render frame finished {:.2} s after frame 1 ended", secs(t.frame_end, Some(now)));
            t.done = true;
        }
        None => t.done = true,
    }
}

pub struct StartTimesPlugin;

impl Plugin for StartTimesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Times>();
        for mark in [Mark::StartupBegins, Mark::StartupEnds, Mark::UpdateBegins, Mark::UpdateEnds, Mark::FrameEnds] {
            app.init_schedule(mark);
            app.edit_schedule(mark, |s| {
                s.set_executor(SingleThreadedExecutor::new());
            });
        }
        let mut order = app.world_mut().resource_mut::<bevy::app::MainScheduleOrder>();
        order.insert_startup_before(PreStartup, Mark::StartupBegins);
        order.insert_startup_after(PostStartup, Mark::StartupEnds);
        order.insert_before(Update, Mark::UpdateBegins);
        order.insert_after(Update, Mark::UpdateEnds);
        order.insert_after(Last, Mark::FrameEnds);
        app.add_systems(Mark::StartupBegins, startup_begins)
            .add_systems(Mark::StartupEnds, startup_ends)
            .add_systems(Mark::UpdateBegins, update_begins)
            .add_systems(Mark::UpdateEnds, update_ends)
            .add_systems(Mark::FrameEnds, frame_ends);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_adds_up_the_frame() {
        assert_eq!(line(0.81, 0.2, 1.5, 0.1), "start: Startup 0.81 s; frame 1 1.80 s: 0.20 s before Update, 1.50 s in Update, 0.10 s after it");
    }

    #[test]
    fn the_marks_come_in_order_and_are_reported_once() {
        let mut app = App::new();
        app.add_plugins(StartTimesPlugin);
        app.update();
        app.update();
        let t = app.world().resource::<Times>();
        let at = [t.startup, t.frame, t.update, t.update_end, t.frame_end].map(|m| m.expect("every mark passed"));
        assert!(at.windows(2).all(|w| w[0] <= w[1]), "in order");
        assert!(t.done, "no render world to wait for here");
    }
}
