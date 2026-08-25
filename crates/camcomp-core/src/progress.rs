//! Progress reporting and cancellation.
//!
//! Analysis must be interruptible and must report progress
//! (`REQUIREMENTS.md` §8). Both are adapter-agnostic: the CLI prints them, a
//! Tauri command forwards them as typed events.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::Serialize;

/// A pipeline stage, in execution order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    /// Enumerating input files.
    Discover,
    /// Hashing and reading metadata.
    Inspect,
    /// Session detection and candidate-pair generation.
    Candidates,
    /// Decoding and feature extraction.
    Features,
    /// Pairwise registration and scoring.
    Pairs,
    /// Group construction and manual overrides.
    Grouping,
    /// Serializing `camcomp.json`.
    Persist,
}

impl Stage {
    /// Stable identifier used in progress payloads.
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Discover => "discover",
            Stage::Inspect => "inspect",
            Stage::Candidates => "candidates",
            Stage::Features => "features",
            Stage::Pairs => "pairs",
            Stage::Grouping => "grouping",
            Stage::Persist => "persist",
        }
    }
}

/// A progress notification.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProgressEvent {
    /// A stage started. `total` is `None` when the work is not countable yet.
    StageStarted {
        /// Stage that started.
        stage: Stage,
        /// Number of units of work, when known.
        total: Option<u64>,
    },
    /// Progress within a stage.
    Advanced {
        /// Stage in progress.
        stage: Stage,
        /// Units completed.
        done: u64,
        /// Units total, when known.
        total: Option<u64>,
        /// Short description of the current unit. Never an absolute path.
        detail: Option<String>,
    },
    /// A stage finished.
    StageFinished {
        /// Stage that finished.
        stage: Stage,
        /// Wall time spent in the stage.
        elapsed_ms: u64,
    },
    /// Something non-fatal worth surfacing (unreadable file, missing timestamp).
    Warning {
        /// Stage that produced the warning.
        stage: Stage,
        /// Human-readable message.
        message: String,
    },
}

/// Receives [`ProgressEvent`]s. Implementations must not block for long.
pub trait ProgressSink: Send + Sync {
    /// Handle one event.
    fn emit(&self, event: &ProgressEvent);
}

/// Discards every event.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopProgress;

impl ProgressSink for NoopProgress {
    fn emit(&self, _event: &ProgressEvent) {}
}

impl<F> ProgressSink for F
where
    F: Fn(&ProgressEvent) + Send + Sync,
{
    fn emit(&self, event: &ProgressEvent) {
        self(event)
    }
}

/// Cooperative cancellation flag, checked between stages and inside the pair
/// and feature loops.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token that has not been cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Request cancellation. Idempotent and callable from any thread.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// `Err(Error::Cancelled)` once cancellation has been requested.
    pub fn check(&self) -> crate::Result<()> {
        if self.is_cancelled() {
            Err(crate::Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Bundles the two ambient concerns so the pipeline signature stays readable.
pub struct Reporter<'a> {
    sink: &'a dyn ProgressSink,
    cancel: &'a CancelToken,
}

impl<'a> Reporter<'a> {
    /// Wrap a sink and a cancellation token.
    pub fn new(sink: &'a dyn ProgressSink, cancel: &'a CancelToken) -> Self {
        Self { sink, cancel }
    }

    /// Emit an event.
    pub fn emit(&self, event: ProgressEvent) {
        self.sink.emit(&event);
    }

    /// Emit a stage start.
    pub fn stage_started(&self, stage: Stage, total: Option<u64>) {
        self.emit(ProgressEvent::StageStarted { stage, total });
    }

    /// Emit a stage completion.
    pub fn stage_finished(&self, stage: Stage, elapsed_ms: u64) {
        self.emit(ProgressEvent::StageFinished { stage, elapsed_ms });
    }

    /// Emit an advance.
    pub fn advanced(&self, stage: Stage, done: u64, total: Option<u64>, detail: Option<String>) {
        self.emit(ProgressEvent::Advanced {
            stage,
            done,
            total,
            detail,
        });
    }

    /// Emit a warning.
    pub fn warn(&self, stage: Stage, message: impl Into<String>) {
        self.emit(ProgressEvent::Warning {
            stage,
            message: message.into(),
        });
    }

    /// Propagate cancellation.
    pub fn check_cancelled(&self) -> crate::Result<()> {
        self.cancel.check()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn cancellation_is_observed_by_the_reporter() {
        let cancel = CancelToken::new();
        let sink = NoopProgress;
        let reporter = Reporter::new(&sink, &cancel);
        assert!(reporter.check_cancelled().is_ok());
        cancel.cancel();
        assert!(matches!(
            reporter.check_cancelled(),
            Err(crate::Error::Cancelled)
        ));
    }

    #[test]
    fn closures_are_usable_as_sinks() {
        let seen = Mutex::new(Vec::new());
        {
            let sink = |ev: &ProgressEvent| {
                if let ProgressEvent::Warning { message, .. } = ev {
                    seen.lock().unwrap().push(message.clone());
                }
            };
            let cancel = CancelToken::new();
            let reporter = Reporter::new(&sink, &cancel);
            reporter.warn(Stage::Inspect, "missing timestamp");
        }
        assert_eq!(seen.into_inner().unwrap(), vec!["missing timestamp"]);
    }

    #[test]
    fn events_serialize_with_a_stable_tag() {
        let json = serde_json::to_string(&ProgressEvent::StageStarted {
            stage: Stage::Pairs,
            total: Some(3),
        })
        .unwrap();
        assert!(json.contains("\"kind\":\"stage-started\""), "{json}");
        assert!(json.contains("\"stage\":\"pairs\""), "{json}");
    }
}
