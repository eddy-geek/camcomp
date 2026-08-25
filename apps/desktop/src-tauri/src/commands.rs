//! Tauri command handlers.
//!
//! In a submodule rather than in `lib.rs` on purpose: `#[tauri::command]`
//! expands to a `#[macro_export]`ed helper macro plus a `pub use` of it, and at
//! the crate root those two collide.

use std::path::PathBuf;
use std::sync::Mutex;

use camcomp_core::cache;
use camcomp_core::pipeline::{self, AnalysisOptions};
use camcomp_core::progress::{CancelToken, ProgressEvent, ProgressSink};
use camcomp_core::schema::{AnalysisDocument, ManualOverride};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// Event name for progress notifications.
pub const PROGRESS_EVENT: &str = "camcomp://progress";

/// A structured error the frontend can branch on.
///
/// Carries a stable `code` as well as a message, so the UI does not have to
/// string-match, and never includes an absolute path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    /// Stable discriminant, from `camcomp_core::Error::code`.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

impl From<camcomp_core::Error> for CommandError {
    fn from(error: camcomp_core::Error) -> Self {
        Self {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

/// What the frontend asks for.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeRequest {
    /// Folders or files to analyze.
    pub inputs: Vec<PathBuf>,
    /// Write `camcomp.json` back to the source folder.
    #[serde(default = "default_true")]
    pub persist: bool,
    /// Reuse a previous analysis found in the folder.
    #[serde(default = "default_true")]
    pub reuse_cache: bool,
    /// Overrides for the specified tunables. Absent fields keep their defaults.
    #[serde(default)]
    pub session_gap_seconds: Option<u32>,
    /// Angle-of-view tolerance override.
    #[serde(default)]
    pub field_of_view_tolerance: Option<f64>,
    /// Compatibility threshold override.
    #[serde(default)]
    pub compatibility_threshold: Option<f64>,
    /// Analysis image longest edge override.
    #[serde(default)]
    pub analysis_longest_edge: Option<u32>,
}

fn default_true() -> bool {
    true
}

/// What the frontend gets back.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeResponse {
    /// The analysis document.
    pub document: AnalysisDocument,
    /// Where it was written, relative to nothing — just the file name — so a
    /// hosted log never learns the user's folder layout.
    pub written: Option<String>,
}

/// Version and capability negotiation (`ADR-DROP1.md` §6).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// `camcomp.json` schema version written by this build.
    pub schema_version: String,
    /// Schema major version this build can read.
    pub schema_major: u32,
    /// Algorithm identity.
    pub algorithm: String,
    /// Algorithm version.
    pub algorithm_version: String,
    /// Formats this build decodes.
    pub formats: Vec<String>,
    /// Whether the analysis engine is in-process (no sidecar).
    pub in_process_engine: bool,
}

/// Cancellation token for the run currently in flight.
#[derive(Default)]
pub struct RunState {
    current: Mutex<Option<CancelToken>>,
}

impl RunState {
    /// Register a new run, cancelling any run still in flight.
    fn begin(&self) -> CancelToken {
        let token = CancelToken::new();
        let mut slot = self.current.lock().expect("run state poisoned");
        if let Some(previous) = slot.replace(token.clone()) {
            previous.cancel();
        }
        token
    }

    /// Clear the slot when a run finishes.
    fn finish(&self) {
        let mut slot = self.current.lock().expect("run state poisoned");
        *slot = None;
    }

    /// Ask the in-flight run to stop. `false` when nothing is running.
    pub fn cancel(&self) -> bool {
        let slot = self.current.lock().expect("run state poisoned");
        match slot.as_ref() {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }
}

/// Forwards core progress events to the webview.
struct EventForwarder {
    app: AppHandle,
}

impl ProgressSink for EventForwarder {
    fn emit(&self, event: &ProgressEvent) {
        // A dropped progress event must never fail the analysis, so the result
        // is deliberately discarded.
        let _ = self.app.emit(PROGRESS_EVENT, event);
    }
}

/// Report what this build can do.
#[tauri::command]
pub fn capabilities() -> Capabilities {
    Capabilities {
        schema_version: camcomp_core::SCHEMA_VERSION.to_string(),
        schema_major: camcomp_core::SCHEMA_MAJOR,
        algorithm: camcomp_core::ALGORITHM_NAME.to_string(),
        algorithm_version: camcomp_core::ALGORITHM_VERSION.to_string(),
        formats: vec!["jpeg".into(), "png".into(), "heic".into()],
        in_process_engine: true,
    }
}

/// Read metadata for a selection without analyzing pixels.
#[tauri::command]
pub async fn inspect_inputs(
    app: AppHandle,
    inputs: Vec<PathBuf>,
) -> Result<Vec<camcomp_core::Photo>, CommandError> {
    // `spawn_blocking` because decoding and hashing are CPU-bound: doing them on
    // the async runtime would stall every other command.
    tauri::async_runtime::spawn_blocking(
        move || -> camcomp_core::Result<Vec<camcomp_core::Photo>> {
            let cancel = CancelToken::new();
            let sink = EventForwarder { app };
            let reporter = camcomp_core::progress::Reporter::new(&sink, &cancel);
            let algorithm = camcomp_core::AlgorithmInfo::default();
            let inspection = pipeline::inspect(&inputs, &algorithm, None, &reporter)?;
            Ok(inspection
                .photos
                .iter()
                .map(|p| camcomp_core::Photo {
                    id: p.id.clone(),
                    path: p.relative_path.clone(),
                    configuration_id: p.configuration_id.clone(),
                    fingerprint: p.fingerprint.clone(),
                    dimensions: camcomp_core::schema::Dimensions {
                        original: [p.original_dimensions.0, p.original_dimensions.1],
                        oriented: [p.oriented_dimensions.0, p.oriented_dimensions.1],
                        analysis: None,
                    },
                    metadata: p.metadata.clone(),
                    features: None,
                    notes: p.notes.clone(),
                })
                .collect())
        },
    )
    .await
    .map_err(|e| CommandError {
        code: "join".into(),
        message: e.to_string(),
    })?
    .map_err(CommandError::from)
}

/// Run the analysis off the UI thread, emitting progress and honouring
/// cancellation.
#[tauri::command]
pub async fn analyze(
    app: AppHandle,
    request: AnalyzeRequest,
) -> Result<AnalyzeResponse, CommandError> {
    let state = app.state::<RunState>();
    let cancel = state.begin();
    let result = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        let cancel = cancel.clone();
        move || run_analysis(&app, request, &cancel)
    })
    .await;
    app.state::<RunState>().finish();

    match result {
        Ok(inner) => inner.map_err(CommandError::from),
        Err(e) => Err(CommandError {
            code: "join".into(),
            message: e.to_string(),
        }),
    }
}

fn run_analysis(
    app: &AppHandle,
    request: AnalyzeRequest,
    cancel: &CancelToken,
) -> camcomp_core::Result<AnalyzeResponse> {
    let mut options = AnalysisOptions::with_reuse();
    options.reuse_previous = request.reuse_cache;
    let p = &mut options.algorithm.parameters;
    if let Some(v) = request.session_gap_seconds {
        p.session_gap_seconds = v;
    }
    if let Some(v) = request.field_of_view_tolerance {
        p.field_of_view_tolerance = v;
    }
    if let Some(v) = request.compatibility_threshold {
        p.compatibility_threshold = v;
    }
    if let Some(v) = request.analysis_longest_edge {
        p.analysis_longest_edge = v;
    }

    let target = target_path(&request.inputs);
    let previous = if request.reuse_cache {
        target
            .as_ref()
            .filter(|path| path.is_file())
            .and_then(|path| cache::load(path).ok())
    } else {
        None
    };

    let sink = EventForwarder { app: app.clone() };
    let document = pipeline::analyze(&request.inputs, &options, previous.as_ref(), &sink, cancel)?;

    let written = match (&target, request.persist) {
        (Some(path), true) => {
            cache::save_atomic(path, &document)?;
            path.file_name().map(|n| n.to_string_lossy().to_string())
        }
        _ => None,
    };
    Ok(AnalyzeResponse { document, written })
}

fn target_path(inputs: &[PathBuf]) -> Option<PathBuf> {
    let first = inputs.first()?;
    if first.is_dir() {
        Some(first.join(cache::CACHE_FILE_NAME))
    } else {
        first.parent().map(|d| d.join(cache::CACHE_FILE_NAME))
    }
}

/// Ask the in-flight analysis to stop. The previously persisted document is
/// untouched, so cancelling never loses completed work.
#[tauri::command]
pub fn cancel_analysis(state: State<'_, RunState>) -> bool {
    state.cancel()
}

/// Load an existing analysis.
#[tauri::command]
pub fn load_analysis(path: PathBuf) -> Result<AnalysisDocument, CommandError> {
    let path = if path.is_dir() {
        cache::find_in_folder(&path).ok_or_else(|| CommandError {
            code: "io".into(),
            message: format!("no {} in the selected folder", cache::CACHE_FILE_NAME),
        })?
    } else {
        path
    };
    cache::load(&path).map_err(CommandError::from)
}

/// Apply manual overrides and rewrite the document.
#[tauri::command]
pub fn apply_overrides(
    path: PathBuf,
    overrides: Vec<ManualOverride>,
) -> Result<AnalysisDocument, CommandError> {
    let path = if path.is_dir() {
        cache::find_in_folder(&path).ok_or_else(|| CommandError {
            code: "io".into(),
            message: format!("no {} in the selected folder", cache::CACHE_FILE_NAME),
        })?
    } else {
        path
    };
    let document = cache::load(&path).map_err(CommandError::from)?;
    let updated = pipeline::regroup(&document, &overrides).map_err(CommandError::from)?;
    cache::save_atomic(&path, &updated).map_err(CommandError::from)?;
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_report_an_in_process_engine() {
        let caps = capabilities();
        assert!(caps.in_process_engine, "no sidecar is the point of the ADR");
        assert_eq!(caps.schema_major, camcomp_core::SCHEMA_MAJOR);
        assert!(caps.formats.iter().any(|f| f == "heic"));
    }

    #[test]
    fn core_errors_keep_their_stable_code_across_the_boundary() {
        let error: CommandError = camcomp_core::Error::UnsupportedFormat {
            path: "a.dng".into(),
        }
        .into();
        assert_eq!(error.code, "unsupported-format");
        // Round-trips as JSON, which is how it reaches the frontend.
        let json = serde_json::to_string(&error).unwrap();
        let back: CommandError = serde_json::from_str(&json).unwrap();
        assert_eq!(error, back);
        assert!(json.contains("\"code\":\"unsupported-format\""), "{json}");
    }

    #[test]
    fn cancelling_with_nothing_running_is_not_an_error() {
        let state = RunState::default();
        assert!(!state.cancel());
    }

    #[test]
    fn beginning_a_run_cancels_the_previous_one() {
        let state = RunState::default();
        let first = state.begin();
        assert!(!first.is_cancelled());
        let second = state.begin();
        assert!(first.is_cancelled(), "a new run must stop the old one");
        assert!(!second.is_cancelled());
        assert!(state.cancel());
        assert!(second.is_cancelled());
        state.finish();
        assert!(!state.cancel());
    }

    #[test]
    fn a_request_deserializes_with_only_the_inputs_supplied() {
        let request: AnalyzeRequest = serde_json::from_str(r#"{"inputs":["/photos"]}"#).unwrap();
        assert!(request.persist);
        assert!(request.reuse_cache);
        assert!(request.session_gap_seconds.is_none());
    }

    #[test]
    fn tunable_overrides_reach_the_request() {
        let request: AnalyzeRequest = serde_json::from_str(
            r#"{"inputs":["/photos"],"persist":false,"sessionGapSeconds":45,"fieldOfViewTolerance":0.4}"#,
        )
        .unwrap();
        assert!(!request.persist);
        assert_eq!(request.session_gap_seconds, Some(45));
        assert_eq!(request.field_of_view_tolerance, Some(0.4));
    }

    #[test]
    fn the_write_target_lands_next_to_the_photos() {
        let dir = std::env::temp_dir().join(format!("camcomp-desktop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            target_path(std::slice::from_ref(&dir)),
            Some(dir.join(cache::CACHE_FILE_NAME))
        );
        let file = dir.join("a.jpg");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(target_path(&[file]), Some(dir.join(cache::CACHE_FILE_NAME)));
        assert_eq!(target_path(&[]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loading_a_folder_without_an_analysis_reports_it_clearly() {
        let dir =
            std::env::temp_dir().join(format!("camcomp-desktop-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let error = load_analysis(dir.clone()).unwrap_err();
        assert_eq!(error.code, "io");
        assert!(error.message.contains("camcomp.json"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_events_serialize_for_the_frontend_contract() {
        // The frontend listens on one channel and switches on `kind`.
        let json = serde_json::to_string(&ProgressEvent::Advanced {
            stage: camcomp_core::Stage::Pairs,
            done: 3,
            total: Some(9),
            detail: Some("pair-a-b".into()),
        })
        .unwrap();
        assert!(json.contains("\"kind\":\"advanced\""), "{json}");
        assert!(json.contains("\"stage\":\"pairs\""), "{json}");
        assert!(json.contains("\"done\":3"), "{json}");
    }
}
