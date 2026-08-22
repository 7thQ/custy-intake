use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Every imported document page lives here. Delegates to `forms_core`
/// so `doc-calibrator` and `customer-intake` always agree on the same
/// physical folder.
pub fn pages_dir() -> PathBuf {
    forms_core::pages_dir()
}

/// Imports a source PDF into `pages_dir()`, splitting it into one file per
/// page (a single-page source still produces one `-page-1.pdf` file, so
/// everything in the folder follows the same naming). Returns the paths written.
pub fn import_pdf(source: &Path) -> Result<Vec<PathBuf>> {
    import_pdf_into(source, &pages_dir())
}

/// Same as [`import_pdf`] but into an arbitrary directory instead of
/// the hardcoded default. Exists so tests can exercise this without
/// writing into the same shared directory the app (and other tests
/// that only read from it) use — overwriting real page files in place
/// while another test reads them is a real race, not a hypothetical
/// one.
pub fn import_pdf_into(source: &Path, dir: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string());

    let doc =
        lopdf::Document::load(source).with_context(|| format!("failed to parse {}", source.display()))?;
    let page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();

    let mut written = Vec::with_capacity(page_numbers.len());
    for &page_number in &page_numbers {
        let mut single = doc.clone();
        let others: Vec<u32> = page_numbers
            .iter()
            .copied()
            .filter(|&n| n != page_number)
            .collect();
        single.delete_pages(&others);
        single.prune_objects();
        single.renumber_objects();
        single.compress();

        let out_path = dir.join(format!("{stem}-page-{page_number}.pdf"));
        single
            .save(&out_path)
            .with_context(|| format!("failed to save {}", out_path.display()))?;
        written.push(out_path);
    }

    Ok(written)
}

/// Lists all PDF files currently stored in `pages_dir()`, sorted by name.
pub fn list_pages() -> Result<Vec<PathBuf>> {
    list_pages_in(&pages_dir())
}

/// Lists all PDF files in an arbitrary directory, sorted by name. Same
/// as [`list_pages`] but not tied to the hardcoded default dir, so
/// callers working with a page that might live elsewhere (e.g. tests
/// using a temp directory) list the right place.
pub fn list_pages_in(dir: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("failed to read {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        })
        .collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_and_lists() {
        // Uses an isolated temp dir, not the real pages_dir(): other
        // tests read/copy the real customer_forms-page-*.pdf files as
        // fixtures, and cargo runs tests in parallel by default, so
        // writing into that same shared directory races with those
        // reads (import_pdf's save() can be caught mid-write).
        let dir = std::env::temp_dir().join("doc-calibrator-import-test");
        let _ = fs::remove_dir_all(&dir);

        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../customer_forms.pdf");
        let written = import_pdf_into(&source, &dir).unwrap();
        assert_eq!(written.len(), 3);
        for p in &written {
            assert!(p.exists());
        }
        let listed = list_pages_in(&dir).unwrap();
        assert_eq!(listed.len(), 3);

        fs::remove_dir_all(&dir).unwrap();
    }
}
