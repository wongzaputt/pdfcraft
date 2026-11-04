//! pdfcraft-ocr — Scan & OCR ▸ Recognize text (L4).
//!
//! Recognition runs the ocrs engine (MIT/Apache-2.0) with its pre-trained models (CC-BY-SA-4.0,
//! fetched by `cargo xtask models`; see ATTRIBUTION.toml). The caller renders a page to pixels;
//! [`Ocr::recognize`] finds the words in them, and [`text_layer`] turns words placed in user space
//! into page content: invisible text (rendering mode 3) over each word, so the page becomes a
//! searchable image (Acrobat's "Searchable Image (Exact)": the image is left untouched).
//!
//! The models read the Latin alphabet (English and other languages written without accents).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::{Path, PathBuf};

pub use pdfcraft_fonts::helvetica_width;

/// The model files, as named in ATTRIBUTION.toml.
pub const DETECTION_MODEL: &str = "text-detection.onnx";
pub const RECOGNITION_MODEL: &str = "text-recognition.onnx";
pub const THAI_RECOGNITION_MODEL: &str = "text-recognition-tha.onnx";

/// The recognition model's alphabet, as it was trained (ocrs-models' `DEFAULT_ALPHABET`): its
/// output classes follow this order. The ocrs crate's own copy has an `E` where the training had
/// `€`, which made a scanned "€250" read as "E250".
pub const ALPHABET: &str = " 0123456789!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~€ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Thai Unicode block characters for OCR alphabet extension.
pub const THAI_ALPHABET: &str = " กขฃคฅฆงจฉชซฌญฎฏฐฑฒณดตถทธนบปผฝพฟภมยรฤลฦวศษสหฬอฮฯะัาำิีึืฺุู฿เแโใไๅๆ็่้๊๋์ํ๎๏๐๑๒๓๔๕๖๗๘๙๚๛";

/// The languages the models read (ISO 639-1).
pub const LANGUAGES: &[(&str, &str)] = &[("en", "English"), ("tha", "Thai (ภาษาไทย)")];

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    /// Shown to end users (release packages ship the models; a source build fetches them with
    /// `cargo xtask models`). The UI catalogs key this exact text.
    #[error("the text recognition models are not installed (reinstall PdfCraft, or set PDFCRAFT_MODELS to the folder that holds them)")]
    NoModels,
    #[error("loading {0}: {1}")]
    Load(String, String),
    #[error("text recognition failed: {0}")]
    Recognize(String),
    #[error("the image is empty")]
    EmptyImage,
}

/// Where the two model files are.
#[derive(Clone, Debug, PartialEq)]
pub struct Models {
    pub detection: PathBuf,
    pub recognition: PathBuf,
}

impl Models {
    /// Look for models in `dir`, considering language.
    pub fn in_dir_with_lang(dir: &Path, lang: &str) -> Option<Models> {
        let is_thai = lang.eq_ignore_ascii_case("tha") || lang.starts_with("th");
        if is_thai {
            for (det, rec) in [(DETECTION_MODEL, THAI_RECOGNITION_MODEL), ("text-detection.rten", "text-recognition-tha.rten")] {
                let m = Models { detection: dir.join(det), recognition: dir.join(rec) };
                if m.detection.is_file() && m.recognition.is_file() {
                    return Some(m);
                }
            }
        }
        Self::in_dir(dir)
    }

    /// A complete model pair in `dir`, preferring current ONNX files over legacy RTEN files.
    /// An incomplete update never mixes formats or hides an intact legacy pair.
    pub fn in_dir(dir: &Path) -> Option<Models> {
        [(DETECTION_MODEL, RECOGNITION_MODEL), ("text-detection.rten", "text-recognition.rten")].into_iter().find_map(|(detection, recognition)| {
            let m = Models { detection: dir.join(detection), recognition: dir.join(recognition) };
            (m.detection.is_file() && m.recognition.is_file()).then_some(m)
        })
    }

    /// Look for the models: `$PDFCRAFT_MODELS`, then where the release packages install them
    /// beside the executable ([`Models::dirs_beside_exe`]), then the source tree's `assets/models/`.
    pub fn find() -> Option<Models> {
        Self::find_with_lang("en")
    }

    /// Look for the models for `lang`.
    pub fn find_with_lang(lang: &str) -> Option<Models> {
        Self::search_dirs().iter().find_map(|d| Self::in_dir_with_lang(d, lang))
    }

    /// The directories [`Models::find`] looks in, in order.
    pub fn search_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(d) = std::env::var_os("PDFCRAFT_MODELS") {
            dirs.push(PathBuf::from(d));
        }
        if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            dirs.extend(Self::dirs_beside_exe(&exe));
        }
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models"));
        dirs
    }

    /// Where the release packages put the models, relative to the directory holding the
    /// executable (packaging/*):
    /// - `models/` beside it: the Windows MSI and portable zip;
    /// - `../Resources/models`: the macOS app bundle (`Contents/MacOS` → `Contents/Resources`);
    /// - `../share/pdfcraft/models`: the FHS-style trees (`bin/` → `share/`) of the Linux
    ///   deb/rpm/tar.gz/AppImage/Flatpak and the FreeBSD tarball, wherever they are installed.
    pub fn dirs_beside_exe(exe_dir: &Path) -> Vec<PathBuf> {
        vec![exe_dir.join("models"), exe_dir.join("../Resources/models"), exe_dir.join("../share/pdfcraft/models")]
    }
}

/// A recognised word: its text and its box in image pixels `[left, top, right, bottom]`.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub rect: [f32; 4],
}

/// A recognised line of words, in reading order.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Line {
    pub words: Vec<Word>,
}

impl Line {
    pub fn text(&self) -> String {
        self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

/// A loaded recogniser. Loading takes a moment; keep one and reuse it.
pub struct Ocr {
    engine: ocrs::OcrEngine,
}

impl Ocr {
    pub fn load(models: &Models) -> Result<Ocr, OcrError> {
        Self::load_with_alphabet(models, ALPHABET)
    }

    pub fn load_with_lang(models: &Models, lang: &str) -> Result<Ocr, OcrError> {
        let is_thai = lang.eq_ignore_ascii_case("tha") || lang.starts_with("th");
        if is_thai {
            // Combined Thai alphabet
            let mut alpha = ALPHABET.to_string();
            for c in THAI_ALPHABET.chars() {
                if !alpha.contains(c) {
                    alpha.push(c);
                }
            }
            Self::load_with_alphabet(models, &alpha)
        } else {
            Self::load(models)
        }
    }

    pub fn load_with_alphabet(models: &Models, alphabet: &str) -> Result<Ocr, OcrError> {
        let load = |p: &Path| rten::Model::load_file(p).map_err(|e| OcrError::Load(p.display().to_string(), e.to_string()));
        let params = ocrs::OcrEngineParams {
            detection_model: Some(load(&models.detection)?),
            recognition_model: Some(load(&models.recognition)?),
            alphabet: Some(alphabet.into()),
            ..Default::default()
        };
        let engine = ocrs::OcrEngine::new(params).map_err(|e| OcrError::Load("ocr engine".into(), e.to_string()))?;
        Ok(Ocr { engine })
    }

    /// Load the models found by [`Models::find`].
    pub fn find() -> Result<Ocr, OcrError> {
        Self::load(&Models::find().ok_or(OcrError::NoModels)?)
    }

    /// Load the models found by [`Models::find_with_lang`].
    pub fn find_with_lang(lang: &str) -> Result<Ocr, OcrError> {
        let models = Models::find_with_lang(lang).ok_or(OcrError::NoModels)?;
        Self::load_with_lang(&models, lang)
    }

    /// Recognise the text in an RGBA (or RGB, or grey) image, `width` × `height` pixels.
    pub fn recognize(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Line>, OcrError> {
        if width == 0 || height == 0 || pixels.is_empty() {
            return Err(OcrError::EmptyImage);
        }
        // ocrs wants 1 or 3 channels.
        let rgb: std::borrow::Cow<[u8]> = if pixels.len() == (width * height * 4) as usize {
            pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect::<Vec<u8>>().into()
        } else {
            pixels.into()
        };
        let err = |e: &dyn std::fmt::Display| OcrError::Recognize(e.to_string());
        let source = ocrs::ImageSource::from_bytes(&rgb, (width, height)).map_err(|e| err(&e))?;
        let input = self.engine.prepare_input(source).map_err(|e| err(&e))?;
        let found = self.engine.detect_words(&input).map_err(|e| err(&e))?;
        let lines = self.engine.find_text_lines(&input, &found);
        let read = self.engine.recognize_text(&input, &lines).map_err(|e| err(&e))?;
        use ocrs::TextItem;
        Ok(read
            .into_iter()
            .flatten()
            .map(|line| Line {
                words: line
                    .words()
                    .map(|w| {
                        let r = w.bounding_rect();
                        Word { text: w.to_string(), rect: [r.left() as f32, r.top() as f32, r.right() as f32, r.bottom() as f32] }
                    })
                    .filter(|w| !w.text.trim().is_empty())
                    .collect(),
            })
            .filter(|l| !l.words.is_empty())
            .collect())
    }
}

/// A word placed on a page, in PDF user space: the bottom-left corner of its box, and the
/// vectors along its bottom edge (`across`, left → right as read) and its left edge (`up`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedWord {
    pub text: String,
    pub origin: [f64; 2],
    pub across: [f64; 2],
    pub up: [f64; 2],
}

impl PlacedWord {
    /// Place a pixel-space word with `to_user`, which maps an image pixel (x right, y down) to
    /// user space.
    pub fn place(word: &Word, to_user: impl Fn(f32, f32) -> [f64; 2]) -> PlacedWord {
        let [l, t, r, b] = word.rect;
        let o = to_user(l, b);
        let br = to_user(r, b);
        let tl = to_user(l, t);
        PlacedWord { text: word.text.clone(), origin: o, across: [br[0] - o[0], br[1] - o[1]], up: [tl[0] - o[0], tl[1] - o[1]] }
    }
}

/// Helvetica's ascender and descender (em fractions): a word box spans about this much.
const BOX_EM: f64 = 0.93;
const DESCENT: f64 = 0.21;
/// Helvetica's cap height and x-height (em fractions).
const CAP_HEIGHT: f64 = 0.718;
const X_HEIGHT: f64 = 0.523;

/// How far `text`'s ink reaches above and below the baseline, in em fractions: a word box is
/// tight around the ink, so "DT" spans the cap height only, "sun" the x-height, and "gap" reaches
/// down to the descender. Text without letters or digits keeps the full ascender-descender span.
fn ink_extent(text: &str) -> (f64, f64) {
    if !text.chars().any(char::is_alphanumeric) {
        return (BOX_EM - DESCENT, -DESCENT);
    }
    let tall = text
        .chars()
        .any(|c| c.is_uppercase() || c.is_ascii_digit() || "bdfhijklt!?/\\()[]{}|'\"#$%&@".contains(c) || (c.is_alphabetic() && !c.is_ascii()));
    let low = text.chars().any(|c| "gjpqy()[]{}|_".contains(c));
    (if tall { CAP_HEIGHT } else { X_HEIGHT }, if low { -DESCENT } else { 0.0 })
}

fn num(v: f64) -> String {
    let s = format!("{:.4}", if v.abs() < 1e-9 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

use pdfcraft_cos::{Dict, Document, Object};

type OwnedClusterKey = (pdfcraft_fonts::ShapedScript, String, Vec<(u32, u64, u64)>, u64);

fn owned_cluster_key(c: &pdfcraft_fonts::ShapedCluster) -> OwnedClusterKey {
    (c.script, c.text.clone(), c.glyphs.iter().map(|(id, [x, y])| (*id, x.to_bits(), y.to_bits())).collect(), c.advance.to_bits())
}

/// Whether `c` is a Thai combining character (vowel mark, tone mark, diacritic).
pub fn is_thai_combining(c: char) -> bool {
    matches!(u32::from(c), 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E)
}

/// Whether `c` is a Thai upper/lower vowel mark (sara i, ii, ue, uee, u, uu, phinthu, etc.).
fn is_thai_vowel_mark(c: char) -> bool {
    matches!(u32::from(c), 0x0E31 | 0x0E34..=0x0E3A)
}

/// Whether `c` is a Thai tone mark or diacritic (mai ek, tho, tri, chattawa, thanthakhat, etc.).
fn is_thai_tone_or_diacritic(c: char) -> bool {
    matches!(u32::from(c), 0x0E47..=0x0E4E)
}

/// Normalize Thai text for OCR and search:
/// 1. Reorders tone marks and thanthakhat that appear before vowel marks (e.g., `ก + ่ + ิ` -> `ก + ิ + ่`),
///    ensuring canonical orthographical Unicode ordering.
/// 2. Decomposes sara am (`ำ`, U+0E33) if preceded by a tone mark to standard canonical order (`ัํ` + tone + `า`).
/// 3. Reattaches or preserves combining marks properly so HarfBuzz shaper and ToUnicode produce correct clusters.
pub fn normalize_thai_text(text: &str) -> String {
    if !text.chars().any(pdfcraft_fonts::is_thai) {
        return text.to_string();
    }

    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        if is_thai_tone_or_diacritic(c) {
            // Check if immediately preceding character was a vowel mark that should precede this tone mark (normal).
            // But if a vowel mark comes *after* this tone mark, we will swap when that vowel arrives.
            out.push(c);
        } else if is_thai_vowel_mark(c) {
            // If previous char in `out` is a tone mark or diacritic, swap them so vowel comes before tone mark!
            if let Some(&prev) = out.last()
                && is_thai_tone_or_diacritic(prev)
            {
                let tone = out.pop().unwrap_or(prev);
                out.push(c);
                out.push(tone);
                continue;
            }
            out.push(c);
        } else {
            out.push(c);
        }
    }

    out.into_iter().collect()
}

/// Generate text layer content and optional font resources for words.
/// For words containing Thai characters, shapes them and creates Type 3 font resources with ToUnicode mapping.
/// For Latin words, uses `/PCHelv`.
pub fn text_layer_with_resources(words: &[PlacedWord], doc: &mut Document) -> (Vec<u8>, Option<Dict>) {
    // Normalize Thai words first to ensure canonical combining mark ordering (vowel before tone).
    let normalized_words: Vec<PlacedWord> = words
        .iter()
        .map(|w| {
            if w.text.chars().any(pdfcraft_fonts::is_thai) {
                let norm = normalize_thai_text(&w.text);
                PlacedWord { text: norm, origin: w.origin, across: w.across, up: w.up }
            } else {
                w.clone()
            }
        })
        .collect();
    let words = &normalized_words;

    let has_thai = words.iter().any(|w| w.text.chars().any(pdfcraft_fonts::is_thai));
    if !has_thai {
        return (text_layer(words), None);
    }

    let mut unique_clusters = Vec::new();
    let mut seen_keys = std::collections::HashSet::new();
    for w in words {
        if w.text.chars().any(pdfcraft_fonts::is_thai)
            && let Ok(clusters) = pdfcraft_fonts::shape_thai(&w.text)
        {
            for c in clusters {
                let key = owned_cluster_key(&c);
                if seen_keys.insert(key.clone()) {
                    unique_clusters.push((key, c));
                }
            }
        }
    }

    let mut t3_fonts = Vec::new();
    let mut cluster_font_map = std::collections::HashMap::new();
    for (chunk_idx, chunk) in unique_clusters.chunks(240).enumerate() {
        let chunk_refs: Vec<&pdfcraft_fonts::ShapedCluster> = chunk.iter().map(|(_, c)| c).collect();
        if let Ok(t3) = pdfcraft_fonts::type3_for_clusters(doc, &chunk_refs) {
            let font_name = if chunk_idx == 0 { "T3Thai".to_string() } else { format!("T3Thai{chunk_idx}") };
            t3_fonts.push((t3, font_name));
            for (local_idx, (key, _)) in chunk.iter().enumerate() {
                cluster_font_map.insert(key.clone(), (chunk_idx, local_idx));
            }
        }
    }

    let mut font_dict = Dict::new();
    for (t3_ref, font_name) in &t3_fonts {
        font_dict.set(font_name.as_bytes().to_vec(), Object::Ref(*t3_ref));
    }

    let mut out = b"/OCR BMC\nq\nBT\n3 Tr\n/PCHelv 1 Tf\n".to_vec();
    for w in words {
        let height = w.up[0].hypot(w.up[1]);
        if height <= 0.0 {
            continue;
        }

        if w.text.chars().any(pdfcraft_fonts::is_thai) && !t3_fonts.is_empty() {
            if let Ok(clusters) = pdfcraft_fonts::shape_thai(&w.text) {
                let total_adv: f64 = clusters.iter().map(|c| c.advance).sum();
                if total_adv <= 0.0 {
                    continue;
                }
                let across_len = w.across[0].hypot(w.across[1]);
                if across_len <= 0.0 {
                    continue;
                }
                let scale_x = w.across[0] / total_adv;
                let scale_y = w.across[1] / total_adv;
                let up_x = w.up[0] / BOX_EM;
                let up_y = w.up[1] / BOX_EM;
                let m = [scale_x, scale_y, up_x, up_y, w.origin[0], w.origin[1]];
                let nums: Vec<String> = m.iter().map(|v| num(*v)).collect();
                out.extend_from_slice(nums.join(" ").as_bytes());
                out.extend_from_slice(b" Tm\n");

                let mut current_font_idx: Option<usize> = None;
                for c in &clusters {
                    let key = owned_cluster_key(c);
                    if let Some(&(chunk_idx, local_idx)) = cluster_font_map.get(&key) {
                        if current_font_idx != Some(chunk_idx) {
                            let font_res = &t3_fonts[chunk_idx].1;
                            out.extend(format!("/{} 1 Tf ", font_res).bytes());
                            current_font_idx = Some(chunk_idx);
                        }
                        out.extend(format!("<{:02X}> Tj\n", local_idx + 1).bytes());
                    }
                }
                out.extend_from_slice(b"/PCHelv 1 Tf\n");
            }
        } else {
            let width = helvetica_width(&w.text, 1.0);
            if width <= 0.0 {
                continue;
            }
            let (top, bottom) = ink_extent(&w.text);
            let em = [w.up[0] / (top - bottom), w.up[1] / (top - bottom)];
            let ax = [w.across[0] / width, w.across[1] / width];
            let o = [w.origin[0] - em[0] * bottom, w.origin[1] - em[1] * bottom];
            let m = [ax[0], ax[1], em[0], em[1], o[0], o[1]];
            let nums: Vec<String> = m.iter().map(|v| num(*v)).collect();
            out.extend_from_slice(nums.join(" ").as_bytes());
            out.extend_from_slice(b" Tm ");
            out.extend(pdfcraft_fonts::literal(&pdfcraft_fonts::win_ansi(&w.text)));
            out.extend_from_slice(b" Tj\n");
        }
    }
    out.extend_from_slice(b"ET\nQ\nEMC\n");
    (out, if font_dict.is_empty() { None } else { Some(font_dict) })
}

/// Page content that writes `words` as invisible text in `/PCHelv` (standard Helvetica,
/// WinAnsiEncoding), each word stretched to its box so selection and search highlight the
/// right place. Marked content `/OCR` so it can be told apart from the page's own text.
pub fn text_layer(words: &[PlacedWord]) -> Vec<u8> {
    // `3 Tr` and the font are graphics state, which `ET` doesn't reset: without `q … Q` they
    // would stay set for every later content stream on the page, so text added after OCR (a
    // header or footer) drew invisibly (#3029).
    let mut out = b"/OCR BMC\nq\nBT\n3 Tr\n/PCHelv 1 Tf\n".to_vec();
    for w in words {
        let width = helvetica_width(&w.text, 1.0);
        let height = w.up[0].hypot(w.up[1]);
        if width <= 0.0 || height <= 0.0 {
            continue;
        }
        // The em square: the word's own ink (cap height or x-height, descenders or not) spans the
        // box, so the text is as tall as the scanned word; its width is stretched to fit the word.
        let (top, bottom) = ink_extent(&w.text);
        let em = [w.up[0] / (top - bottom), w.up[1] / (top - bottom)];
        let ax = [w.across[0] / width, w.across[1] / width];
        let o = [w.origin[0] - em[0] * bottom, w.origin[1] - em[1] * bottom];
        let m = [ax[0], ax[1], em[0], em[1], o[0], o[1]];
        let nums: Vec<String> = m.iter().map(|v| num(*v)).collect();
        out.extend_from_slice(nums.join(" ").as_bytes());
        out.extend_from_slice(b" Tm ");
        out.extend(pdfcraft_fonts::literal(&pdfcraft_fonts::win_ansi(&w.text)));
        out.extend_from_slice(b" Tj\n");
    }
    out.extend_from_slice(b"ET\nQ\nEMC\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_words_through_the_mapping() {
        // 2 pixels per point, page 612 × 792, y flipped.
        let to_user = |x: f32, y: f32| [x as f64 / 2.0, 792.0 - y as f64 / 2.0];
        let w = PlacedWord::place(&Word { text: "Hello".into(), rect: [100.0, 200.0, 300.0, 240.0] }, to_user);
        assert_eq!(w.origin, [50.0, 672.0]);
        assert_eq!(w.across, [100.0, 0.0]);
        assert_eq!(w.up, [0.0, 20.0]);
    }

    /// The text layer is Helvetica in WinAnsiEncoding: every character the model can recognise
    /// must be one it can hold, or recognised text would be saved as `?`. A model with a wider
    /// alphabet (Chinese, for example) needs a text layer in another font first.
    #[test]
    fn the_model_alphabet_fits_the_text_layer_font() {
        assert_eq!(pdfcraft_fonts::first_undrawable(ALPHABET), None);
    }

    #[test]
    fn text_layer_is_invisible_text_fitted_to_each_box() {
        let w = PlacedWord { text: "Hi".into(), origin: [10.0, 20.0], across: [30.0, 0.0], up: [0.0, 9.3] };
        let s = String::from_utf8(text_layer(&[w])).unwrap();
        assert!(s.contains("3 Tr") && s.contains("/PCHelv 1 Tf"), "{s}");
        let width = helvetica_width("Hi", 1.0);
        // "Hi" has no descender: its box spans the cap height, from the baseline up.
        assert!(s.contains(&format!("{} 0 0 {} 10 20 Tm (Hi) Tj", num(30.0 / width), num(9.3 / CAP_HEIGHT))), "{s}");
        assert!(s.starts_with("/OCR BMC") && s.ends_with("EMC\n"));
    }

    /// #3029: the layer's invisible rendering mode and font end with the layer. Every operator
    /// that sets them sits inside a `q … Q` pair that the layer closes itself, so content drawn
    /// after it on the page starts from the graphics state it had before.
    #[test]
    fn the_layer_restores_the_graphics_state_it_changes() {
        let w = PlacedWord { text: "Hi".into(), origin: [10.0, 20.0], across: [30.0, 0.0], up: [0.0, 9.3] };
        let s = String::from_utf8(text_layer(&[w])).unwrap();
        let mut depth = 0_i32;
        for token in s.split_ascii_whitespace() {
            match token {
                "q" => depth += 1,
                "Q" => depth -= 1,
                "Tr" | "Tf" | "Tm" => assert!(depth > 0, "`{token}` outside q … Q in {s:?}"),
                _ => {}
            }
            assert!(depth >= 0, "unbalanced Q in {s:?}");
        }
        assert_eq!(depth, 0, "the layer leaves a q open: {s:?}");
    }

    /// A word box is tight around the ink, so the text's size and baseline come from the letters:
    /// capitals, lowercase without ascenders, descenders.
    #[test]
    fn the_text_is_as_tall_as_the_scanned_word() {
        assert_eq!(ink_extent("DT"), (CAP_HEIGHT, 0.0));
        assert_eq!(ink_extent("402337"), (CAP_HEIGHT, 0.0));
        assert_eq!(ink_extent("sun"), (X_HEIGHT, 0.0));
        assert_eq!(ink_extent("gap"), (X_HEIGHT, -DESCENT));
        assert_eq!(ink_extent("Gallus.Ze@camtel.cm"), (CAP_HEIGHT, 0.0));
        assert_eq!(ink_extent("Corporation"), (CAP_HEIGHT, -DESCENT));
        assert_eq!(ink_extent("--"), (BOX_EM - DESCENT, -DESCENT));
        let w = PlacedWord { text: "gap".into(), origin: [0.0, 100.0], across: [20.0, 0.0], up: [0.0, 7.3] };
        let s = String::from_utf8(text_layer(&[w])).unwrap();
        let em = 7.3 / (X_HEIGHT + DESCENT);
        assert!(s.contains(&format!(" 0 0 {} 0 {} Tm (gap) Tj", num(em), num(100.0 + em * DESCENT))), "{s}");
    }

    #[test]
    fn rotated_pages_keep_the_reading_direction() {
        // A page turned 90°: image x runs up the page.
        let to_user = |x: f32, y: f32| [y as f64, x as f64];
        let w = PlacedWord::place(&Word { text: "Up".into(), rect: [0.0, 0.0, 50.0, 10.0] }, to_user);
        assert_eq!(w.across, [0.0, 50.0]);
        assert_eq!(w.up, [-10.0, 0.0]);
    }

    /// The current ONNX pair wins when an installation also retains both legacy models.
    #[test]
    fn model_discovery_prefers_the_complete_onnx_pair() {
        let dir = tempfile::tempdir().unwrap();
        for file in ["text-detection.onnx", "text-recognition.onnx"] {
            std::fs::write(dir.path().join(file), b"model").unwrap();
        }
        let expected = Some(Models { detection: dir.path().join("text-detection.onnx"), recognition: dir.path().join("text-recognition.onnx") });
        assert_eq!(Models::in_dir(dir.path()), expected, "a complete ONNX pair is sufficient");
        for file in ["text-detection.rten", "text-recognition.rten"] {
            std::fs::write(dir.path().join(file), b"model").unwrap();
        }
        assert_eq!(Models::in_dir(dir.path()), expected, "the complete ONNX pair takes precedence over legacy models");
    }

    /// An interrupted ONNX update still permits an intact legacy pair in the same directory.
    #[test]
    fn model_discovery_falls_back_to_legacy_when_the_onnx_pair_is_incomplete() {
        for current in ["text-detection.onnx", "text-recognition.onnx"] {
            let dir = tempfile::tempdir().unwrap();
            for file in [current, "text-detection.rten", "text-recognition.rten"] {
                std::fs::write(dir.path().join(file), b"model").unwrap();
            }
            assert_eq!(
                Models::in_dir(dir.path()),
                Some(Models { detection: dir.path().join("text-detection.rten"), recognition: dir.path().join("text-recognition.rten") }),
                "{current} alone must not hide a complete legacy pair"
            );
        }
    }

    /// Detection and recognition must come from one complete format pair, never a mixed update.
    #[test]
    fn model_discovery_refuses_mixed_format_pairs() {
        for pair in [["text-detection.onnx", "text-recognition.rten"], ["text-detection.rten", "text-recognition.onnx"]] {
            let dir = tempfile::tempdir().unwrap();
            for file in pair {
                std::fs::write(dir.path().join(file), b"model").unwrap();
            }
            assert_eq!(Models::in_dir(dir.path()), None, "{pair:?} is not a complete model pair");
        }
    }

    /// Existing source builds and PDFCRAFT_MODELS overrides retain their complete RTEN pair.
    #[test]
    fn model_discovery_keeps_legacy_only_installations() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Models::in_dir(dir.path()), None);
        std::fs::write(dir.path().join("text-detection.rten"), b"model").unwrap();
        assert_eq!(Models::in_dir(dir.path()), None, "one legacy model is incomplete");
        std::fs::write(dir.path().join("text-recognition.rten"), b"model").unwrap();
        assert_eq!(
            Models::in_dir(dir.path()),
            Some(Models { detection: dir.path().join("text-detection.rten"), recognition: dir.path().join("text-recognition.rten") })
        );
    }

    /// #103: every release package's layout puts the models where the installed app looks.
    /// Mirrors packaging/{windows/package.ps1,macos/package.sh,linux/package.sh,freebsd/package.sh}.
    #[test]
    fn finds_the_models_in_every_release_package_layout() {
        let base = std::env::temp_dir().join(format!("pdfcraft-ocr-layouts-{}", std::process::id()));
        let layouts = [
            ("windows", "PdfCraft", "PdfCraft/models"),
            ("macos", "PdfCraft.app/Contents/MacOS", "PdfCraft.app/Contents/Resources/models"),
            ("linux-deb-rpm", "usr/bin", "usr/share/pdfcraft/models"),
            ("linux-tar", "pdfcraft-1.0.0-linux-x86_64/bin", "pdfcraft-1.0.0-linux-x86_64/share/pdfcraft/models"),
            ("appimage", "PdfCraft.AppDir/usr/bin", "PdfCraft.AppDir/usr/share/pdfcraft/models"),
            ("flatpak", "app/bin", "app/share/pdfcraft/models"),
            ("freebsd", "usr/local/bin", "usr/local/share/pdfcraft/models"),
        ];
        for (name, exe_dir, models_dir) in layouts {
            let root = base.join(name);
            let (exe_dir, models_dir) = (root.join(exe_dir), root.join(models_dir));
            std::fs::create_dir_all(&exe_dir).unwrap();
            std::fs::create_dir_all(&models_dir).unwrap();
            let found = || Models::dirs_beside_exe(&exe_dir).iter().find_map(|d| Models::in_dir(d));
            assert_eq!(found(), None, "{name}: no models yet");
            for file in [DETECTION_MODEL, RECOGNITION_MODEL] {
                std::fs::write(models_dir.join(file), b"model").unwrap();
            }
            let models = found().unwrap_or_else(|| panic!("{name}: models in {} not found", models_dir.display()));
            assert_eq!(models.detection.canonicalize().unwrap(), models_dir.join(DETECTION_MODEL).canonicalize().unwrap(), "{name}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The message tells end users what to do; release builds have no cargo.
    #[test]
    fn the_alphabet_is_the_trained_one() {
        let chars: Vec<char> = ALPHABET.chars().collect();
        assert_eq!(chars.len(), 96);
        // The euro sign follows the ASCII punctuation (class 44; 0 is the CTC blank), not a second E.
        assert_eq!(chars.iter().position(|&c| c == '€'), Some(43));
        assert_eq!(chars.iter().filter(|&&c| c == 'E').count(), 1);
    }

    #[test]
    fn missing_models_message_is_for_end_users() {
        let message = OcrError::NoModels.to_string();
        assert!(!message.contains("cargo") && message.contains("PDFCRAFT_MODELS"), "{message}");
    }

    /// End to end on rendered text, when the models are installed (`cargo xtask models`).
    #[test]
    fn reads_rendered_text() {
        let Some(models) = Models::find() else {
            eprintln!("skipped: OCR models not installed");
            return;
        };
        let ocr = Ocr::load(&models).unwrap();
        let (w, h, px) = test_image::hello();
        let lines = ocr.recognize(&px, w, h).unwrap();
        let text: Vec<String> = lines.iter().map(Line::text).collect();
        assert!(text.iter().any(|t| t.contains("HELL") && t.contains("WOR")), "{text:?}");
        let word = lines.iter().flat_map(|l| &l.words).find(|w| w.text.contains("HELL")).unwrap();
        assert!(word.rect[0] >= 10.0 && word.rect[0] < 60.0, "{word:?}");
    }

    #[test]
    fn normalize_thai_text_reorders_reversed_marks() {
        // Reversed vowel and tone: ย + ่ (U+0E48) + ู (U+0E39) -> ย + ู + ่ ("ยู่")
        let reversed_yu = "ย\u{0E48}\u{0E39}";
        assert_eq!(normalize_thai_text(reversed_yu), "ยู่");

        // Reversed vowel and tone: ม + ่ (U+0E48) + ู (U+0E39) -> ม + ู + ่ ("มู่")
        let reversed_mu = "ม\u{0E48}\u{0E39}";
        assert_eq!(normalize_thai_text(reversed_mu), "มู่");

        // Normal order remains unchanged
        assert_eq!(normalize_thai_text("อยู่บ้าน"), "อยู่บ้าน");
        assert_eq!(normalize_thai_text("หมู่ที่"), "หมู่ที่");
        assert_eq!(normalize_thai_text("ไปรษณีย์"), "ไปรษณีย์");
        assert_eq!(normalize_thai_text("หมายเลขโทรศัพท์"), "หมายเลขโทรศัพท์");
        assert_eq!(normalize_thai_text("English text 123"), "English text 123");
    }

    #[test]
    fn text_layer_with_resources_handles_thai() {
        let mut doc = Document::new_empty();
        let words = [PlacedWord {
            text: "ภาษาไทย".into(),
            origin: [100.0, 200.0],
            across: [80.0, 0.0],
            up: [0.0, 15.0],
        }];
        let (content, fonts) = text_layer_with_resources(&words, &mut doc);
        let s = String::from_utf8(content).unwrap();
        assert!(s.contains("3 Tr"));
        if pdfcraft_fonts::document_thai_font().is_some() {
            assert!(s.contains("/T3Thai 1 Tf"), "{s}");
            assert!(fonts.is_some(), "Should produce font dictionary for Thai");
            let f = fonts.unwrap();
            assert!(f.get(b"T3Thai").is_some());
        }
    }

    mod test_image {
        /// "HELLO WORLD" drawn with blocky 5×7 letters, 4 pixels per dot, black on white.
        pub fn hello() -> (u32, u32, Vec<u8>) {
            const GLYPHS: &[(char, [&str; 7])] = &[
                ('H', ["X...X", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X"]),
                ('E', ["XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"]),
                ('L', ["X....", "X....", "X....", "X....", "X....", "X....", "XXXXX"]),
                ('O', [".XXX.", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX."]),
                ('W', ["X...X", "X...X", "X...X", "X.X.X", "X.X.X", "XX.XX", "X...X"]),
                ('R', ["XXXX.", "X...X", "X...X", "XXXX.", "X.X..", "X..X.", "X...X"]),
                ('D', ["XXXX.", "X...X", "X...X", "X...X", "X...X", "X...X", "XXXX."]),
            ];
            let text = "HELLO WORLD";
            let (dot, x0, y0) = (4u32, 40u32, 40u32);
            let (w, h) = (x0 * 2 + text.len() as u32 * 6 * dot, y0 * 2 + 7 * dot);
            let mut px = vec![255u8; (w * h * 4) as usize];
            for (i, c) in text.chars().enumerate() {
                let Some((_, rows)) = GLYPHS.iter().find(|g| g.0 == c) else { continue };
                for (ry, row) in rows.iter().enumerate() {
                    for (rx, b) in row.bytes().enumerate() {
                        if b != b'X' {
                            continue;
                        }
                        for dy in 0..dot {
                            for dx in 0..dot {
                                let x = x0 + (i as u32 * 6 + rx as u32) * dot + dx;
                                let y = y0 + ry as u32 * dot + dy;
                                let o = ((y * w + x) * 4) as usize;
                                px[o..o + 3].copy_from_slice(&[0, 0, 0]);
                            }
                        }
                    }
                }
            }
            (w, h, px)
        }
    }
}
