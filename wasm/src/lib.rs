//! WebAssembly bindings for pdf-handouts.
//!
//! The browser build does exactly what `pdf-handouts build` does: merge a set
//! of PDFs and images, then stamp headers and footers onto the result. All of
//! it runs in the page, so the files never leave the reader's machine — which
//! matters for handouts carrying student names.
//!
//! Files are added one at a time with [`HandoutBuilder::add_file`], then
//! [`HandoutBuilder::build`] returns the finished PDF. Page order is whatever
//! order they were added in; the caller decides, not this crate.

use pdf_handouts::build::build_handout;
use pdf_handouts::date::{parse_date_expression, resolve_date};
use pdf_handouts::pdf::{FitAction, FitMode, FontSpec, HeaderFooterOptions, NamedInput};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// Install the panic hook so a Rust panic shows up as a readable console
/// message rather than an opaque `unreachable` trap.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Options accepted from JavaScript, mirroring the CLI's flags.
///
/// Every field is optional; omitted ones fall back to the same defaults the
/// command line uses.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct JsOptions {
    title: Option<String>,
    footer_left: Option<String>,
    footer_center: Option<String>,
    footer_right: Option<String>,
    /// A date expression: "today", "2026-07-28", "Tuesday+1", ...
    date: Option<String>,
    /// Font spec, e.g. "24pt #1a4d8f"
    header_font: Option<String>,
    /// Font spec, e.g. "14pt #555555"
    footer_font: Option<String>,
    /// "auto", "shift" or "off"
    fit: Option<String>,
}

/// Blank strings arrive from empty form fields; treat them as absent.
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty())
}

impl JsOptions {
    fn into_header_footer_options(self) -> Result<HeaderFooterOptions, String> {
        let date = match non_empty(self.date) {
            Some(expr) => {
                let parsed = parse_date_expression(&expr)
                    .map_err(|_| format!("Could not understand the date \"{}\"", expr))?;
                Some(
                    resolve_date(&parsed)
                        .ok_or_else(|| format!("Could not resolve the date \"{}\"", expr))?,
                )
            }
            None => None,
        };

        let header_font = non_empty(self.header_font).map(|s| FontSpec::parse(&s));
        let footer_font = non_empty(self.footer_font).map(|s| FontSpec::parse(&s));

        let fit = match self.fit.as_deref() {
            None | Some("auto") => FitMode::Auto,
            Some("shift") => FitMode::ShiftOnly,
            Some("off") => FitMode::Off,
            Some(other) => return Err(format!("Unknown fit mode \"{}\"", other)),
        };

        Ok(HeaderFooterOptions {
            title: non_empty(self.title),
            footer_left: non_empty(self.footer_left),
            footer_center: non_empty(self.footer_center),
            footer_right: non_empty(self.footer_right),
            date,
            title_font_size: header_font.as_ref().and_then(|f| f.size).unwrap_or(24.0),
            footer_font_size: footer_font.as_ref().and_then(|f| f.size).unwrap_or(14.0),
            header_font,
            footer_font,
            fit,
            ..Default::default()
        })
    }
}

/// A finished handout, handed back to JavaScript.
#[wasm_bindgen]
pub struct BuildResult {
    pdf: Vec<u8>,
    notes: Vec<String>,
}

#[wasm_bindgen]
impl BuildResult {
    /// The generated PDF.
    #[wasm_bindgen(getter)]
    pub fn pdf(&self) -> Vec<u8> {
        self.pdf.clone()
    }

    /// Human-readable notes about pages whose content had to be adjusted.
    #[wasm_bindgen(getter)]
    pub fn notes(&self) -> Vec<String> {
        self.notes.clone()
    }
}

/// Collects input files, then builds the handout.
#[wasm_bindgen]
#[derive(Default)]
pub struct HandoutBuilder {
    inputs: Vec<NamedInput>,
}

#[wasm_bindgen]
impl HandoutBuilder {
    /// Start an empty builder.
    #[wasm_bindgen(constructor)]
    pub fn new() -> HandoutBuilder {
        HandoutBuilder { inputs: Vec::new() }
    }

    /// Add one input file. Pages appear in the order files are added.
    ///
    /// The name is used only to identify the file in error messages; the file's
    /// type is determined from its contents, not its extension.
    pub fn add_file(&mut self, name: &str, data: &[u8]) {
        self.inputs.push(NamedInput {
            name: name.to_string(),
            data: data.to_vec(),
        });
    }

    /// How many files have been added.
    #[wasm_bindgen(getter)]
    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    /// Whether no files have been added yet.
    #[wasm_bindgen(getter)]
    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }

    /// Merge the inputs and stamp headers and footers onto the result.
    ///
    /// `options_json` is a JSON object; see the CLI flags for what each field
    /// means. Errors come back as JavaScript exceptions carrying the same
    /// message the command line would print.
    pub fn build(&self, options_json: &str) -> Result<BuildResult, JsError> {
        let raw: JsOptions = serde_json::from_str(options_json)
            .map_err(|e| JsError::new(&format!("Could not read the options: {}", e)))?;

        let options = raw
            .into_header_footer_options()
            .map_err(|e| JsError::new(&e))?;

        let handout =
            build_handout(&self.inputs, &options).map_err(|e| JsError::new(&format!("{}", e)))?;

        let notes = handout
            .fits
            .iter()
            .filter_map(|fit| match fit.action {
                FitAction::Shifted(dy) => Some(format!(
                    "Page {}: moved content {:.0}pt clear of the title/footer",
                    fit.page,
                    dy.abs()
                )),
                FitAction::Scaled(scale) => Some(format!(
                    "Page {}: scaled content to {:.0}% to fit",
                    fit.page,
                    scale * 100.0
                )),
                FitAction::Unchanged => None,
            })
            .collect();

        Ok(BuildResult {
            pdf: handout.pdf,
            notes,
        })
    }
}

/// Report whether a file looks like something the builder can accept.
///
/// Lets the page reject an unusable drop immediately rather than after a build.
#[wasm_bindgen]
pub fn describe_input(name: &str, data: &[u8]) -> String {
    use pdf_handouts::pdf::{detect_input_kind_from_bytes, InputKind};

    match detect_input_kind_from_bytes(name, data) {
        Ok(InputKind::Pdf) => "PDF".to_string(),
        Ok(InputKind::Image(format)) => format.name().to_string(),
        Err(_) => String::new(),
    }
}
