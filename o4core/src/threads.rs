//! Thread budget for the heavy (OpenCV) stages.
//!
//! The optical and refine stages run their per-chunk work on a dedicated,
//! bounded rayon pool (never the global pool). One pool per size is shared
//! by the whole process, so GUI jobs that run concurrently with the same
//! budget share the same workers instead of multiplying threads: the
//! budget is a machine-wide cap, and work stealing balances the jobs.
//!
//! Inside a worker every OpenCV call runs single-threaded:
//! - `cv::setNumThreads(1)` (see [`opencv_serial`]) turns off OpenCV's own
//!   parallel_for pool. In OpenCV 4.12 this is process-global state (one
//!   thread count consulted by every parallel region, on any thread; see
//!   the `opencv_serial_is_process_wide` test) and the function is not
//!   thread-safe, so it runs exactly once, before the first parallel
//!   section of the first job.
//! - Each `VideoCapture` opened by a chunk worker asks FFmpeg for a fixed
//!   small number of decode threads ([`DECODE_THREADS`]; FFmpeg's default
//!   is one per core, per capture).
//!
//! Workers run below normal priority on Windows so the desktop (and the
//! GUI's own UI thread, which is not a pool worker) stays responsive, and
//! opt out of EcoQoS so a background repair is not confined to the
//! efficiency cores of a hybrid CPU.
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Once, OnceLock};

/// FFmpeg decode threads per chunk `VideoCapture`. Measured on 0083
/// (docs/experiments/perf-v1/results.md): 2 gave the same wall time for
/// +15% CPU and +2 threads per worker, so parallelism comes from chunks.
pub const DECODE_THREADS: i32 = 1;

/// Frame pairs per unit of parallel work (~1 s at 100 fps). Each chunk pays
/// one seek (decode from an earlier keyframe, ~13% of the optical CPU at
/// 100); 200 saved 5% CPU on 0083 but gave no faster wall time and a
/// coarser tail, which matters more with small thread budgets.
pub const CHUNK_PAIRS: i64 = 100;

fn logical_cores() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Default budget: half the logical cores, at least 2.
pub fn auto_threads() -> usize {
    (logical_cores() / 2).max(2)
}

/// `None` or `Some(0)` = auto.
pub fn resolve(requested: Option<usize>) -> usize {
    match requested {
        Some(n) if n > 0 => n,
        _ => auto_threads(),
    }
}

/// GUI presets: "low" = a quarter of the cores, "high" = all cores minus
/// one, anything else = auto.
pub fn preset(name: &str) -> usize {
    let n = logical_cores();
    match name {
        "low" => (n / 4).max(1),
        "high" => n.saturating_sub(1).max(1),
        _ => auto_threads(),
    }
}

/// The process-wide pool with `n` workers (created on first use).
pub fn pool(n: usize) -> Arc<ThreadPool> {
    static POOLS: OnceLock<Mutex<HashMap<usize, Arc<ThreadPool>>>> = OnceLock::new();
    let n = n.max(1);
    let mut pools = POOLS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    pools
        .entry(n)
        .or_insert_with(|| {
            Arc::new(
                ThreadPoolBuilder::new()
                    .num_threads(n)
                    .thread_name(|i| format!("o4fix-worker-{i}"))
                    .start_handler(|_| lower_priority())
                    .build()
                    .expect("build o4fix worker pool"),
            )
        })
        .clone()
}

/// Turn off OpenCV's internal threading, once per process.
pub fn opencv_serial() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = opencv::core::set_num_threads(1);
    });
}

#[cfg(windows)]
fn lower_priority() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadInformation, SetThreadPriority, ThreadPowerThrottling,
        THREAD_POWER_THROTTLING_CURRENT_VERSION, THREAD_POWER_THROTTLING_EXECUTION_SPEED,
        THREAD_POWER_THROTTLING_STATE, THREAD_PRIORITY_BELOW_NORMAL,
    };
    {
        // Opt the worker out of EcoQoS: when the process is not in the
        // foreground, Windows 11 otherwise confines it to the efficiency
        // cores of a hybrid CPU (measured on an i9-12900K: ~7.7 cores used
        // of a 12-thread pool, wall time up to 2x). Below-normal priority
        // already yields to interactive work.
        let state = THREAD_POWER_THROTTLING_STATE {
            Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: 0,
        };
        // SAFETY: pseudo-handle of the calling thread; `state` is a valid
        // THREAD_POWER_THROTTLING_STATE of the size passed.
        unsafe {
            SetThreadInformation(
                GetCurrentThread(),
                ThreadPowerThrottling,
                &state as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
            );
        }
    }
    // SAFETY: GetCurrentThread returns a pseudo-handle for the calling
    // thread; SetThreadPriority only adjusts that thread's scheduling.
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(not(windows))]
fn lower_priority() {}

/// Split frames `f0..=f1` into chunks of at most `CHUNK_PAIRS` consecutive
/// pairs. Consecutive chunks share their boundary frame, so the chunk pair
/// sets partition the pairs `(f, f+1)` for `f` in `f0..f1` exactly.
pub fn chunks(f0: i64, f1: i64) -> Vec<(i64, i64)> {
    let mut out = vec![];
    let mut a = f0;
    while a < f1 {
        let b = (a + CHUNK_PAIRS).min(f1);
        out.push((a, b));
        a = b;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_partition_pairs() {
        assert!(chunks(5, 5).is_empty());
        assert_eq!(chunks(0, 1), vec![(0, 1)]);
        assert_eq!(chunks(10, 110), vec![(10, 110)]);
        assert_eq!(chunks(10, 111), vec![(10, 110), (110, 111)]);
        for (f0, f1) in [(0, 0), (3, 250), (7, 1007), (0, 99)] {
            let pairs: Vec<i64> = chunks(f0, f1).iter().flat_map(|&(a, b)| a..b).collect();
            assert_eq!(pairs, (f0..f1).collect::<Vec<_>>());
        }
    }

    #[test]
    fn opencv_serial_is_process_wide() {
        opencv_serial();
        let seen = pool(3).install(|| opencv::core::get_num_threads().unwrap());
        assert_eq!(seen, 1, "a worker thread must see OpenCV threading off");
        let seen = std::thread::spawn(|| opencv::core::get_num_threads().unwrap())
            .join()
            .unwrap();
        assert_eq!(seen, 1);
    }

    #[test]
    fn budget_is_bounded() {
        assert!(auto_threads() >= 2);
        assert_eq!(resolve(Some(3)), 3);
        assert_eq!(resolve(Some(0)), auto_threads());
        assert_eq!(resolve(None), auto_threads());
        assert!(preset("low") >= 1 && preset("low") <= preset("auto"));
        assert!(preset("high") >= 1);
        assert_eq!(pool(2).current_num_threads(), 2);
        assert!(Arc::ptr_eq(&pool(2), &pool(2)));
    }
}
