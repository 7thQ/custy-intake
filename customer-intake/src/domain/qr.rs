use anyhow::{Context, Result};
use qrcode::QrCode;
use qrcode::render::svg;

/// Renders `data` (typically an absolute URL) as an SVG QR code.
/// Colors match the dark UI theme rather than a plain white square, so
/// it sits naturally on the lobby display.
pub fn svg_for(data: &str) -> Result<String> {
    let code = QrCode::new(data).context("failed to encode QR code")?;
    Ok(code
        .render()
        .quiet_zone(true)
        .dark_color(svg::Color("#f4f5f7"))
        .light_color(svg::Color("#14161b"))
        .build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_url_as_svg() {
        let svg = svg_for("http://example.com/sign-in").unwrap();
        assert!(svg.contains("<svg"));
    }
}
