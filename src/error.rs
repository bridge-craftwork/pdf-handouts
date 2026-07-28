//! Error types for PDF handouts library

use std::path::PathBuf;
use thiserror::Error;

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the PDF handouts library
#[derive(Error, Debug)]
pub enum Error {
    /// PDF processing error
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Date parsing error
    #[error("Invalid date expression: {0}")]
    InvalidDateExpression(String),

    /// File not found
    #[error("File not found: {}", .0.display())]
    FileNotFound(PathBuf),

    /// Invalid glob pattern
    #[error("Invalid glob pattern: {0}")]
    InvalidGlob(String),

    /// No files matched pattern
    #[error("No PDF files found matching pattern: {0}")]
    NoFilesMatched(String),

    /// Invalid PDF (no pages)
    #[error("PDF has no pages: {}", .0.display())]
    EmptyPdf(PathBuf),

    /// Input file is neither a PDF nor a supported image
    #[error("Unsupported input file: {} (supported formats: {})", .0.display(), crate::pdf::SUPPORTED_INPUT_FORMATS)]
    UnsupportedInput(PathBuf),

    /// Image file could not be decoded
    #[error("Failed to decode {format} image: {}", .path.display())]
    ImageDecode {
        /// Path to the image that failed to decode
        path: PathBuf,
        /// Detected image format name (e.g. "PNG")
        format: String,
    },

    /// Font error
    #[error("Font error: {0}")]
    Font(String),

    /// General error
    #[error("{0}")]
    General(String),
}
