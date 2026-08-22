use anyhow::{Context, Result};
use lopdf::{Document, Object, ObjectId};

/// Describes a page's coordinate systems and how to reconcile them.
///
/// PDF pages carry their content in "raw" `MediaBox` space, but a
/// `/Rotate` entry can tell viewers (and rasterizers like `pdftoppm`) to
/// spin that content 90/180/270 degrees for display. Our scanned forms
/// are rotated this way (landscape-scanned pages displayed as portrait),
/// so a naive fill using raw MediaBox coordinates would place text
/// sideways or in the wrong spot.
///
/// Everywhere else in this crate (and in the calibrator), field
/// geometry is authored and stored in **display space**: the same
/// coordinate system as the rasterized preview image the operator draws
/// boxes on (origin bottom-left, y-up, matching what a viewer actually
/// shows). `correction` is the `cm` matrix that maps display-space
/// coordinates back into raw content space, so callers only ever need
/// to think in display space.
#[derive(Debug, Clone, Copy)]
pub struct PageGeometry {
    pub raw_width: f32,
    pub raw_height: f32,
    pub rotate: i64,
    pub display_width: f32,
    pub display_height: f32,
    /// `[a, b, c, d, e, f]` operands for a PDF `cm` operator.
    pub correction: [f32; 6],
}

pub fn page_geometry(doc: &Document, page_id: ObjectId) -> Result<PageGeometry> {
    let dict = doc.get_object(page_id)?.as_dict()?;

    let media_box = dict
        .get(b"MediaBox")
        .ok()
        .and_then(|o| o.as_array().ok())
        .and_then(|arr| parse_box(arr))
        .context("page is missing a usable /MediaBox")?;

    let rotate = dict
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);

    let raw_width = media_box.2 - media_box.0;
    let raw_height = media_box.3 - media_box.1;

    let (correction, display_width, display_height) = match rotate {
        90 => (
            [0.0, 1.0, -1.0, 0.0, raw_width, 0.0],
            raw_height,
            raw_width,
        ),
        180 => (
            [-1.0, 0.0, 0.0, -1.0, raw_width, raw_height],
            raw_width,
            raw_height,
        ),
        270 => (
            [0.0, -1.0, 1.0, 0.0, 0.0, raw_height],
            raw_height,
            raw_width,
        ),
        _ => ([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], raw_width, raw_height),
    };

    Ok(PageGeometry {
        raw_width,
        raw_height,
        rotate,
        display_width,
        display_height,
        correction,
    })
}

fn parse_box(arr: &[Object]) -> Option<(f32, f32, f32, f32)> {
    if arr.len() != 4 {
        return None;
    }
    let nums: Vec<f32> = arr.iter().filter_map(|o| o.as_float().ok()).collect();
    if nums.len() != 4 {
        return None;
    }
    Some((nums[0], nums[1], nums[2], nums[3]))
}
