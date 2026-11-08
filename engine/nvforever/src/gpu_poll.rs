//! wgpu's clean-up of finished GPU work, moved off the render thread (#1427).
//!
//! [documented: wgpu-core 30.0.1 `device/queue.rs` `Queue::submit`, `device/life.rs`
//! `triage_submissions`] Each `Queue::submit` ends by polling the device: the submissions
//! the GPU has finished since the last poll are dropped there, with every resource
//! reference their command buffers held ("drop command buffer trackers"). Bevy polls the
//! device nowhere else, so that work runs on the render thread inside every frame's
//! `queue_submit`. [observed 2026-11-03, docs/design/frame-rate-audit.md §5] It was about
//! 0.8 ms of `queue_submit`'s 1.2 ms at Goodsprings.
//!
//! A thread of its own polls the device (without waiting) every millisecond, so a frame's
//! submissions are dropped soon after the GPU finishes them, while the render thread
//! records the next frame, and `queue_submit` finds them done. The work and the callbacks
//! (buffer mappings, work-done notices) are the same; they only run on this thread.
//! Nothing drawn changes.

use bevy::prelude::*;
use bevy::render::renderer::RenderDevice;
use bevy::render::{Render, RenderApp, RenderSystems};
use std::time::Duration;

/// How long the thread sleeps between polls.
const PERIOD: Duration = Duration::from_millis(1);

pub struct GpuPollPlugin;

impl Plugin for GpuPollPlugin {
    fn build(&self, app: &mut App) {
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render.add_systems(Render, start_poll.in_set(RenderSystems::Cleanup));
        }
    }
}

/// Starts the polling thread once the render device is there.
fn start_poll(device: Res<RenderDevice>, mut started: Local<bool>) {
    if std::mem::replace(&mut *started, true) {
        return;
    }
    let device = device.wgpu_device().clone();
    let spawned = std::thread::Builder::new().name("gpu poll".into()).spawn(move || loop {
        std::thread::sleep(PERIOD);
        if let Err(e) = device.poll(wgpu::PollType::Poll) {
            warn!("gpu poll: stopped, {e}");
            return;
        }
    });
    if let Err(e) = spawned {
        warn!("gpu poll: no thread, {e}");
    }
}
