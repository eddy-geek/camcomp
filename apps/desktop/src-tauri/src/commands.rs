//! Tauri command handlers.
//!
//! In a submodule rather than in `lib.rs` on purpose: `#[tauri::command]`
//! expands to a `#[macro_export]`ed helper macro plus a `pub use` of it, and at
//! the crate root those two collide.

#[cfg(not(target_os = "macos"))]
use std::collections::HashSet;
use std::fs::File;
#[cfg(not(target_os = "macos"))]
use std::io::BufWriter;
use std::io::Read;
#[cfg(not(target_os = "macos"))]
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_os = "macos"))]
use std::sync::Condvar;
use std::sync::Mutex;

use camcomp_core::cache;
use camcomp_core::imageio::SourceFormat;
use camcomp_core::pipeline::{self, AnalysisOptions, Discovery};
use camcomp_core::progress::{CancelToken, ProgressEvent, ProgressSink};
use camcomp_core::schema::{AnalysisDocument, ManualOverride, Photo};
#[cfg(not(target_os = "macos"))]
use image::ExtendedColorType;
#[cfg(not(target_os = "macos"))]
use image::ImageEncoder;
use image::{ImageDecoder, ImageReader};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

/// Event name for progress notifications.
pub const PROGRESS_EVENT: &str = "camcomp://progress";

/// Largest compressed HEIC accepted by the display decoder.
#[cfg(not(target_os = "macos"))]
const MAX_HEIC_COMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
/// Largest oriented HEIC accepted by the display decoder.
#[cfg(not(target_os = "macos"))]
const MAX_HEIC_PIXELS: u64 = 120_000_000;
#[cfg(not(target_os = "macos"))]
const MAX_PARALLEL_HEIC_DECODES: usize = 2;
/// Versioned suffix so changing display-decoder behavior invalidates old files.
#[cfg(not(target_os = "macos"))]
const HEIC_CACHE_SUFFIX: &str = "heif-oxide-v1.png";

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

fn command_error(code: &str, message: impl Into<String>) -> CommandError {
    CommandError {
        code: code.to_string(),
        message: message.into(),
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

/// Safe metadata returned before analysis can write anything.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreflightResponse {
    /// Basename of the common source root, never its absolute path.
    pub source_root_label: String,
    /// Human-readable relative destination label.
    pub write_target_label: String,
    /// Whether the source root already contains `camcomp.json`.
    pub has_camcomp_json: bool,
    /// Number of supported photos found by core discovery.
    pub photo_count: usize,
}

/// Kind of local source returned for display.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PhotoSourceKind {
    /// The acquired JPEG, PNG, or macOS-native HEIC itself.
    Native,
    /// A dimension-matching same-directory JPEG or PNG substitute.
    Sidecar,
    /// An SDR PNG derived into application cache storage with lossless encoding.
    Decoded,
}

/// Container format of the display source selected by magic bytes.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PhotoDisplayFormat {
    /// JPEG, including JPEG data stored under another extension.
    Jpeg,
    /// PNG source or decoded derivative.
    Png,
    /// Native HEIC/HEIF source.
    Heic,
}

/// A local photo source consumed only by the Tauri adapter.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePhotoSourceResponse {
    /// ID from the analysis document.
    pub photo_id: String,
    /// How the display source was obtained.
    pub source_kind: PhotoSourceKind,
    /// Container format selected from the display source's magic bytes.
    pub display_format: PhotoDisplayFormat,
    /// Absolute local display path; the adapter converts this to an asset URL.
    pub display_path: PathBuf,
    /// Absolute path of the immutable acquired source photo.
    pub original_path: PathBuf,
    /// Non-fatal substitution or stale-sidecar observations.
    pub warnings: Vec<String>,
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
    current: Mutex<Option<(u64, CancelToken)>>,
    next_id: AtomicU64,
}

impl RunState {
    /// Register a new run, cancelling any run still in flight.
    fn begin(&self) -> (u64, CancelToken) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let token = CancelToken::new();
        let mut slot = self.current.lock().expect("run state poisoned");
        if let Some((_, previous)) = slot.replace((id, token.clone())) {
            previous.cancel();
        }
        (id, token)
    }

    /// Clear the slot when the matching run finishes.
    fn finish(&self, id: u64) {
        let mut slot = self.current.lock().expect("run state poisoned");
        if slot.as_ref().map(|(current_id, _)| *current_id) == Some(id) {
            *slot = None;
        }
    }

    /// Ask the in-flight run to stop. `false` when nothing is running.
    pub fn cancel(&self) -> bool {
        let slot = self.current.lock().expect("run state poisoned");
        match slot.as_ref() {
            Some((_, token)) => {
                token.cancel();
                true
            }
            None => false,
        }
    }
}

#[cfg(not(target_os = "macos"))]
struct DecodeGateState {
    active: HashSet<String>,
}

#[cfg(not(target_os = "macos"))]
struct DecodeGate {
    state: Mutex<DecodeGateState>,
    ready: Condvar,
}

#[cfg(not(target_os = "macos"))]
impl Default for DecodeGate {
    fn default() -> Self {
        Self {
            state: Mutex::new(DecodeGateState {
                active: HashSet::new(),
            }),
            ready: Condvar::new(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl DecodeGate {
    fn enter<'a>(&'a self, key: &str) -> Result<DecodePermit<'a>, CommandError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| command_error("state", "The native HEIC decoder is unavailable."))?;
        while state.active.len() >= MAX_PARALLEL_HEIC_DECODES || state.active.contains(key) {
            state = self
                .ready
                .wait(state)
                .map_err(|_| command_error("state", "The native HEIC decoder is unavailable."))?;
        }
        state.active.insert(key.to_string());
        Ok(DecodePermit {
            gate: self,
            key: key.to_string(),
        })
    }
}

#[cfg(not(target_os = "macos"))]
struct DecodePermit<'a> {
    gate: &'a DecodeGate,
    key: String,
}

#[cfg(not(target_os = "macos"))]
impl Drop for DecodePermit<'_> {
    fn drop(&mut self) {
        let mut state = self
            .gate
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.active.remove(&self.key);
        self.gate.ready.notify_all();
    }
}

/// Native access state for the source root acquired by the current import.
#[derive(Default)]
pub struct SourceScope {
    root: Mutex<Option<PathBuf>>,
    #[cfg(not(target_os = "macos"))]
    decoder: DecodeGate,
}

impl SourceScope {
    fn set_root(&self, root: PathBuf) -> Result<(), CommandError> {
        let mut current = self
            .root
            .lock()
            .map_err(|_| command_error("state", "The native source scope is unavailable."))?;
        *current = Some(root);
        Ok(())
    }

    fn root(&self) -> Result<PathBuf, CommandError> {
        self.root
            .lock()
            .map_err(|_| command_error("state", "The native source scope is unavailable."))?
            .clone()
            .ok_or_else(|| {
                command_error(
                    "source-not-acquired",
                    "Open the containing photo folder before displaying this photo.",
                )
            })
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

#[derive(Debug)]
struct AcquiredSource {
    root: PathBuf,
    response: PreflightResponse,
}

fn invalid_write_target(detail: &str) -> CommandError {
    command_error(
        "invalid-write-target",
        format!("{detail} Open a containing photo folder so camcomp.json has a safe destination."),
    )
}

fn safe_discovery_error(error: camcomp_core::Error) -> CommandError {
    match error {
        camcomp_core::Error::UnsupportedFormat { .. } => command_error(
            "unsupported-format",
            "A selected file is not a supported JPEG, PNG, or HEIC image.",
        ),
        other => command_error(
            other.code(),
            "A selected file or folder could not be read during input discovery.",
        ),
    }
}

fn is_filesystem_or_volume_root(root: &Path) -> bool {
    let Some(parent) = root.parent() else {
        return true;
    };
    if parent == root {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(root_metadata), Ok(parent_metadata)) =
            (std::fs::metadata(root), std::fs::metadata(parent))
        {
            return root_metadata.dev() != parent_metadata.dev();
        }
    }
    false
}

fn acquire_source(inputs: &[PathBuf]) -> Result<AcquiredSource, CommandError> {
    if inputs.is_empty() {
        return Err(invalid_write_target("No photos or folder were selected."));
    }
    let Discovery { root, files, .. } = pipeline::discover(inputs).map_err(safe_discovery_error)?;
    let root = root.ok_or_else(|| {
        invalid_write_target("The selected photos do not have a common source folder.")
    })?;
    if files.is_empty() && !root.join(cache::CACHE_FILE_NAME).is_file() {
        return Err(invalid_write_target(
            "The selection contains no supported photos or existing camcomp.json.",
        ));
    }
    let root = root.canonicalize().map_err(|_| {
        invalid_write_target("The common source folder could not be resolved securely.")
    })?;
    if is_filesystem_or_volume_root(&root) {
        return Err(invalid_write_target(
            "The common source folder is a filesystem or volume root.",
        ));
    }
    let source_root_label = root
        .file_name()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "selected folder".to_string());
    Ok(AcquiredSource {
        response: PreflightResponse {
            write_target_label: format!("{source_root_label}/{}", cache::CACHE_FILE_NAME),
            has_camcomp_json: root.join(cache::CACHE_FILE_NAME).is_file(),
            photo_count: files.len(),
            source_root_label,
        },
        root,
    })
}

fn validate_source_root(root: &Path) -> Result<PathBuf, CommandError> {
    let root = root.canonicalize().map_err(|_| {
        invalid_write_target("The selected source folder could not be resolved securely.")
    })?;
    if !root.is_dir() || is_filesystem_or_volume_root(&root) {
        return Err(invalid_write_target(
            "The selected source folder is a filesystem or volume root.",
        ));
    }
    Ok(root)
}

fn register_source_root(app: &AppHandle, root: &Path) -> Result<(), CommandError> {
    app.state::<SourceScope>().set_root(root.to_path_buf())
}

/// Compute and register the real common source root before analysis writes.
#[tauri::command]
pub async fn preflight_inputs(
    app: AppHandle,
    inputs: Vec<PathBuf>,
) -> Result<PreflightResponse, CommandError> {
    let acquired = tauri::async_runtime::spawn_blocking(move || acquire_source(&inputs))
        .await
        .map_err(|_| command_error("join", "Input preflight did not complete."))??;
    register_source_root(&app, &acquired.root)?;
    Ok(acquired.response)
}

/// Read metadata for a selection without analyzing pixels.
#[tauri::command]
pub async fn inspect_inputs(
    app: AppHandle,
    inputs: Vec<PathBuf>,
) -> Result<Vec<camcomp_core::Photo>, CommandError> {
    // `spawn_blocking` because decoding and hashing are CPU-bound: doing them on
    // the async runtime would stall every other command.
    let worker_app = app.clone();
    let photos = tauri::async_runtime::spawn_blocking(
        move || -> Result<(PathBuf, Vec<camcomp_core::Photo>), CommandError> {
            let acquired = acquire_source(&inputs)?;
            let cancel = CancelToken::new();
            let sink = EventForwarder { app: worker_app };
            let reporter = camcomp_core::progress::Reporter::new(&sink, &cancel);
            let algorithm = camcomp_core::AlgorithmInfo::default();
            let inspection = pipeline::inspect(&inputs, &algorithm, None, &reporter)
                .map_err(CommandError::from)?;
            let photos = inspection
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
                .collect();
            Ok((acquired.root, photos))
        },
    )
    .await
    .map_err(|_| command_error("join", "Input inspection did not complete."))??;
    register_source_root(&app, &photos.0)?;
    Ok(photos.1)
}

/// Run the analysis off the UI thread, emitting progress and honouring
/// cancellation.
#[tauri::command]
pub async fn analyze(
    app: AppHandle,
    request: AnalyzeRequest,
) -> Result<AnalyzeResponse, CommandError> {
    let state = app.state::<RunState>();
    let (run_id, cancel) = state.begin();
    let result = tauri::async_runtime::spawn_blocking({
        let app = app.clone();
        let cancel = cancel.clone();
        move || run_analysis(&app, request, &cancel)
    })
    .await;
    app.state::<RunState>().finish(run_id);

    match result {
        Ok(inner) => inner,
        Err(_) => Err(command_error("join", "Analysis did not complete.")),
    }
}

fn run_analysis(
    app: &AppHandle,
    request: AnalyzeRequest,
    cancel: &CancelToken,
) -> Result<AnalyzeResponse, CommandError> {
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

    let acquired = acquire_source(&request.inputs)?;
    register_source_root(app, &acquired.root)?;
    let target = acquired.root.join(cache::CACHE_FILE_NAME);
    let previous = if request.reuse_cache && target.is_file() {
        let document = cache::load(&target).map_err(CommandError::from)?;
        pipeline::validate(&document).map_err(CommandError::from)?;
        Some(document)
    } else {
        None
    };

    let sink = EventForwarder { app: app.clone() };
    let document = pipeline::analyze(&request.inputs, &options, previous.as_ref(), &sink, cancel)
        .map_err(CommandError::from)?;

    let written = if request.persist {
        cache::save_atomic(&target, &document).map_err(CommandError::from)?;
        target
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
    } else {
        None
    };
    Ok(AnalyzeResponse { document, written })
}

/// Ask the in-flight analysis to stop. The previously persisted document is
/// untouched, so cancelling never loses completed work.
#[tauri::command]
pub fn cancel_analysis(state: State<'_, RunState>) -> bool {
    state.cancel()
}

fn selected_document(path: PathBuf) -> Result<(PathBuf, PathBuf), CommandError> {
    if path.is_dir() {
        let root = validate_source_root(&path)?;
        let document_path = cache::find_in_folder(&root).ok_or_else(|| {
            command_error(
                "io",
                format!("no {} in the selected folder", cache::CACHE_FILE_NAME),
            )
        })?;
        Ok((document_path, root))
    } else {
        let document_path = path.canonicalize().map_err(|_| {
            command_error("io", "The selected analysis document could not be read.")
        })?;
        let root = document_path.parent().ok_or_else(|| {
            invalid_write_target("The analysis document has no containing source folder.")
        })?;
        let root = validate_source_root(root)?;
        Ok((document_path, root))
    }
}

fn load_validated_document(path: &Path) -> Result<AnalysisDocument, CommandError> {
    let document = cache::load(path).map_err(CommandError::from)?;
    pipeline::validate(&document).map_err(CommandError::from)?;
    Ok(document)
}

/// Load an existing analysis.
#[tauri::command]
pub fn load_analysis(app: AppHandle, path: PathBuf) -> Result<AnalysisDocument, CommandError> {
    let (path, root) = selected_document(path)?;
    let document = load_validated_document(&path)?;
    register_source_root(&app, &root)?;
    Ok(document)
}

/// Apply manual overrides and rewrite the document.
#[tauri::command]
pub fn apply_overrides(
    app: AppHandle,
    path: PathBuf,
    overrides: Vec<ManualOverride>,
) -> Result<AnalysisDocument, CommandError> {
    let (path, root) = selected_document(path)?;
    let document = load_validated_document(&path)?;
    let updated = pipeline::regroup(&document, &overrides).map_err(CommandError::from)?;
    cache::save_atomic(&path, &updated).map_err(CommandError::from)?;
    register_source_root(&app, &root)?;
    Ok(updated)
}

fn normalize_relative_photo_path(path: &str) -> Result<PathBuf, CommandError> {
    if path.is_empty()
        || path.contains('\\')
        || path.starts_with('/')
        || path.as_bytes().get(1) == Some(&b':')
    {
        return Err(command_error(
            "invalid-photo-path",
            "The analysis contains an absolute or non-portable photo path.",
        ));
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(command_error(
                    "invalid-photo-path",
                    "The analysis contains a photo path that leaves its source folder.",
                ))
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(command_error(
            "invalid-photo-path",
            "The analysis contains an empty photo path.",
        ));
    }
    Ok(normalized)
}

fn resolve_relative_photo(root: &Path, relative: &str) -> Result<PathBuf, CommandError> {
    let relative = normalize_relative_photo_path(relative)?;
    let candidate = root.join(relative);
    let resolved = candidate.canonicalize().map_err(|_| {
        command_error(
            "photo-not-found",
            "A photo referenced by the analysis is missing or inaccessible.",
        )
    })?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(command_error(
            "invalid-photo-path",
            "A photo path or symlink leaves the acquired source folder.",
        ));
    }
    Ok(resolved)
}

fn display_source_format(path: &Path) -> Result<SourceFormat, CommandError> {
    let mut header = [0_u8; 12];
    let length = File::open(path)
        .and_then(|mut file| file.read(&mut header))
        .map_err(|_| command_error("io", "The photo source header could not be read."))?;
    SourceFormat::from_magic(&header[..length])
        .or_else(|| SourceFormat::from_extension(path))
        .ok_or_else(|| {
            command_error(
                "unsupported-format",
                "The photo is not a supported JPEG, PNG, or HEIC image.",
            )
        })
}

fn displayed_dimensions(path: &Path) -> Result<(u32, u32), CommandError> {
    let reader = ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|_| command_error("decode", "An image header could not be read."))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| command_error("decode", "An image header could not be decoded."))?;
    let (mut width, mut height) = decoder.dimensions();
    let orientation = decoder
        .orientation()
        .map_err(|_| command_error("decode", "Image orientation metadata could not be read."))?;
    if (5..=8).contains(&orientation.to_exif()) {
        std::mem::swap(&mut width, &mut height);
    }
    Ok((width, height))
}

fn dimension_mismatch(expected: [u32; 2]) -> CommandError {
    command_error(
        "source-dimension-mismatch",
        format!(
            "The display source does not match the document's oriented dimensions {}×{}.",
            expected[0], expected[1]
        ),
    )
}

#[cfg(not(target_os = "macos"))]
fn find_sidecar(
    root: &Path,
    original: &Path,
    expected: [u32; 2],
) -> (Option<PathBuf>, Vec<String>) {
    let mut warnings = Vec::new();
    for extension in ["jpg", "jpeg", "png"] {
        let candidate = original.with_extension(extension);
        if !candidate.is_file() {
            continue;
        }
        let label = candidate
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("sidecar.{extension}"));
        let Ok(candidate) = candidate.canonicalize() else {
            warnings.push(format!("Ignored unreadable sidecar {label}."));
            continue;
        };
        if !candidate.starts_with(root) {
            warnings.push(format!(
                "Ignored sidecar {label} outside the acquired folder."
            ));
            continue;
        }
        match displayed_dimensions(&candidate) {
            Ok(dimensions) if dimensions == (expected[0], expected[1]) => {
                warnings.push(format!(
                    "Using sidecar {label}; compression, color, crop, or content may differ."
                ));
                return (Some(candidate), warnings);
            }
            Ok((width, height)) => warnings.push(format!(
                "Ignored sidecar {label}: {width}×{height} does not match {}×{}.",
                expected[0], expected[1]
            )),
            Err(_) => warnings.push(format!("Ignored undecodable sidecar {label}.")),
        }
    }
    (None, warnings)
}

#[cfg(not(target_os = "macos"))]
fn checked_expected_pixels(photo: &Photo) -> Result<u64, CommandError> {
    let [width, height] = photo.dimensions.oriented;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| command_error("decode-limit", "The photo dimensions are too large."))?;
    if width == 0 || height == 0 || pixels > MAX_HEIC_PIXELS {
        return Err(command_error(
            "decode-limit",
            format!(
                "The photo exceeds the native display limit of {MAX_HEIC_PIXELS} oriented pixels."
            ),
        ));
    }
    Ok(pixels)
}

#[cfg(not(target_os = "macos"))]
fn u16_samples_to_bytes(samples: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_ne_bytes());
    }
    bytes
}

#[cfg(not(target_os = "macos"))]
fn write_decoded_png(path: &Path, decoded: heif_oxide::DecodedImage) -> Result<(), CommandError> {
    let temporary = path.with_extension("png.tmp");
    let result = (|| {
        let file = File::create(&temporary)
            .map_err(|_| command_error("io", "The decoded photo cache could not be created."))?;
        let mut writer = BufWriter::new(file);
        let encoder = image::codecs::png::PngEncoder::new(&mut writer);
        let (bytes, color) = match decoded.pixels {
            heif_oxide::Pixels::Rgb8(samples) => (samples, ExtendedColorType::Rgb8),
            heif_oxide::Pixels::Rgba8(samples) => (samples, ExtendedColorType::Rgba8),
            heif_oxide::Pixels::Rgb16(samples) => {
                (u16_samples_to_bytes(&samples), ExtendedColorType::Rgb16)
            }
            heif_oxide::Pixels::Rgba16(samples) => {
                (u16_samples_to_bytes(&samples), ExtendedColorType::Rgba16)
            }
        };
        encoder
            .write_image(&bytes, decoded.width, decoded.height, color)
            .map_err(|_| {
                command_error("decode", "The lossless display PNG could not be encoded.")
            })?;
        writer
            .flush()
            .map_err(|_| command_error("io", "The decoded photo cache could not be flushed."))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|_| command_error("io", "The decoded photo cache could not be flushed."))?;
        std::fs::rename(&temporary, path)
            .map_err(|_| command_error("io", "The decoded photo cache could not be committed."))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(target_os = "macos"))]
fn decode_heic_to_cache(
    app: &AppHandle,
    original: &Path,
    photo: &Photo,
) -> Result<PathBuf, CommandError> {
    let expected_pixels = checked_expected_pixels(photo)?;
    let compressed_bytes = std::fs::metadata(original)
        .map_err(|_| command_error("io", "The HEIC source metadata could not be read."))?
        .len();
    if compressed_bytes > MAX_HEIC_COMPRESSED_BYTES {
        return Err(command_error(
            "decode-limit",
            format!(
                "The HEIC source exceeds the native compressed-size limit of {} MiB.",
                MAX_HEIC_COMPRESSED_BYTES / (1024 * 1024)
            ),
        ));
    }
    let digest = cache::sha256_file(original).map_err(CommandError::from)?;
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|_| command_error("io", "Application cache storage is unavailable."))?
        .join("photo-sources");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|_| command_error("io", "Application cache storage could not be created."))?;
    let display = cache_dir.join(format!("{digest}-{HEIC_CACHE_SUFFIX}"));
    let expected_dimensions = (photo.dimensions.oriented[0], photo.dimensions.oriented[1]);
    if display.is_file() && displayed_dimensions(&display).ok() == Some(expected_dimensions) {
        return Ok(display);
    }

    let source_scope = app.state::<SourceScope>();
    let _decoder = source_scope.decoder.enter(&digest)?;
    if display.is_file() && displayed_dimensions(&display).ok() == Some(expected_dimensions) {
        return Ok(display);
    }
    let _ = std::fs::remove_file(&display);

    let mut bytes = Vec::with_capacity(compressed_bytes as usize);
    File::open(original)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|_| command_error("io", "The HEIC source could not be read."))?;
    let decoded = heif_oxide::decode_bytes(&bytes)
        .map_err(|_| command_error("decode", "The HEIC source could not be decoded."))?;
    drop(bytes);
    let decoded_pixels = u64::from(decoded.width)
        .checked_mul(u64::from(decoded.height))
        .ok_or_else(|| command_error("decode-limit", "Decoded dimensions are too large."))?;
    if decoded_pixels > MAX_HEIC_PIXELS || decoded_pixels != expected_pixels {
        return Err(dimension_mismatch(photo.dimensions.oriented));
    }
    if [decoded.width, decoded.height] != photo.dimensions.oriented {
        return Err(dimension_mismatch(photo.dimensions.oriented));
    }
    write_decoded_png(&display, decoded)?;
    Ok(display)
}

fn resolve_photo_source_sync(
    app: &AppHandle,
    photo: &Photo,
) -> Result<ResolvePhotoSourceResponse, CommandError> {
    let root = app.state::<SourceScope>().root()?;
    let original = resolve_relative_photo(&root, &photo.path)?;
    let expected = photo.dimensions.oriented;
    let (source_kind, display_path, warnings) = match display_source_format(&original)? {
        SourceFormat::Jpeg | SourceFormat::Png => {
            if displayed_dimensions(&original)? != (expected[0], expected[1]) {
                return Err(dimension_mismatch(expected));
            }
            (PhotoSourceKind::Native, original.clone(), Vec::new())
        }
        SourceFormat::Heif => {
            #[cfg(target_os = "macos")]
            {
                // WKWebView can decode HEIC natively. Preserve the original so any
                // intrinsic HDR or auxiliary gain-map rendition reaches the HTML
                // image compositor instead of being flattened by heif-oxide.
                (
                    PhotoSourceKind::Native,
                    original.clone(),
                    vec![
                        "Using the original HEIC in macOS WKWebView; native HDR output is unverified."
                            .to_string(),
                    ],
                )
            }
            #[cfg(not(target_os = "macos"))]
            {
                let (sidecar, mut warnings) = find_sidecar(&root, &original, expected);
                if let Some(sidecar) = sidecar {
                    (PhotoSourceKind::Sidecar, sidecar, warnings)
                } else {
                    let decoded = decode_heic_to_cache(app, &original, photo)?;
                    warnings.push(
                        "Decoded the HEIC source to an SDR PNG in application cache storage."
                            .to_string(),
                    );
                    (PhotoSourceKind::Decoded, decoded, warnings)
                }
            }
        }
    };
    let display_format = match display_source_format(&display_path)? {
        SourceFormat::Jpeg => PhotoDisplayFormat::Jpeg,
        SourceFormat::Png => PhotoDisplayFormat::Png,
        SourceFormat::Heif => PhotoDisplayFormat::Heic,
    };
    app.asset_protocol_scope()
        .allow_file(&display_path)
        .map_err(|_| {
            command_error(
                "asset-scope",
                "The display photo could not be added to the native asset scope.",
            )
        })?;
    Ok(ResolvePhotoSourceResponse {
        photo_id: photo.id.clone(),
        source_kind,
        display_format,
        display_path,
        original_path: original,
        warnings,
    })
}

/// Resolve one document photo without transferring image bytes through IPC.
#[tauri::command]
pub async fn resolve_photo_source(
    app: AppHandle,
    photo: Photo,
) -> Result<ResolvePhotoSourceResponse, CommandError> {
    let worker_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || resolve_photo_source_sync(&worker_app, &photo))
        .await
        .map_err(|_| command_error("join", "Photo source resolution did not complete."))?
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
        let (first_id, first) = state.begin();
        assert!(!first.is_cancelled());
        let (second_id, second) = state.begin();
        assert!(first.is_cancelled(), "a new run must stop the old one");
        assert!(!second.is_cancelled());
        state.finish(first_id);
        assert!(
            state.cancel(),
            "a stale completion must not clear the active run"
        );
        assert!(second.is_cancelled());
        state.finish(second_id);
        assert!(!state.cancel());
    }

    #[test]
    fn decode_gate_bounds_parallelism_and_serializes_matching_cache_keys() {
        use std::sync::{mpsc, Arc};
        use std::time::Duration;

        let gate = Arc::new(DecodeGate::default());
        let first = gate.enter("first").unwrap();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let second_gate = Arc::clone(&gate);
        let second = std::thread::spawn(move || {
            let _permit = second_gate.enter("second").unwrap();
            acquired_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();

        let (third_tx, third_rx) = mpsc::channel();
        let third_gate = Arc::clone(&gate);
        let third = std::thread::spawn(move || {
            let _permit = third_gate.enter("third").unwrap();
            third_tx.send(()).unwrap();
        });
        assert!(third_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        third_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release_tx.send(()).unwrap();
        second.join().unwrap();
        third.join().unwrap();

        let first = gate.enter("same").unwrap();
        let (matching_tx, matching_rx) = mpsc::channel();
        let matching_gate = Arc::clone(&gate);
        let matching = std::thread::spawn(move || {
            let _permit = matching_gate.enter("same").unwrap();
            matching_tx.send(()).unwrap();
        });
        assert!(matching_rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(first);
        matching_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        matching.join().unwrap();
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

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("camcomp-desktop-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        image::RgbImage::new(width, height).save(path).unwrap();
    }

    #[test]
    fn preflight_uses_the_real_common_root_and_safe_labels() {
        let dir = temp_dir("common-root");
        let left = dir.join("left");
        let right = dir.join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let a = left.join("a.jpg");
        let b = right.join("b.png");
        std::fs::write(&a, b"jpeg fixture").unwrap();
        std::fs::write(&b, b"png fixture").unwrap();

        let acquired = acquire_source(&[a, b]).unwrap();
        assert_eq!(acquired.root, dir.canonicalize().unwrap());
        assert_eq!(acquired.response.photo_count, 2);
        assert_eq!(
            acquired.response.source_root_label,
            dir.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(
            acquired.response.write_target_label,
            format!(
                "{}/{}",
                acquired.response.source_root_label,
                cache::CACHE_FILE_NAME
            )
        );
        let json = serde_json::to_string(&acquired.response).unwrap();
        assert!(!json.contains(&dir.to_string_lossy().to_string()), "{json}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_selections_and_filesystem_roots_are_invalid_write_targets() {
        let empty = acquire_source(&[]).unwrap_err();
        assert_eq!(empty.code, "invalid-write-target");
        #[cfg(unix)]
        {
            let root = validate_source_root(Path::new("/")).unwrap_err();
            assert_eq!(root.code, "invalid-write-target");
            assert!(root.message.contains("containing photo folder"));
        }
    }

    #[test]
    fn loading_a_folder_without_an_analysis_reports_it_clearly() {
        let dir = temp_dir("empty");
        let error = selected_document(dir.clone()).unwrap_err();
        assert_eq!(error.code, "io");
        assert!(error.message.contains("camcomp.json"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_photo_paths_reject_absolute_and_parent_traversal() {
        let dir = temp_dir("traversal");
        for path in [
            "../outside.jpg",
            "nested/../../outside.jpg",
            "/tmp/photo.jpg",
            "C:/photo.jpg",
            r"nested\photo.jpg",
        ] {
            let error = resolve_relative_photo(&dir, path).unwrap_err();
            assert_eq!(error.code, "invalid-photo-path", "{path}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn relative_photo_paths_reject_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("symlink-root");
        let outside = temp_dir("symlink-outside");
        let outside_photo = outside.join("outside.jpg");
        std::fs::write(&outside_photo, b"outside").unwrap();
        symlink(&outside_photo, dir.join("inside.jpg")).unwrap();

        let error = resolve_relative_photo(&dir.canonicalize().unwrap(), "inside.jpg").unwrap_err();
        assert_eq!(error.code, "invalid-photo-path");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn display_format_prefers_jpeg_magic_over_a_heic_extension() {
        let dir = temp_dir("mislabeled-jpeg");
        let source = dir.join("photo.heic");
        image::RgbImage::new(4, 3)
            .save_with_format(&source, image::ImageFormat::Jpeg)
            .unwrap();

        assert_eq!(display_source_format(&source).unwrap(), SourceFormat::Jpeg);
        assert_eq!(displayed_dimensions(&source).unwrap(), (4, 3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sidecars_follow_order_and_skip_dimension_mismatches() {
        let dir = temp_dir("sidecars");
        let original = dir.join("photo.heic");
        std::fs::write(&original, b"heic fixture").unwrap();
        write_png(&original.with_extension("jpg"), 10, 10);
        write_png(&original.with_extension("jpeg"), 40, 20);
        write_png(&original.with_extension("png"), 40, 20);

        let root = dir.canonicalize().unwrap();
        let original = original.canonicalize().unwrap();
        let (selected, warnings) = find_sidecar(&root, &original, [40, 20]);
        assert_eq!(selected, Some(original.with_extension("jpeg")));
        assert!(warnings.iter().any(|warning| warning.contains("10×10")));

        write_png(&original.with_extension("jpg"), 40, 20);
        let (selected, _) = find_sidecar(&root, &original, [40, 20]);
        assert_eq!(selected, Some(original.with_extension("jpg")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn loaded_documents_receive_semantic_validation() {
        let dir = temp_dir("semantic-validation");
        let path = dir.join(cache::CACHE_FILE_NAME);
        let mut document = AnalysisDocument::default();
        document.ungrouped.push(camcomp_core::schema::Ungrouped {
            photo_id: "missing-photo".into(),
            reason: camcomp_core::schema::UngroupedReason::UnreadableFile,
            detail: None,
        });
        cache::save_atomic(&path, &document).unwrap();

        let error = load_validated_document(&path).unwrap_err();
        assert_eq!(error.code, "inconsistent-document");
        assert!(error.message.contains("unknown photo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolved_source_response_serializes_without_image_bytes() {
        let response = ResolvePhotoSourceResponse {
            photo_id: "photo-1".into(),
            source_kind: PhotoSourceKind::Sidecar,
            display_format: PhotoDisplayFormat::Jpeg,
            display_path: PathBuf::from("/source/photo.jpg"),
            original_path: PathBuf::from("/source/photo.heic"),
            warnings: vec!["sidecar selected".into()],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""photoId":"photo-1""#), "{json}");
        assert!(json.contains(r#""sourceKind":"sidecar""#), "{json}");
        assert!(json.contains(r#""displayFormat":"jpeg""#), "{json}");
        assert!(
            json.contains(r#""displayPath":"/source/photo.jpg""#),
            "{json}"
        );
        assert!(!json.contains("bytes"), "{json}");
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
