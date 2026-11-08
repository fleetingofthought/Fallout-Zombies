//! A start-up read that leaves the main thread (#1732): a table made only from the plugins,
//! on a thread of its own, its resource inserted when it is done.
//!
//! The Startup schedule ends with its slowest system, and the first frame waits for that.
//! Its systems already run on the task pool beside each other, so moving one is not about
//! the main thread being busy: it is that nothing downstream should wait for this one
//! [observed: `music: ... read in 805 ms`, `radio: ... read in 713 ms`, `sfx: ... read in
//! 306 ms`, overlapping in one Goodsprings start's log]. A thread of its own rather than
//! the pool: the async compute pool compiles the pipelines the first picture waits for,
//! and physics' loader is on its own thread for the same reason (`phys/loader.rs`).
//!
//! Fit for this only: a read needing nothing but `PlayData`'s `'static` plugins, whose
//! every reader takes the resource as an `Option` and only skips its work without it.

use bevy::prelude::*;
use std::thread::JoinHandle;

/// `T` being read on its thread.
#[derive(Resource)]
pub struct Reading<T: Send + 'static>(Option<JoinHandle<T>>);

/// Read `T` on a thread named `name`; [`collect`] inserts it. If no thread can be started
/// it is read here, as it was before.
pub fn start<T: Resource>(commands: &mut Commands, name: &str, read: impl FnOnce() -> T + Send + Copy + 'static) {
    match std::thread::Builder::new().name(name.to_string()).spawn(read) {
        Ok(thread) => commands.insert_resource(Reading(Some(thread))),
        Err(e) => {
            warn!("{name}: cannot start its thread ({e}); reading on this one");
            commands.insert_resource(read());
        }
    }
}

/// Insert `T` once its thread is done. A panic there has written its crash report (crash.rs);
/// the game goes on without `T`, which its readers allow.
pub fn collect<T: Resource>(mut commands: Commands, mut reading: ResMut<Reading<T>>) {
    if !reading.0.as_ref().is_some_and(JoinHandle::is_finished) {
        return;
    }
    commands.remove_resource::<Reading<T>>();
    match reading.0.take().map(JoinHandle::join) {
        Some(Ok(value)) => commands.insert_resource(value),
        _ => warn!("a start-up read stopped with a panic: {} is missing this run", std::any::type_name::<T>()),
    }
}

/// [`collect`] for `T`, each frame while it is being read.
pub fn add_collect<T: Resource>(app: &mut App) {
    app.add_systems(PreUpdate, collect::<T>.run_if(resource_exists::<Reading<T>>));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Debug, PartialEq)]
    struct Table(u32);

    fn until_gone(app: &mut App) {
        let t0 = std::time::Instant::now();
        while app.world().contains_resource::<Reading<Table>>() {
            assert!(t0.elapsed().as_secs() < 10, "the read never ended");
            app.update();
        }
    }

    #[test]
    fn the_table_arrives_and_the_frames_go_on_meanwhile() {
        let mut app = App::new();
        add_collect::<Table>(&mut app);
        app.add_systems(Startup, |mut commands: Commands| {
            start(&mut commands, "nvf-test-read", || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Table(7)
            });
        });
        app.update();
        assert!(!app.world().contains_resource::<Table>(), "not there while it is being read");
        until_gone(&mut app);
        // `collect` queued it; its commands ran in that frame.
        assert_eq!(app.world().get_resource::<Table>(), Some(&Table(7)));
    }

    #[test]
    fn a_read_that_panics_leaves_the_game_without_it() {
        let mut app = App::new();
        add_collect::<Table>(&mut app);
        app.add_systems(Startup, |mut commands: Commands| {
            start(&mut commands, "nvf-test-read", || -> Table { panic!("this read stops here on purpose") });
        });
        app.update();
        until_gone(&mut app);
        assert!(!app.world().contains_resource::<Table>());
    }
}
