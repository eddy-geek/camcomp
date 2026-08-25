//! camcomp analysis core — Drop 1.
//!
//! Finds groups of photos that show substantially the same scene, taken within
//! one shooting session by different devices or in different shooting modes, and
//! persists the result plus its evidence in a versioned `camcomp.json`.
//!
//! The crate is UI-independent: the CLI, the Tauri desktop shell and a future
//! WASM build are adapters over the same four operations in [`pipeline`].
//!
//! # Quick start
//!
//! ```no_run
//! use camcomp_core::{pipeline, cache};
//! use std::path::PathBuf;
//!
//! let folder = PathBuf::from("/path/to/photos");
//! let options = pipeline::AnalysisOptions::with_reuse();
//!
//! // Reuse a previous analysis when the folder already has one.
//! let previous = cache::find_in_folder(&folder)
//!     .and_then(|p| cache::load(&p).ok());
//!
//! let document = pipeline::analyze_quiet(&[folder.clone()], &options, previous.as_ref())?;
//! cache::save_atomic(&folder.join(cache::CACHE_FILE_NAME), &document)?;
//! # Ok::<(), camcomp_core::Error>(())
//! ```
//!
//! # Coordinate conventions
//!
//! Everything persisted uses the *oriented* image and normalized
//! `[0, 1] × [0, 1]` coordinates, origin top-left, y downwards. Homographies are
//! row-major `3 × 3` matrices in the `photo-to-anchor` direction, so
//! `anchorPoint ~ H · photoPoint`. This is what lets Drop 3 apply a transform at
//! full resolution that Drop 1 estimated from a reduced analysis image.
//!
//! # What decides a pair
//!
//! Geometry first: a pair that fails a hard gate is rejected regardless of its
//! composite score. Weak evidence produces `uncertain`, never a decision — a
//! false negative costs one manual action, a false positive silently corrupts a
//! comparison.

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod cache;
pub mod domain;
pub mod error;
pub mod features;
pub mod geometry;
pub mod grouping;
pub mod heif;
pub mod imageio;
pub mod matching;
pub mod metadata;
pub mod metrics;
pub mod pipeline;
pub mod progress;
pub mod registration;
pub mod schema;
pub mod sessions;
pub mod warp;

pub use error::{Error, Result};
pub use pipeline::{analyze, analyze_quiet, inspect, regroup, validate, AnalysisOptions};
pub use progress::{CancelToken, ProgressEvent, ProgressSink, Stage};
pub use schema::{
    AlgorithmInfo, AnalysisDocument, AnalysisParameters, Group, HardGates, MetricWeights,
    PairResult, PairState, Photo, ALGORITHM_NAME, ALGORITHM_VERSION, SCHEMA_MAJOR, SCHEMA_VERSION,
};
