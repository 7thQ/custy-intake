mod boxes;
mod pages;
mod render;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use boxes::{BoxDraft, DragState};
use forms_core::FieldKind;

const RENDER_DPI: f32 = 150.0;

#[derive(Clone)]
enum Screen {
    Home,
    FileList,
    Calibrate(PathBuf),
}

struct CalibratorApp {
    screen: Screen,
    status: String,
    files: Vec<PathBuf>,
    page_texture: Option<(PathBuf, egui::TextureHandle)>,
    page_error: Option<String>,
    /// Boxes drawn so far, keyed by page filename (e.g.
    /// `customer_forms-page-1.pdf`) so switching pages doesn't lose work.
    boxes_by_page: HashMap<String, Vec<BoxDraft>>,
    selected: Option<usize>,
    drag: DragState,
    /// In-progress "drag out a new box" rectangle, in image-pixel space.
    draft_rect_px: Option<egui::Rect>,
    schema_status: String,
}

impl Default for CalibratorApp {
    fn default() -> Self {
        Self {
            screen: Screen::Home,
            status: String::new(),
            files: Vec::new(),
            page_texture: None,
            page_error: None,
            boxes_by_page: HashMap::new(),
            selected: None,
            drag: DragState::None,
            draft_rect_px: None,
            schema_status: String::new(),
        }
    }
}

impl CalibratorApp {
    fn open_document(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_title("Open a scanned document")
            .pick_file()
        else {
            return;
        };

        self.status = match pages::import_pdf(&path) {
            Ok(written) => format!(
                "Imported '{}' into {} page file(s) in {}.",
                path.display(),
                written.len(),
                pages::pages_dir().display()
            ),
            Err(err) => format!("Failed to import document: {err:#}"),
        };
        self.screen = Screen::Home;
    }

    fn open_file_list(&mut self) {
        match pages::list_pages() {
            Ok(files) => {
                self.files = files;
                self.screen = Screen::FileList;
            }
            Err(err) => {
                self.status = format!("Failed to list pages: {err:#}");
            }
        }
    }

    fn show_home(&mut self, ui: &mut egui::Ui) {
        ui.heading("Document Calibrator");
        ui.separator();

        if ui.button("Open PDF...").clicked() {
            self.open_document();
        }

        if ui.button("Calibrate a Page...").clicked() {
            self.open_file_list();
        }

        if !self.status.is_empty() {
            ui.add_space(8.0);
            ui.label(&self.status);
        }
    }

    fn show_file_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Select a Page to Calibrate");
        ui.separator();

        if ui.button("< Back").clicked() {
            self.screen = Screen::Home;
            return;
        }

        ui.add_space(8.0);

        if self.files.is_empty() {
            ui.label(format!("No pages found in {}.", pages::pages_dir().display()));
        } else {
            for file in self.files.clone() {
                let name = file
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if ui.button(name).clicked() {
                    self.screen = Screen::Calibrate(file);
                }
            }
        }
    }

    fn show_calibrate(&mut self, ui: &mut egui::Ui, path: &Path) {
        ui.heading("Calibrate");
        ui.separator();

        if ui.button("< Back").clicked() {
            self.screen = Screen::Home;
            return;
        }

        ui.add_space(8.0);
        ui.label(format!("Opened: {}", path.display()));

        let already_loaded = self
            .page_texture
            .as_ref()
            .is_some_and(|(loaded_path, _)| loaded_path == path);

        let page_file = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if !already_loaded {
            match render::render_page(path, RENDER_DPI as u32) {
                Ok(page) => {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [page.width, page.height],
                        &page.rgba,
                    );
                    let texture = ui.ctx().load_texture(
                        path.display().to_string(),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.page_texture = Some((path.to_path_buf(), texture));
                    self.page_error = None;
                    self.load_existing_boxes(path, &page_file);
                }
                Err(err) => {
                    self.page_texture = None;
                    self.page_error = Some(format!("Failed to render page: {err:#}"));
                }
            }
        }

        ui.add_space(8.0);

        if let Some(error) = &self.page_error {
            ui.colored_label(egui::Color32::RED, error);
            return;
        }

        let Some((_, texture)) = self.page_texture.clone() else {
            return;
        };

        // Compute the display scale against the *real* on-screen available
        // space, before entering the nested horizontal/vertical layout and
        // the scroll area below — ui.available_size() reports something
        // very different (and useless for fitting) once called from
        // inside those, which is what made the page render tiny.
        const PANEL_WIDTH: f32 = 280.0;
        let outer_available = ui.available_size();
        let img_size = texture.size_vec2();
        let canvas_available = egui::vec2(
            (outer_available.x - PANEL_WIDTH - 24.0).max(50.0),
            outer_available.y,
        );
        let scale = (canvas_available.x / img_size.x)
            .min(canvas_available.y / img_size.y)
            .min(1.0);
        let display_size = img_size * scale;

        ui.horizontal(|ui| {
            // No ScrollArea here: `scale` above already guarantees
            // `display_size` fits inside `canvas_available`, so the whole
            // page is always fully visible. A ScrollArea's default
            // viewport sizing clipped this down to a sliver, since it
            // doesn't know our content is already bounded to fit.
            ui.vertical(|ui| {
                self.draw_canvas(ui, &texture, &page_file, display_size, scale);
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.set_width(PANEL_WIDTH);
                self.show_field_panel(ui, path, &page_file);
            });
        });
    }

    fn draw_canvas(
        &mut self,
        ui: &mut egui::Ui,
        texture: &egui::TextureHandle,
        page_file: &str,
        display_size: egui::Vec2,
        scale: f32,
    ) {
        let (response, painter) = ui.allocate_painter(display_size, egui::Sense::click_and_drag());
        let img_origin = response.rect.min;

        painter.image(
            texture.id(),
            response.rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        let to_image_px =
            |p: egui::Pos2| egui::pos2((p.x - img_origin.x) / scale, (p.y - img_origin.y) / scale);

        let page_boxes = self.boxes_by_page.entry(page_file.to_string()).or_default();

        // --- interaction ---
        if response.drag_started() {
            if let Some(start) = response.interact_pointer_pos() {
                let mut started = false;

                if let Some(sel) = self.selected {
                    if let Some(b) = page_boxes.get(sel) {
                        let handle = boxes::rotate_handle_screen(b, img_origin, scale);
                        if start.distance(handle) <= 10.0 {
                            self.drag = DragState::Rotating { index: sel };
                            started = true;
                        }
                    }
                }

                if !started {
                    if let Some(idx) = boxes::hit_test(page_boxes, start, img_origin, scale) {
                        self.selected = Some(idx);
                        let center_screen = img_origin + page_boxes[idx].center_px.to_vec2() * scale;
                        self.drag = DragState::Moving {
                            index: idx,
                            grab_offset_px: (start - center_screen) / scale,
                        };
                    } else {
                        self.selected = None;
                        self.drag = DragState::Creating {
                            start_px: to_image_px(start),
                        };
                    }
                }
            }
        }

        if response.dragged() {
            if let Some(cur) = response.interact_pointer_pos() {
                match self.drag {
                    DragState::Creating { start_px } => {
                        self.draft_rect_px = Some(egui::Rect::from_two_pos(start_px, to_image_px(cur)));
                    }
                    DragState::Moving { index, grab_offset_px } => {
                        if let Some(b) = page_boxes.get_mut(index) {
                            b.center_px = to_image_px(cur) - grab_offset_px;
                        }
                    }
                    DragState::Rotating { index } => {
                        if let Some(b) = page_boxes.get_mut(index) {
                            let center_screen = img_origin + b.center_px.to_vec2() * scale;
                            let v = cur - center_screen;
                            b.rotation_deg = v.y.atan2(v.x).to_degrees() + 90.0;
                        }
                    }
                    DragState::None => {}
                }
            }
        }

        if response.drag_stopped() {
            if let DragState::Creating { .. } = std::mem::take(&mut self.drag) {
                if let Some(rect) = self.draft_rect_px.take() {
                    let size = rect.size().abs();
                    if size.x > 4.0 && size.y > 4.0 {
                        page_boxes.push(BoxDraft::new(rect.center(), size));
                        self.selected = Some(page_boxes.len() - 1);
                    }
                }
            }
            self.draft_rect_px = None;
        }

        // --- drawing ---
        for (idx, b) in page_boxes.iter().enumerate() {
            let selected = self.selected == Some(idx);
            let color = if selected {
                egui::Color32::from_rgb(255, 200, 0)
            } else {
                egui::Color32::from_rgb(0, 200, 255)
            };
            let corners = boxes::corners_screen(b, img_origin, scale);
            painter.add(egui::Shape::closed_line(
                corners.to_vec(),
                egui::epaint::PathStroke::new(2.0, color),
            ));

            let label = if b.field_key.is_empty() {
                "(unnamed)"
            } else {
                b.field_key.as_str()
            };
            painter.text(
                corners[0] + egui::vec2(2.0, -14.0),
                egui::Align2::LEFT_BOTTOM,
                label,
                egui::FontId::proportional(12.0),
                color,
            );

            if selected {
                let handle = boxes::rotate_handle_screen(b, img_origin, scale);
                let center = img_origin + b.center_px.to_vec2() * scale;
                painter.line_segment([center, handle], egui::Stroke::new(1.5, color));
                painter.circle_filled(handle, 6.0, color);
            }
        }

        if let Some(rect) = self.draft_rect_px {
            let min = img_origin + rect.min.to_vec2() * scale;
            let max = img_origin + rect.max.to_vec2() * scale;
            painter.rect_stroke(
                egui::Rect::from_min_max(min, max),
                0.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 200, 255)),
                egui::StrokeKind::Middle,
            );
        }
    }

    fn show_field_panel(&mut self, ui: &mut egui::Ui, page_path: &Path, page_file: &str) {
        ui.heading("Fields");
        ui.label("Drag on the image to add a box.");
        ui.label("Drag the dot above a selected box to rotate it.");
        ui.separator();

        // Save button and status are pinned above the (potentially long,
        // scrolling) box list, so they're always reachable no matter how
        // many fields you've drawn or how far you've scrolled.
        if ui.button("Save Schema").clicked() {
            self.save_schema(page_path);
        }
        if !self.schema_status.is_empty() {
            ui.add_space(4.0);
            ui.label(&self.schema_status);
        }
        ui.separator();

        let boxes = self.boxes_by_page.entry(page_file.to_string()).or_default();

        if boxes.is_empty() {
            ui.label("No fields on this page yet.");
            return;
        }

        let mut to_delete = None;
        egui::ScrollArea::vertical()
            .id_salt("field_panel_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (idx, b) in boxes.iter_mut().enumerate() {
                    let selected = self.selected == Some(idx);
                    ui.group(|ui| {
                        let title = if b.field_key.is_empty() {
                            "(unnamed)".to_string()
                        } else {
                            b.field_key.clone()
                        };
                        if ui.selectable_label(selected, title).clicked() {
                            self.selected = Some(idx);
                        }

                        ui.horizontal(|ui| {
                            ui.label("field_key:");
                            ui.text_edit_singleline(&mut b.field_key);
                        });
                        ui.horizontal(|ui| {
                            ui.label("label:");
                            ui.text_edit_singleline(&mut b.label);
                        });
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut b.kind, FieldKind::Text, "Text");
                            ui.radio_value(&mut b.kind, FieldKind::Checkbox, "Checkbox");
                        });
                        ui.label(format!("rotation: {:.0} deg", b.rotation_deg));

                        if ui.button("Delete").clicked() {
                            to_delete = Some(idx);
                        }
                    });
                }
            });

        if let Some(idx) = to_delete {
            boxes.remove(idx);
            self.selected = match self.selected {
                Some(sel) if sel == idx => None,
                Some(sel) if sel > idx => Some(sel - 1),
                other => other,
            };
        }
    }

    /// Loads existing fields out of each sibling page's own
    /// `<page>.schema.json` (if present) into `boxes_by_page`, so
    /// reopening a page you've already calibrated shows your existing
    /// boxes instead of a blank slate. Checks every page belonging to
    /// the same document, not just the one being opened, so switching
    /// between pages doesn't require re-saving first. Pages already
    /// present in `boxes_by_page` (loaded earlier, or drawn this
    /// session) are left untouched.
    fn load_existing_boxes(&mut self, page_path: &Path, page_file: &str) {
        let pages_dir = page_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(pages::pages_dir);
        let source = document_stem(page_file);

        let Ok(sibling_pages) = pages::list_pages_in(&pages_dir) else {
            return;
        };

        let mut loaded_count = 0;
        for sibling_path in sibling_pages {
            let sibling_file = sibling_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if document_stem(&sibling_file) != source {
                continue;
            }
            if self.boxes_by_page.contains_key(&sibling_file) {
                continue;
            }

            let schema_path = page_schema_path(&pages_dir, &sibling_file);
            let Ok(schema) = forms_core::Schema::load(&schema_path) else {
                continue;
            };
            let Some(page_info) = schema.pages.iter().find(|p| p.file == sibling_file) else {
                continue;
            };

            let loaded: Vec<BoxDraft> = schema
                .fields
                .iter()
                .filter(|f| f.page_file == sibling_file)
                .map(|f| boxes::from_field(f, RENDER_DPI, page_info.height_pt))
                .collect();
            loaded_count += loaded.len();
            self.boxes_by_page.insert(sibling_file, loaded);
        }

        if loaded_count > 0 {
            self.schema_status = format!("Loaded {loaded_count} existing field(s) for {source}.");
        }
    }

    /// Saves every page of the current document that has boxes drawn
    /// this session, each into its own `<page>.schema.json` next to
    /// that page's PDF — one schema file per page, not one shared file
    /// per document. A page's own file is only ever touched when that
    /// page itself has boxes in memory, so this can't clobber a
    /// sibling page's already-saved file.
    fn save_schema(&mut self, current_page_path: &Path) {
        let pages_dir = current_page_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(pages::pages_dir);

        let current_page_file = current_page_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let source = document_stem(&current_page_file);

        let page_files: Vec<String> = self
            .boxes_by_page
            .keys()
            .filter(|pf| document_stem(pf) == source)
            .cloned()
            .collect();

        let mut saved_files = 0;
        let mut saved_fields = 0;

        for page_file in page_files {
            let draft_boxes = &self.boxes_by_page[&page_file];
            if draft_boxes.is_empty() {
                continue;
            }

            let page_path = pages_dir.join(&page_file);
            let geometry = match load_page_geometry(&page_path) {
                Ok(g) => g,
                Err(err) => {
                    self.schema_status = format!("Failed to read {page_file}: {err:#}");
                    return;
                }
            };

            let fields: Vec<forms_core::Field> = draft_boxes
                .iter()
                .filter(|b| !b.field_key.trim().is_empty())
                .map(|b| boxes::to_field(b, RENDER_DPI, geometry.display_height, &page_file))
                .collect();

            let schema = forms_core::Schema {
                source: page_file.clone(),
                pages: vec![forms_core::PageInfo {
                    file: page_file.clone(),
                    width_pt: geometry.display_width,
                    height_pt: geometry.display_height,
                }],
                fields,
            };

            let out_path = page_schema_path(&pages_dir, &page_file);
            if let Err(err) = schema.save(&out_path) {
                self.schema_status = format!("Failed to save {}: {err:#}", out_path.display());
                return;
            }
            saved_fields += schema.fields.len();
            saved_files += 1;
        }

        self.schema_status =
            format!("Saved {saved_fields} field(s) across {saved_files} page schema file(s).");
    }
}

/// The source document a split page file came from, derived from its
/// name (`import_pdf` names pages `<stem>-page-N.pdf`). Used to keep
/// each document's schema separate: `boxes_by_page` accumulates pages
/// from every document calibrated in a session, but a schema should
/// only describe the one document currently open.
fn document_stem(page_file: &str) -> String {
    let stem = Path::new(page_file)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| page_file.to_string());
    stem.split("-page-").next().unwrap_or(&stem).to_string()
}

/// Where one page's own schema file lives: `<page-file-stem>.schema.json`
/// next to that page's PDF, e.g. `customer_forms-page-1.schema.json`.
fn page_schema_path(pages_dir: &Path, page_file: &str) -> PathBuf {
    pages_dir.join(Path::new(page_file).with_extension("schema.json"))
}

fn load_page_geometry(page_path: &Path) -> anyhow::Result<forms_core::PageGeometry> {
    let doc = lopdf::Document::load(page_path)?;
    let page_id = *doc
        .get_pages()
        .values()
        .next()
        .ok_or_else(|| anyhow::anyhow!("page file contains no pages"))?;
    forms_core::page_geometry(&doc, page_id)
}

impl eframe::App for CalibratorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        match self.screen.clone() {
            Screen::Home => self.show_home(ui),
            Screen::FileList => self.show_file_list(ui),
            Screen::Calibrate(path) => self.show_calibrate(ui, &path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_stem_strips_page_suffix_and_extension() {
        assert_eq!(document_stem("customer_forms-page-1.pdf"), "customer_forms");
        assert_eq!(document_stem("other_doc-page-12.pdf"), "other_doc");
    }

    /// Two different source documents calibrated in the same session
    /// must not bleed into each other's schema files: saving docA's
    /// fields should not touch docB's, and vice versa.
    #[test]
    fn save_schema_keeps_documents_separate() {
        let real_page = pages::pages_dir().join("customer_forms-page-1.pdf");
        let dir = std::env::temp_dir().join("doc-calibrator-multi-doc-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let doc_a_page = dir.join("docA-page-1.pdf");
        let doc_b_page = dir.join("docB-page-1.pdf");
        std::fs::copy(&real_page, &doc_a_page).unwrap();
        std::fs::copy(&real_page, &doc_b_page).unwrap();

        let mut app = CalibratorApp::default();
        let mut box_a = BoxDraft::new(egui::pos2(100.0, 100.0), egui::vec2(50.0, 20.0));
        box_a.field_key = "field_a".to_string();
        app.boxes_by_page
            .insert("docA-page-1.pdf".to_string(), vec![box_a]);

        let mut box_b = BoxDraft::new(egui::pos2(200.0, 200.0), egui::vec2(50.0, 20.0));
        box_b.field_key = "field_b".to_string();
        app.boxes_by_page
            .insert("docB-page-1.pdf".to_string(), vec![box_b]);

        app.save_schema(&doc_a_page);

        let schema_a_path = dir.join("docA-page-1.schema.json");
        let schema_b_path = dir.join("docB-page-1.schema.json");
        assert!(
            schema_a_path.exists(),
            "docA-page-1.schema.json should be written"
        );
        assert!(
            !schema_b_path.exists(),
            "saving docA must not create docB-page-1.schema.json"
        );

        let schema_a = forms_core::Schema::load(&schema_a_path).unwrap();
        assert_eq!(schema_a.fields.len(), 1);
        assert_eq!(schema_a.fields[0].field_key, "field_a");

        app.save_schema(&doc_b_page);
        let schema_b = forms_core::Schema::load(&schema_b_path).unwrap();
        assert_eq!(schema_b.fields.len(), 1);
        assert_eq!(schema_b.fields[0].field_key, "field_b");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The bug that bit us: saving after only having one page of a
    /// multi-page document loaded into memory must not touch a sibling
    /// page's already-saved file at all. With one schema file per page,
    /// that's true by construction — saving page 2 only ever writes
    /// `<...>-page-2.schema.json`.
    #[test]
    fn save_schema_never_touches_a_sibling_pages_file() {
        let real_page = pages::pages_dir().join("customer_forms-page-1.pdf");
        let dir = std::env::temp_dir().join("doc-calibrator-partial-save-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let doc_page1 = dir.join("docC-page-1.pdf");
        let doc_page2 = dir.join("docC-page-2.pdf");
        std::fs::copy(&real_page, &doc_page1).unwrap();
        std::fs::copy(&real_page, &doc_page2).unwrap();

        // Page 1 was already calibrated (in an earlier session, or
        // never opened this session) and has its own saved file.
        let page1_schema = forms_core::Schema {
            source: "docC-page-1.pdf".to_string(),
            pages: vec![forms_core::PageInfo {
                file: "docC-page-1.pdf".to_string(),
                width_pt: 612.0,
                height_pt: 792.0,
            }],
            fields: vec![forms_core::Field {
                id: "docC-page-1.pdf#page1_field".to_string(),
                field_key: "page1_field".to_string(),
                label: "Page 1 Field".to_string(),
                page_file: "docC-page-1.pdf".to_string(),
                kind: FieldKind::Text,
                rect: forms_core::RotatedRect {
                    cx: 100.0,
                    cy: 100.0,
                    width: 50.0,
                    height: 20.0,
                    rotation_deg: 0.0,
                },
            }],
        };
        let page1_schema_path = dir.join("docC-page-1.schema.json");
        page1_schema.save(&page1_schema_path).unwrap();
        let page1_bytes_before = std::fs::read(&page1_schema_path).unwrap();

        // This session only ever opened page 2 - boxes_by_page has no
        // entry at all for page 1.
        let mut app = CalibratorApp::default();
        let mut box2 = BoxDraft::new(egui::pos2(300.0, 300.0), egui::vec2(50.0, 20.0));
        box2.field_key = "page2_field".to_string();
        app.boxes_by_page
            .insert("docC-page-2.pdf".to_string(), vec![box2]);

        app.save_schema(&doc_page2);

        let page1_bytes_after = std::fs::read(&page1_schema_path).unwrap();
        assert_eq!(
            page1_bytes_before, page1_bytes_after,
            "page 1's schema file must be byte-for-byte untouched by a page-2-only save"
        );

        let page2_schema =
            forms_core::Schema::load(&dir.join("docC-page-2.schema.json")).unwrap();
        assert_eq!(page2_schema.fields.len(), 1);
        assert_eq!(page2_schema.fields[0].field_key, "page2_field");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Opening one page of a document should pull every sibling page's
    /// own schema file into memory too, not just the one being viewed -
    /// this is what lets one Save Schema click persist everything
    /// you've worked on across the document's pages, each into its own
    /// file.
    #[test]
    fn load_existing_boxes_loads_every_sibling_pages_file() {
        let real_page = pages::pages_dir().join("customer_forms-page-1.pdf");
        let dir = std::env::temp_dir().join("doc-calibrator-load-whole-doc-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // list_pages_in() (used to discover sibling pages) scans for
        // actual .pdf files, so these need to exist alongside the
        // schema files below, same as in the real pages/ directory.
        std::fs::copy(&real_page, dir.join("docD-page-1.pdf")).unwrap();
        std::fs::copy(&real_page, dir.join("docD-page-2.pdf")).unwrap();

        let page1_schema = forms_core::Schema {
            source: "docD-page-1.pdf".to_string(),
            pages: vec![forms_core::PageInfo {
                file: "docD-page-1.pdf".to_string(),
                width_pt: 612.0,
                height_pt: 792.0,
            }],
            fields: vec![forms_core::Field {
                id: "docD-page-1.pdf#f1".to_string(),
                field_key: "f1".to_string(),
                label: String::new(),
                page_file: "docD-page-1.pdf".to_string(),
                kind: FieldKind::Text,
                rect: forms_core::RotatedRect {
                    cx: 100.0,
                    cy: 100.0,
                    width: 50.0,
                    height: 20.0,
                    rotation_deg: 0.0,
                },
            }],
        };
        page1_schema
            .save(&dir.join("docD-page-1.schema.json"))
            .unwrap();

        let page2_schema = forms_core::Schema {
            source: "docD-page-2.pdf".to_string(),
            pages: vec![forms_core::PageInfo {
                file: "docD-page-2.pdf".to_string(),
                width_pt: 612.0,
                height_pt: 792.0,
            }],
            fields: vec![forms_core::Field {
                id: "docD-page-2.pdf#f2".to_string(),
                field_key: "f2".to_string(),
                label: String::new(),
                page_file: "docD-page-2.pdf".to_string(),
                kind: FieldKind::Text,
                rect: forms_core::RotatedRect {
                    cx: 200.0,
                    cy: 200.0,
                    width: 50.0,
                    height: 20.0,
                    rotation_deg: 0.0,
                },
            }],
        };
        page2_schema
            .save(&dir.join("docD-page-2.schema.json"))
            .unwrap();

        let mut app = CalibratorApp::default();
        app.load_existing_boxes(&dir.join("docD-page-1.pdf"), "docD-page-1.pdf");

        assert!(app.boxes_by_page.contains_key("docD-page-1.pdf"));
        assert!(
            app.boxes_by_page.contains_key("docD-page-2.pdf"),
            "opening page 1 should also have pulled page 2's saved fields into memory"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Document Calibrator",
        options,
        Box::new(|_cc| Ok(Box::new(CalibratorApp::default()))),
    )
}
