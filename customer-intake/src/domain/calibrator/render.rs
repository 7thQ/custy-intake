use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Rasterizes a single-page PDF to PNG bytes at `dpi`, by shelling out
/// to the system `pdftoppm` (poppler-utils). Requires poppler-utils to
/// be installed and on PATH. Returns raw PNG bytes rather than decoded
/// pixels — the browser decodes the image itself, so there's no need
/// to round-trip through an in-process image decoder here.
pub fn render_page_png(pdf_path: &Path, dpi: u32) -> Result<Vec<u8>> {
    let prefix: PathBuf = std::env::temp_dir().join(format!(
        "admin-calibrator-preview-{}-{}",
        std::process::id(),
        rand_suffix()
    ));

    let status = Command::new("pdftoppm")
        .arg("-png")
        .arg("-singlefile")
        .arg("-r")
        .arg(dpi.to_string())
        .arg(pdf_path)
        .arg(&prefix)
        .status()
        .context("failed to run pdftoppm (is poppler-utils installed and on PATH?)")?;

    if !status.success() {
        bail!("pdftoppm exited with status {status}");
    }

    let png_path = prefix.with_extension("png");
    let bytes = std::fs::read(&png_path)
        .with_context(|| format!("failed to read rendered page {}", png_path.display()))?;
    let _ = std::fs::remove_file(&png_path);

    Ok(bytes)
}

/// Small non-cryptographic suffix so concurrent renders don't collide on
/// the same temp file name.
fn rand_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_real_page() {
        let path = forms_core::pages_dir().join("customer_forms-page-1.pdf");
        let png = render_page_png(&path, 150).unwrap();
        // PNG signature: 89 50 4E 47 0D 0A 1A 0A
        assert_eq!(&png[..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
