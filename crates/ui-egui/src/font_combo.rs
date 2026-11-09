//! Reusable font selection combo box offering Standard PDF fonts,
//! Auto-discovered Thai / System fonts, and Windows Installed Fonts.

use egui::Ui;

/// The user's selection in the font combobox.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FontSelection {
    /// Standard PDF font family: "Helvetica", "Times", "Courier"
    Standard(&'static str),
    /// A discovered Thai / system font entry
    Thai(pdfcraft_fonts::ThaiFontEntry),
    /// A font file installed on this computer
    Installed(std::path::PathBuf),
}

/// Render a font selection combo box.
///
/// Returns `Some(FontSelection)` when a new font is chosen by the user.
/// When a Thai or installed font is chosen, it automatically activates the Thai font via
/// `pdfcraft_fonts::set_active_thai_font_by_path` and refreshes egui font definitions.
pub(crate) fn font_combo_box(
    ui: &mut Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    current_label: &str,
    width: Option<f32>,
) -> Option<FontSelection> {
    let mut chosen = None;

    let mut combo = craft_ui::ComboBox::from_id_salt(id_salt).selected_text(current_label);
    if let Some(w) = width {
        combo = combo.width(w);
    }

    combo.show_ui(ui, |ui| {
        // 1. Standard fonts
        for std_name in ["Helvetica", "Times", "Courier"] {
            let selected = current_label.eq_ignore_ascii_case(std_name)
                || (std_name == "Times" && current_label.eq_ignore_ascii_case("Times Roman"));
            let display_name = if std_name == "Times" { "Times Roman" } else { std_name };
            if ui.selectable_label(selected, display_name).clicked() {
                chosen = Some(FontSelection::Standard(std_name));
            }
        }

        // 2. Discovered Thai / System Fonts
        #[cfg(not(target_arch = "wasm32"))]
        {
            let thai_fonts = pdfcraft_fonts::available_thai_fonts();
            if !thai_fonts.is_empty() {
                ui.separator();
                ui.label(
                    egui::RichText::new(tl!("Thai / System fonts"))
                        .small()
                        .color(ui.visuals().text_color().gamma_multiply(0.6)),
                );
                for entry in &thai_fonts {
                    let selected = current_label == entry.name || current_label == entry.family;
                    if ui.selectable_label(selected, &entry.name).clicked() {
                        chosen = Some(FontSelection::Thai(entry.clone()));
                    }
                }
            }
        }

        // 3. Installed fonts on computer
        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.separator();
            ui.menu_button(tl!("Installed fonts…"), |ui| {
                ui.set_min_width(260.0);
                let reload = ui.button(tl!("Reload")).clicked();
                match crate::files::installed_font_files(ui.ctx(), reload) {
                    Some(list) if list.is_empty() => {
                        ui.label(tl!("No openly licensed document fonts found. Load a font file."));
                    }
                    Some(list) => {
                        let search_id = ui.scope_id().with("font-picker-search");
                        let mut query = ui.data(|d| d.get_temp::<String>(search_id)).unwrap_or_default();
                        ui.horizontal(|ui| {
                            let (icon_rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                            crate::icons::paint(ui, icon_rect, "search", 13.0, ui.visuals().text_color().gamma_multiply(0.6));
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut query)
                                    .hint_text(tl!("Search…"))
                                    .desired_width(ui.available_width() - 8.0),
                            );
                            if resp.changed() {
                                ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
                            }
                        });
                        ui.separator();

                        let query_lower = query.trim().to_lowercase();
                        let mut families: std::collections::BTreeMap<&str, Vec<&pdfcraft_engine::text_fonts::FontFile>> =
                            std::collections::BTreeMap::new();
                        for font in &list {
                            if query_lower.is_empty()
                                || font.family.to_lowercase().contains(&query_lower)
                                || font.face.to_lowercase().contains(&query_lower)
                            {
                                families.entry(&font.family).or_default().push(font);
                            }
                        }

                        if families.is_empty() {
                            ui.label(tl!("No openly licensed document fonts found. Load a font file."));
                        } else {
                            egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                                for (family, faces) in families {
                                    if faces.len() == 1 {
                                        if let Some(&font) = faces.first() {
                                            let label = if font.face != font.family && !font.face.is_empty() {
                                                font.face.as_str()
                                            } else {
                                                family
                                            };
                                            if ui.button(label).clicked() {
                                                chosen = Some(FontSelection::Installed(font.path.clone()));
                                                ui.close();
                                            }
                                        }
                                    } else {
                                        ui.menu_button(family, |ui| {
                                            ui.set_min_width(180.0);
                                            for font in faces {
                                                let label = font
                                                    .face
                                                    .strip_prefix(family)
                                                    .map(str::trim)
                                                    .filter(|s| !s.is_empty())
                                                    .unwrap_or(font.face.as_str());
                                                if ui.button(label).clicked() {
                                                    chosen = Some(FontSelection::Installed(font.path.clone()));
                                                    ui.close();
                                                }
                                            }
                                        });
                                    }
                                }
                            });
                        }
                    }
                    None => {
                        ui.label(tl!("Loading fonts…"));
                    }
                }
            });
        }
    });

    if let Some(ref sel) = chosen {
        apply_font_selection(ui, sel);
    }

    chosen
}

/// Activate font selection and update egui font definitions
fn apply_font_selection(ui: &mut Ui, sel: &FontSelection) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let path_opt: Option<&std::path::Path> = match sel {
            FontSelection::Thai(entry) => Some(&entry.path),
            FontSelection::Installed(p) => Some(p.as_path()),
            FontSelection::Standard(_) => None,
        };

        if let Some(path) = path_opt
            && pdfcraft_fonts::set_active_thai_font_by_path(path).is_ok()
        {
            let cjk = crate::theme::CjkPreference::for_language(crate::i18n::current().code());
            ui.ctx().set_fonts(crate::theme::installed_font_definitions(cjk));
        }
    }
}
