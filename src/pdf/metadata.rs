//! PDF metadata extraction

use crate::error::{Error, Result};
use lopdf::{Document, Object};
use std::path::Path;

/// Count pages by reading the Count field from the Pages dictionary
/// This is more reliable than get_pages() which doesn't handle nested page trees
fn count_pages_from_catalog(doc: &Document) -> Result<usize> {
    // Get the catalog (root)
    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .map_err(|_| Error::General("No Root in trailer".to_string()))?;

    let catalog_id = match catalog_ref {
        Object::Reference(id) => *id,
        _ => return Err(Error::General("Root is not a reference".to_string())),
    };

    let catalog = doc.get_object(catalog_id).map_err(Error::Pdf)?;

    let catalog_dict = match catalog {
        Object::Dictionary(dict) => dict,
        _ => return Err(Error::General("Catalog is not a dictionary".to_string())),
    };

    // Get the Pages reference
    let pages_ref = catalog_dict
        .get(b"Pages")
        .map_err(|_| Error::General("No Pages in catalog".to_string()))?;

    let pages_id = match pages_ref {
        Object::Reference(id) => *id,
        _ => return Err(Error::General("Pages is not a reference".to_string())),
    };

    let pages_obj = doc.get_object(pages_id).map_err(Error::Pdf)?;

    let pages_dict = match pages_obj {
        Object::Dictionary(dict) => dict,
        _ => return Err(Error::General("Pages is not a dictionary".to_string())),
    };

    // Get the Count field
    let count = pages_dict
        .get(b"Count")
        .map_err(|_| Error::General("No Count in Pages".to_string()))?;

    match count {
        Object::Integer(n) => Ok(*n as usize),
        _ => Err(Error::General("Count is not an integer".to_string())),
    }
}

/// PDF metadata
#[derive(Debug, Clone)]
pub struct PdfMetadata {
    /// Number of pages in the PDF
    pub page_count: usize,
    /// Document title (if present)
    pub title: Option<String>,
    /// Document author (if present)
    pub author: Option<String>,
}

/// Extract metadata from a PDF file
pub fn extract_metadata(path: &Path) -> Result<PdfMetadata> {
    if !path.exists() {
        return Err(Error::FileNotFound(path.to_path_buf()));
    }

    let doc = Document::load(path)?;

    // Use catalog-based counting for accuracy
    let page_count = count_pages_from_catalog(&doc)?;

    if page_count == 0 {
        return Err(Error::EmptyPdf(path.to_path_buf()));
    }

    // Try to extract title and author from Info dictionary
    let mut title = None;
    let mut author = None;

    if let Ok(Object::Reference(ref_id)) = doc.trailer.get(b"Info") {
        if let Ok(Object::Dictionary(info_dict)) = doc.get_object(*ref_id) {
            // Extract title
            if let Ok(title_obj) = info_dict.get(b"Title") {
                if let Ok(title_bytes) = title_obj.as_str() {
                    if let Ok(title_string) = String::from_utf8(title_bytes.to_vec()) {
                        title = Some(title_string);
                    }
                }
            }

            // Extract author
            if let Ok(author_obj) = info_dict.get(b"Author") {
                if let Ok(author_bytes) = author_obj.as_str() {
                    if let Ok(author_string) = String::from_utf8(author_bytes.to_vec()) {
                        author = Some(author_string);
                    }
                }
            }
        }
    }

    Ok(PdfMetadata {
        page_count,
        title,
        author,
    })
}

/// Count the number of pages in a PDF file
///
/// This is a quick operation that reads the Count field from the Pages dictionary.
pub fn count_pages(path: &Path) -> Result<usize> {
    if !path.exists() {
        return Err(Error::FileNotFound(path.to_path_buf()));
    }

    let doc = Document::load(path)?;
    let page_count = count_pages_from_catalog(&doc)?;

    if page_count == 0 {
        return Err(Error::EmptyPdf(path.to_path_buf()));
    }

    Ok(page_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_pages_nonexistent_file() {
        let result = count_pages(Path::new("nonexistent.pdf"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::FileNotFound(_)));
    }

    #[test]
    fn test_extract_metadata_nonexistent_file() {
        let result = extract_metadata(Path::new("nonexistent.pdf"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::FileNotFound(_)));
    }

    // Integration tests with actual PDFs will be in tests/ directory
}
