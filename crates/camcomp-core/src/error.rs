//! Typed errors for the analysis core.
//!
//! Errors carry the *relative* source path where one is known, so that hosted
//! deployments do not leak absolute filesystem layout into logs
//! (`PLAN-DROP1.md` §4).

use std::path::PathBuf;

/// Result alias used throughout the core.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything the analysis core can fail with.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The input path does not exist or cannot be listed.
    #[error("input `{path}` cannot be read: {source}")]
    Io {
        /// Offending path, relative to the source root when one is known.
        path: PathBuf,
        /// Underlying cause.
        source: std::io::Error,
    },

    /// An I/O failure with no single file to blame.
    #[error("i/o failure: {0}")]
    BareIo(#[from] std::io::Error),

    /// The file extension or magic bytes are not a format Drop 1 supports.
    #[error("`{path}` is not a supported image format (Drop 1 supports JPEG, PNG and HEIC)")]
    UnsupportedFormat {
        /// Offending path.
        path: PathBuf,
    },

    /// The file looks like a supported format but could not be decoded.
    #[error("`{path}` could not be decoded: {detail}")]
    Decode {
        /// Offending path.
        path: PathBuf,
        /// Decoder message.
        detail: String,
    },

    /// The image exceeds the configured pixel guard.
    #[error("`{path}` is {pixels} pixels which exceeds the {limit} pixel guard")]
    ImageTooLarge {
        /// Offending path.
        path: PathBuf,
        /// Pixel count of the offending image.
        pixels: u64,
        /// Configured guard.
        limit: u64,
    },

    /// `camcomp.json` is syntactically invalid.
    #[error("`{path}` is not valid JSON: {detail}")]
    MalformedDocument {
        /// Offending path.
        path: PathBuf,
        /// Parser message.
        detail: String,
    },

    /// `camcomp.json` declares a schema major version this build cannot read.
    #[error(
        "`{path}` declares schema version {found}; this build reads major version {supported_major}"
    )]
    SchemaVersionMismatch {
        /// Offending path.
        path: PathBuf,
        /// Version string found in the document.
        found: String,
        /// Major version this build supports.
        supported_major: u32,
    },

    /// A manual override refers to something that does not exist, or would
    /// produce a structurally invalid document.
    #[error("manual override rejected: {0}")]
    InvalidOverride(String),

    /// The document failed an internal consistency check.
    #[error("analysis document is inconsistent: {0}")]
    InconsistentDocument(String),

    /// The caller cancelled the run.
    #[error("analysis cancelled")]
    Cancelled,

    /// Configuration values that cannot produce a meaningful analysis.
    #[error("invalid analysis parameters: {0}")]
    InvalidParameters(String),
}

impl Error {
    /// Short machine-readable discriminant, for adapters that need a stable
    /// error code across the FFI/IPC boundary.
    pub fn code(&self) -> &'static str {
        match self {
            Error::Io { .. } | Error::BareIo(_) => "io",
            Error::UnsupportedFormat { .. } => "unsupported-format",
            Error::Decode { .. } => "decode",
            Error::ImageTooLarge { .. } => "image-too-large",
            Error::MalformedDocument { .. } => "malformed-document",
            Error::SchemaVersionMismatch { .. } => "schema-version-mismatch",
            Error::InvalidOverride(_) => "invalid-override",
            Error::InconsistentDocument(_) => "inconsistent-document",
            Error::Cancelled => "cancelled",
            Error::InvalidParameters(_) => "invalid-parameters",
        }
    }
}
