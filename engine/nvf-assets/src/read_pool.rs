//! The game's own task pool for reading files and turning them into something usable:
//! cell lists and terrain (`nvforever::world`), models with their textures
//! ([`crate::model`]), actor parts and animation clips ([`crate::actor`]) and the actor
//! read-ahead (`nvforever::readahead`).
//!
//! These used to run on Bevy's async compute pool, which is where render pipelines compile
//! [documented: bevy_render 0.20.1 `render_resource/pipeline_cache.rs`], and the first
//! render frame waits on such a compile [documented: bevy_core_pipeline upscaling, see
//! `nvforever::blit_warm`; that it queued behind the reads is a hypothesis from the pipeline
//! logs, #1732]. Forcing that pool's size changed the start's loading screen from 7.4 s at
//! 1 thread to 3.0 s at 4 [observed: #1732].
//!
//! Why not Bevy's I/O pool, which has the same size: the asset server loads the embedded
//! shaders on it [documented: bevy_asset 0.20.1 `server/mod.rs`], at the same moment the
//! start's reads begin, so compiles would wait behind the reads there instead; and sound
//! decodes went there to stay clear of these reads (`nvf_play::sfx`, `prefetch`) [design].
//!
//! The work is more parsing and decoding than waiting on a disk, so a thread here keeps a
//! core busy and the pool is no larger than the one the reads had: 25 % of the logical
//! threads, 1 to 4, Bevy's rule for the async compute pool [documented: bevy_app 0.20.1
//! `task_pool_plugin.rs`] [design]. Nothing limits how much is queued here beyond what its
//! callers ask for; the tasks are the same `Task`s as on any pool, polled and dropped the
//! same way.
//!
//! Test aid `NVF_PLAY_TEST_READ_THREADS=<n>` (more than 0) sets the size, next to
//! `NVF_PLAY_TEST_ASYNC_THREADS` for the async compute pool (`nvforever` `task_pools`), to
//! compare with a smaller machine's: set both, as a smaller machine would have both smaller.

use bevy::tasks::{available_parallelism, TaskPool, TaskPoolBuilder};
use std::sync::OnceLock;

/// The pool, made on first use (a viewer or a test needs no plugin for it).
pub fn read_pool() -> &'static TaskPool {
    static POOL: OnceLock<TaskPool> = OnceLock::new();
    POOL.get_or_init(|| {
        let aid = std::env::var("NVF_PLAY_TEST_READ_THREADS").ok();
        let threads = thread_count(aid.as_deref(), available_parallelism());
        bevy::log::info!("read pool threads: {threads}");
        TaskPoolBuilder::new().num_threads(threads).thread_name("NVF Read Pool".to_string()).build()
    })
}

/// The pool's size: the test aid's value if it is a number above 0, otherwise a quarter of
/// the `logical` threads, rounded as Bevy rounds, from 1 to 4.
fn thread_count(aid: Option<&str>, logical: usize) -> usize {
    aid.and_then(|v| v.trim().parse().ok()).filter(|&n| n > 0).unwrap_or(((logical + 2) / 4).clamp(1, 4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::tasks::block_on;

    #[test]
    fn a_quarter_of_the_threads_from_one_to_four() {
        for (logical, threads) in [(1, 1), (2, 1), (4, 1), (5, 1), (6, 2), (8, 2), (10, 3), (12, 3), (14, 4), (16, 4), (32, 4), (128, 4)] {
            assert_eq!(thread_count(None, logical), threads, "{logical} logical threads");
        }
    }

    #[test]
    fn the_test_aid_sets_the_size_unless_it_would_leave_no_thread() {
        assert_eq!(thread_count(Some("1"), 32), 1);
        assert_eq!(thread_count(Some("16"), 4), 16);
        assert_eq!(thread_count(Some("0"), 32), 4);
        assert_eq!(thread_count(Some("many"), 8), 2);
    }

    #[test]
    fn tasks_run_on_the_pools_own_threads() {
        let name = block_on(read_pool().spawn(async { std::thread::current().name().map(str::to_string) }));
        assert!(name.is_some_and(|n| n.starts_with("NVF Read Pool")));
    }
}
