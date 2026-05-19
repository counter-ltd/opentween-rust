//! # opentween
//!
//! A small, driver-agnostic tweening / animation engine.
//!
//! - **No threads, no globals.** The caller owns an [`AnimationEngine`] and
//!   drives it by calling [`AnimationEngine::advance`] — from a render loop, a
//!   fixed timer, or anything else.
//! - **No GUI dependency.** Pure `std`. Usable in GUI apps, headless services,
//!   game loops, or any Rust program.
//! - **Re-entrancy-safe.** `advance` returns callbacks instead of invoking them,
//!   so a callback can start or cancel tweens without deadlocking the caller's
//!   lock.
//!
//! ```
//! use opentween::{AnimationEngine, Easing};
//! use std::time::Instant;
//!
//! let mut engine = AnimationEngine::new();
//! engine.start(200, Easing::EaseOutCubic, |t| {
//!     // apply eased progress `t` to whatever you are animating
//!     let _ = t;
//! });
//! // later, each frame / tick:
//! engine.advance(Instant::now()).dispatch();
//! ```

mod easing;
mod engine;

pub use easing::{apply_easing, lerp_f32, lerp_rgba, lerp_u8, shake_offset, Easing};
pub use engine::{AnimationEngine, CompleteFn, TickFn, TickOutcome, TweenId, TweenSnapshot};
