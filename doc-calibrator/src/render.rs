use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub struct RenderedPage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// Rasterizes a single-page PDF to RGBA pixels at `dpi`, by shelling out to
/// the system `pdftoppm` (poppler-utils). Requires poppler-utils to be
/// installed and on PATH.
pub fn render_page(pdf_path: &Path, dpi: u32) -> Result<RenderedPage> {
    let prefix: PathBuf = std::env::temp_dir().join(format!(
        "doc-calibrator-preview-{}-{}",
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
    let img = image::open(&png_path)
        .with_context(|| format!("failed to read rendered page {}", png_path.display()))?;
    let _ = std::fs::remove_file(&png_path);

    let rgba = img.to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;

    Ok(RenderedPage {
        width,
        height,
        rgba: rgba.into_raw(),
    })
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
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("pages/customer_forms-page-1.pdf");
        let page = render_page(&path, 150).unwrap();
        assert!(page.width > 100 && page.height > 100);
        assert_eq!(page.rgba.len(), page.width * page.height * 4);
    }
}
