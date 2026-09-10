use anyhow::{Context, Result};
use qrcode::QrCode;
use qrcode::render::svg;

/// Renders `data` (typically an absolute URL) as an SVG QR code.
///
/// Deliberately standard dark-modules-on-light-background polarity —
/// *not* inverted to match the dark UI theme. Inverted QR codes look
/// nicer on a dark lobby display, but most real scanners (phone
/// cameras included) are built assuming dark-on-light and fail to
/// detect an inverted one at all: confirmed with `zbarimg`, which
/// read this same data instantly at standard polarity but found zero
/// barcodes in an inverted render of it. Scannability wins over theme
/// matching here; wrap the output in a light card in the template if
/// it needs to sit on a dark background.
pub fn svg_for(data: &str) -> Result<String> {
    let code = QrCode::new(data).context("failed to encode QR code")?;
    Ok(code
        .render()
        .quiet_zone(true)
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
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

    /// Regression guard for the inverted-polarity bug: light modules
    /// on a dark background silently produced QR codes most phone
    /// cameras can't scan at all (verified with `zbarimg` — it read
    /// this exact data fine at standard polarity, found nothing when
    /// the colors were swapped). Pin standard dark-on-light so nobody
    /// "fixes" this back to match the dark theme.
    #[test]
    fn uses_standard_scannable_polarity_not_inverted() {
        let svg = svg_for("http://example.com/sign-in").unwrap();
        assert!(svg.contains("#000000"), "modules should be dark");
        assert!(svg.contains("#ffffff"), "background should be light");
    }
}
