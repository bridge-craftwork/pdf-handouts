//! PDF manipulation module

pub mod bounds;
pub mod create;
pub mod fit;
pub mod headers;
pub mod image;
pub mod merge;
pub mod metadata;

// Re-export commonly used items
pub use create::{create_watermark_pdf, WatermarkOptions};
pub use fit::{FitAction, FitMode};
pub use headers::{
    add_headers_footers, add_headers_footers_reporting, FontSpec, HeaderFooterOptions, MaskOptions,
    PageFit,
};
pub use image::{
    detect_input_kind, image_to_pdf, image_to_pdf_bytes, load_input_document, ImageFormat,
    InputKind, SUPPORTED_INPUT_FORMATS,
};
pub use merge::{merge_pdfs, overlay_watermark, MergeOptions};
pub use metadata::{count_pages, extract_metadata, PdfMetadata};
