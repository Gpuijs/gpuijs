//! gpuijs patch: an opt-in breakdown of where each frame's time goes. With `GPUIJS_PROFILE` set,
//! each draw (build, layout, paint) and each present (handing the scene to the GPU) prints one
//! line to stderr, and the GPU prints one per frame it finishes.

use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Whether `GPUIJS_PROFILE` is set.
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("GPUIJS_PROFILE").is_some())
}

#[derive(Default)]
struct Spans {
    /// Exclusive time per span name since the last frame, in first-seen order.
    totals: Vec<(&'static str, Duration)>,
    /// Open spans: name, start, and time spent in spans nested inside.
    open: Vec<(&'static str, Instant, Duration)>,
}

thread_local! {
    static SPANS: RefCell<Spans> = RefCell::default();
}

/// Runs `f`, counting its time towards `name`, less what nested spans count. Re-entering an open
/// span counts towards the outer one.
pub fn time<R>(name: &'static str, f: impl FnOnce() -> R) -> R {
    let opened = enabled()
        && SPANS.with_borrow_mut(|s| {
            let fresh = s.open.iter().all(|(n, _, _)| *n != name);
            if fresh {
                s.open.push((name, Instant::now(), Duration::ZERO));
            }
            fresh
        });
    let result = f();
    if opened {
        SPANS.with_borrow_mut(|s| {
            let Some((name, start, nested)) = s.open.pop() else {
                return;
            };
            let elapsed = start.elapsed();
            if let Some(parent) = s.open.last_mut() {
                parent.2 += elapsed;
            }
            let own = elapsed.saturating_sub(nested);
            match s.totals.iter_mut().find(|(n, _)| *n == name) {
                Some(total) => total.1 += own,
                None => s.totals.push((name, own)),
            }
        });
    }
    result
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.
}

/// Prints the spans since the last line for a `what` (draw or present) that took `total`: each
/// span's own time, then what no span covered. Spans named with a trailing `*` ran between
/// frames, outside `total`.
pub(crate) fn end_frame(what: &str, total: Duration) {
    if !enabled() {
        return;
    }
    let totals = SPANS.with_borrow_mut(|s| std::mem::take(&mut s.totals));
    let covered: Duration = totals
        .iter()
        .filter(|(name, _)| !name.ends_with('*'))
        .map(|(_, d)| *d)
        .sum();
    let parts: Vec<String> = totals
        .iter()
        .map(|(name, d)| format!("{name} {:.2}", ms(*d)))
        .collect();
    eprintln!(
        "[gpuijs profile] {what} {:.2} ms: {}, other {:.2}",
        ms(total),
        parts.join(", "),
        ms(total.saturating_sub(covered)),
    );
}

static TRACK_FRAMES: AtomicBool = AtomicBool::new(false);
static FRAMES_SUBMITTED: AtomicU64 = AtomicU64::new(0);
static FRAMES_COMPLETED: Mutex<VecDeque<(u64, Instant)>> = Mutex::new(VecDeque::new());
/// Completions kept for `frame_completed_at`: a waiter asks within a few frames.
const COMPLETIONS_KEPT: usize = 16;

/// Starts recording when the GPU finishes each frame, for `frame_completed_at` (benchmarks
/// timing input to pixels). Off by default: it costs a lock per frame.
pub fn track_frames() {
    TRACK_FRAMES.store(true, Ordering::Relaxed);
}

/// How many frames have been handed to the GPU; the next one is this plus one.
pub fn frames_submitted() -> u64 {
    FRAMES_SUBMITTED.load(Ordering::Relaxed)
}

/// When the GPU finished frame `frame` (counted by `frames_submitted`), once it has and while
/// it is recent; frames before `track_frames` aren't recorded.
pub fn frame_completed_at(frame: u64) -> Option<Instant> {
    let completed = FRAMES_COMPLETED.lock().ok()?;
    completed
        .iter()
        .find(|(f, _)| *f == frame)
        .map(|(_, at)| *at)
}

/// Numbers a frame handed to the GPU.
pub(crate) fn frame_submitted() -> u64 {
    FRAMES_SUBMITTED.fetch_add(1, Ordering::Relaxed) + 1
}

/// Records that the GPU finished `frame`.
pub(crate) fn frame_completed(frame: u64) {
    if !TRACK_FRAMES.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut completed) = FRAMES_COMPLETED.lock() {
        if completed.len() == COMPLETIONS_KEPT {
            completed.pop_front();
        }
        completed.push_back((frame, Instant::now()));
    }
}

/// Prints how long the GPU took on a frame.
pub(crate) fn gpu_frame(duration_secs: f64) {
    eprintln!("[gpuijs profile] gpu {:.2} ms", duration_secs * 1000.);
}
