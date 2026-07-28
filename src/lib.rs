//! PDF Handouts Library
//!
//! A cross-platform library for merging PDFs and adding headers/footers.
//! This library provides functionality to:
//! - Merge multiple PDF files
//! - Extract metadata (page counts, etc.)
//! - Create watermark PDFs with headers and footers
//! - Parse flexible date expressions
//! - Calculate page layouts
//!
//! # Example
//!
//! ```no_run
//! use pdf_handouts::pdf::{MergeOptions, merge_pdfs};
//! use std::path::PathBuf;
//!
//! let options = MergeOptions {
//!     input_paths: vec![
//!         PathBuf::from("1. intro.pdf"),
//!         PathBuf::from("2. advanced.pdf"),
//!     ],
//!     output_path: PathBuf::from("merged.pdf"),
//! };
//!
//! merge_pdfs(&options).expect("Failed to merge PDFs");
//! ```

pub mod build;
pub mod date;
pub mod error;
pub mod layout;
pub mod pdf;

// Re-export commonly used items
pub use error::{Error, Result};
