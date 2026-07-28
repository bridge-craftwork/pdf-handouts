//! The whole handout pipeline in one call, entirely in memory.
//!
//! [`build_handout`] merges a set of inputs and stamps headers and footers onto
//! the result, which is exactly what the `build` subcommand does. Keeping it
//! here — free of any filesystem access — means the WebAssembly build and the
//! command line run the same code and produce byte-identical output.

use crate::error::Result;
use crate::pdf::headers::{add_headers_footers_bytes, HeaderFooterOptions, PageFit};
use crate::pdf::merge::{merge_documents, NamedInput};

/// A finished handout.
#[derive(Debug, Clone)]
pub struct Handout {
    /// The generated PDF
    pub pdf: Vec<u8>,
    /// Per-page record of how source content was adjusted to clear the bands
    pub fits: Vec<PageFit>,
}

/// Merge inputs and add headers and footers, returning the finished PDF.
///
/// Inputs are used in the order given — no sorting happens here, so the caller
/// decides page order. Each may be a PDF or a raster image; anything else is an
/// error naming the offending input rather than being silently skipped.
///
/// # Example
///
/// ```no_run
/// use pdf_handouts::build::build_handout;
/// use pdf_handouts::pdf::{HeaderFooterOptions, NamedInput};
///
/// let inputs = vec![NamedInput {
///     name: "1. intro.pdf".to_string(),
///     data: std::fs::read("1. intro.pdf").expect("readable"),
/// }];
///
/// let options = HeaderFooterOptions {
///     title: Some("Workshop Handout".to_string()),
///     ..Default::default()
/// };
///
/// let handout = build_handout(&inputs, &options).expect("build succeeds");
/// std::fs::write("out.pdf", handout.pdf).expect("writable");
/// ```
pub fn build_handout(inputs: &[NamedInput], options: &HeaderFooterOptions) -> Result<Handout> {
    let merged = merge_documents(inputs)?;
    let (pdf, fits) = add_headers_footers_bytes("merged document", &merged, options)?;

    Ok(Handout { pdf, fits })
}
