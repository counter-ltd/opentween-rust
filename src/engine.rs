//! The tween engine. Caller-owned, driver-agnostic.
//!
//! [`AnimationEngine`] holds running tweens. The owner drives it by calling
//! [`AnimationEngine::advance`] on whatever cadence suits — a render loop, a
//! fixed timer, anything. The engine never spawns a thread and keeps no global
//! state, so it embeds cleanly in any application.
//!
//! ## Re-entrancy
//!
//! `advance` does **not** invoke callbacks itself — it returns a [`TickOutcome`]
//! for the caller to fire after releasing any lock guarding the engine. This
//! lets a callback start or cancel tweens (e.g. chaining) without deadlocking.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::easing::{apply_easing, Easing};

/// Opaque handle to a running tween.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TweenId(pub u64);

/// Per-frame progress callback. Receives eased `t ∈ [0, 1]`.
pub type TickFn = Arc<dyn Fn(f32) + Send + Sync + 'static>;

/// Fired once after a tween's final tick.
pub type CompleteFn = Box<dyn FnOnce() + Send + 'static>;

struct TweenEntry {
    start: Instant,
    duration_ms: u64,
    easing: Easing,
    on_tick: TickFn,
    on_complete: Option<CompleteFn>,
    owner: Option<String>,
}

/// Read-only view of one running tween, produced by [`AnimationEngine::snapshot`].
#[derive(Debug, Clone)]
pub struct TweenSnapshot {
    pub id: u64,
    pub elapsed_ms: u128,
    pub duration_ms: u64,
    pub easing: &'static str,
    pub owner: Option<String>,
}

/// Callbacks produced by [`AnimationEngine::advance`]. The caller fires these
/// after `advance` returns — never while holding a lock over the engine.
#[derive(Default)]
pub struct TickOutcome {
    /// `(callback, eased_t)` for every tween that is still running this step.
    pub ticks: Vec<(TickFn, f32)>,
    /// Completion callbacks for tweens that finished this step.
    pub completions: Vec<CompleteFn>,
}

impl TickOutcome {
    /// Fire every tick callback, then every completion callback.
    pub fn dispatch(self) {
        for (cb, t) in self.ticks {
            cb(t);
        }
        for cb in self.completions {
            cb();
        }
    }
}

/// A collection of running tweens. See the module docs for the driving contract.
pub struct AnimationEngine {
    tweens: BTreeMap<TweenId, TweenEntry>,
    next_id: u64,
}

impl Default for AnimationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationEngine {
    pub fn new() -> Self {
        Self {
            tweens: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Start a tween. `on_tick` receives eased `t` on every [`advance`](Self::advance).
    pub fn start(
        &mut self,
        duration_ms: u64,
        easing: Easing,
        on_tick: impl Fn(f32) + Send + Sync + 'static,
    ) -> TweenId {
        self.start_with_completion(duration_ms, easing, on_tick, None, None)
    }

    /// Like [`start`](Self::start) but also fires `on_complete` once after the
    /// final tick. `owner` tags the tween so [`cancel_all_for_owner`](Self::cancel_all_for_owner)
    /// can scope cancellation (e.g. per extension).
    pub fn start_with_completion(
        &mut self,
        duration_ms: u64,
        easing: Easing,
        on_tick: impl Fn(f32) + Send + Sync + 'static,
        on_complete: Option<CompleteFn>,
        owner: Option<String>,
    ) -> TweenId {
        let id = TweenId(self.next_id);
        self.next_id += 1;
        self.tweens.insert(
            id,
            TweenEntry {
                start: Instant::now(),
                duration_ms,
                easing,
                on_tick: Arc::new(on_tick),
                on_complete,
                owner,
            },
        );
        id
    }

    /// Cancel a tween. Returns `true` if it was running.
    pub fn cancel(&mut self, id: TweenId) -> bool {
        self.tweens.remove(&id).is_some()
    }

    pub fn is_running(&self, id: TweenId) -> bool {
        self.tweens.contains_key(&id)
    }

    pub fn cancel_all(&mut self) {
        self.tweens.clear();
    }

    /// Cancel every tween tagged with `owner`.
    pub fn cancel_all_for_owner(&mut self, owner: &str) {
        self.tweens
            .retain(|_, e| e.owner.as_deref() != Some(owner));
    }

    pub fn running_count(&self) -> usize {
        self.tweens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tweens.is_empty()
    }

    /// Read-only view of all running tweens, with elapsed time relative to `now`.
    pub fn snapshot(&self, now: Instant) -> Vec<TweenSnapshot> {
        self.tweens
            .iter()
            .map(|(id, e)| TweenSnapshot {
                id: id.0,
                elapsed_ms: now.duration_since(e.start).as_millis(),
                duration_ms: e.duration_ms,
                easing: e.easing.name(),
                owner: e.owner.clone(),
            })
            .collect()
    }

    /// Step every tween to time `now`. Completed tweens are removed. Returns the
    /// callbacks to fire — see [`TickOutcome`]. Does not invoke them itself.
    pub fn advance(&mut self, now: Instant) -> TickOutcome {
        let mut ticks = Vec::with_capacity(self.tweens.len());
        let mut completed = Vec::new();
        for (id, entry) in &self.tweens {
            let raw_t = if entry.duration_ms == 0 {
                1.0
            } else {
                (now.duration_since(entry.start).as_millis() as f32
                    / entry.duration_ms as f32)
                    .min(1.0)
            };
            ticks.push((Arc::clone(&entry.on_tick), apply_easing(entry.easing, raw_t)));
            if raw_t >= 1.0 {
                completed.push(*id);
            }
        }
        let completions = completed
            .into_iter()
            .filter_map(|id| self.tweens.remove(&id).and_then(|e| e.on_complete))
            .collect();
        TickOutcome { ticks, completions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn advance_fires_tick_and_completes_zero_duration() {
        let mut eng = AnimationEngine::new();
        let counter = Arc::new(AtomicU32::new(0));
        let c2 = Arc::clone(&counter);
        let id = eng.start(0, Easing::Linear, move |_t| {
            c2.fetch_add(1, Ordering::Relaxed);
        });
        eng.advance(Instant::now()).dispatch();
        assert!(!eng.is_running(id), "zero-duration tween must complete");
        assert!(counter.load(Ordering::Relaxed) > 0, "on_tick must fire");
    }

    #[test]
    fn cancel_removes_tween() {
        let mut eng = AnimationEngine::new();
        let id = eng.start(10_000, Easing::Linear, |_| {});
        assert!(eng.is_running(id));
        assert!(eng.cancel(id));
        assert!(!eng.is_running(id));
        assert!(!eng.cancel(id), "second cancel reports not-running");
    }

    #[test]
    fn cancel_all_for_owner_is_scoped() {
        let mut eng = AnimationEngine::new();
        let owned = eng.start_with_completion(
            10_000,
            Easing::Linear,
            |_| {},
            None,
            Some("my-ext".to_string()),
        );
        let native = eng.start(10_000, Easing::Linear, |_| {});
        eng.cancel_all_for_owner("my-ext");
        assert!(!eng.is_running(owned), "owned tween must be gone");
        assert!(eng.is_running(native), "untagged tween must survive");
    }

    #[test]
    fn on_complete_fires_exactly_once() {
        let mut eng = AnimationEngine::new();
        let fired = Arc::new(AtomicU32::new(0));
        let f2 = Arc::clone(&fired);
        eng.start_with_completion(
            0,
            Easing::Linear,
            |_| {},
            Some(Box::new(move || {
                f2.fetch_add(1, Ordering::Relaxed);
            })),
            None,
        );
        eng.advance(Instant::now()).dispatch();
        eng.advance(Instant::now()).dispatch();
        assert_eq!(fired.load(Ordering::Relaxed), 1, "on_complete fires once");
    }

    #[test]
    fn snapshot_reports_running_tweens() {
        let mut eng = AnimationEngine::new();
        eng.start_with_completion(
            500,
            Easing::EaseOutCubic,
            |_| {},
            None,
            Some("ext".to_string()),
        );
        let snap = eng.snapshot(Instant::now());
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].duration_ms, 500);
        assert_eq!(snap[0].easing, "ease_out_cubic");
        assert_eq!(snap[0].owner.as_deref(), Some("ext"));
    }
}
