use pdfcraft_fonts::INTER_REGULAR;
use pdfcraft_fonts::opentype::{has_opentype_layout, shape_text};

#[test]
fn test_latin_extended_ligatures() {
    let text = "difficult official floating";
    let size = 16.0;

    // Check with standard fonts supporting 'liga' (Calibri, Arial, Tahoma) or Inter
    let candidate_paths = [
        r"C:\Windows\Fonts\calibri.ttf",
        r"C:\Windows\Fonts\arial.ttf",
        r"C:\Windows\Fonts\tahoma.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];

    let mut font_bytes = None;
    for path in candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            font_bytes = Some(bytes);
            break;
        }
    }

    let bytes = font_bytes.as_deref().unwrap_or(INTER_REGULAR);
    let run = shape_text(text, bytes, size, None, None).expect("shaping succeeds");

    assert!(!run.glyphs.is_empty());
    assert!(run.total_advance_width > 0.0);
    // If standard ligatures (ffi, fl) are active, glyph count is less than char count
    if bytes != INTER_REGULAR {
        assert!(run.glyphs.len() < text.chars().count());
    }
}

#[test]
fn test_vietnamese_stacked_diacritics() {
    let text = "Tiếng Việt";
    let size = 18.0;
    let run = shape_text(text, INTER_REGULAR, size, None, None).expect("vietnamese shaping succeeds");

    assert!(!run.glyphs.is_empty());
    assert!(run.total_advance_width > 0.0);
    // Inter covers Vietnamese composite/stacked diacritics
    for glyph in &run.glyphs {
        assert!(glyph.glyph_id > 0, "glyph should be resolved");
    }
}

#[test]
fn test_hebrew_shaping() {
    // Standard Hebrew text with Niqqud diacritical pointing
    let text = "שָׁלוֹם";
    let size = 18.0;

    // Check system fonts or craft fonts for Hebrew support
    let candidate_paths = [r"C:\Windows\Fonts\tahoma.ttf", r"C:\Windows\Fonts\arial.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"];

    let mut font_bytes = None;
    for path in candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            font_bytes = Some(bytes);
            break;
        }
    }

    if let Some(bytes) = font_bytes {
        let script = rustybuzz::Script::from_iso15924_tag(rustybuzz::ttf_parser::Tag::from_bytes(b"Hebr"));
        let run = shape_text(text, &bytes, size, script, Some(rustybuzz::Direction::RightToLeft)).expect("hebrew shaping succeeds");

        assert!(!run.glyphs.is_empty());
        assert!(run.total_advance_width > 0.0);
        // Base consonants carry advance width, whereas Niqqud marks typically have zero or minimal advance
        let has_zero_advance_mark = run.glyphs.iter().any(|g| g.x_advance.abs() < 0.01);
        let has_positive_advance = run.glyphs.iter().any(|g| g.x_advance > 0.0);
        assert!(has_positive_advance);
        assert!(has_zero_advance_mark);
    }
}

#[test]
fn test_hindi_devanagari_shaping() {
    // Hindi consonant conjuncts and matra reordering:
    // "नमस्ते" contains half-form 'स्' + 'ते' conjunct ligature
    // "हिंदी" contains Anusvara nasal mark and ii-matra
    let text_namaste = "नमस्ते";
    let text_hindi = "हिंदी";
    let size = 18.0;

    let candidate_paths = [
        r"C:\Windows\Fonts\Nirmala.ttc",
        r"C:\Windows\Fonts\Nirmala.ttf",
        r"C:\Windows\Fonts\mangal.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf",
        "/usr/share/fonts/truetype/lohit-devanagari/Lohit-Devanagari.ttf",
    ];

    let mut font_data = None;
    for path in candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            if bytes.starts_with(b"ttcf") {
                // If it's a TTC, extract font 0
                if let Some(ttf) = extract_ttf_from_ttc(&bytes, 0) {
                    font_data = Some(ttf);
                    break;
                }
            } else {
                font_data = Some(bytes);
                break;
            }
        }
    }

    if let Some(bytes) = font_data {
        let script = rustybuzz::Script::from_iso15924_tag(rustybuzz::ttf_parser::Tag::from_bytes(b"Deva"));
        let run_namaste = shape_text(text_namaste, &bytes, size, script, Some(rustybuzz::Direction::LeftToRight)).expect("namaste shaping succeeds");
        let run_hindi = shape_text(text_hindi, &bytes, size, script, Some(rustybuzz::Direction::LeftToRight)).expect("hindi shaping succeeds");

        assert!(!run_namaste.glyphs.is_empty());
        assert!(run_namaste.total_advance_width > 0.0);
        assert!(!run_hindi.glyphs.is_empty());
        assert!(run_hindi.total_advance_width > 0.0);

        // Devanagari conjunct shaping forms ligatures or half forms
        for g in &run_namaste.glyphs {
            assert!(g.glyph_id > 0, "Devanagari glyph ID must be valid");
        }
    }
}

/// Helper to extract a standalone TrueType font from a TTC (TrueType Collection).
fn extract_ttf_from_ttc(ttc_data: &[u8], font_index: usize) -> Option<Vec<u8>> {
    if !ttc_data.starts_with(b"ttcf") || ttc_data.len() < 12 {
        return None;
    }
    let num_fonts = u32::from_be_bytes(ttc_data.get(8..12)?.try_into().ok()?) as usize;
    if font_index >= num_fonts {
        return None;
    }
    let off_pos = 12 + font_index * 4;
    let offset = u32::from_be_bytes(ttc_data.get(off_pos..off_pos + 4)?.try_into().ok()?) as usize;
    let sfnt_version = ttc_data.get(offset..offset + 4)?;
    let num_tables = u16::from_be_bytes(ttc_data.get(offset + 4..offset + 6)?.try_into().ok()?) as usize;

    let mut table_records = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let rec_off = offset + 12 + i * 16;
        let tag = ttc_data.get(rec_off..rec_off + 4)?;
        let checksum = ttc_data.get(rec_off + 4..rec_off + 8)?;
        let tab_off = u32::from_be_bytes(ttc_data.get(rec_off + 8..rec_off + 12)?.try_into().ok()?) as usize;
        let tab_len = u32::from_be_bytes(ttc_data.get(rec_off + 12..rec_off + 16)?.try_into().ok()?) as usize;
        table_records.push((tag, checksum, tab_off, tab_len));
    }

    let mut current_off = 12 + num_tables * 16;
    let mut new_records = Vec::with_capacity(num_tables);
    let mut data_blocks = Vec::with_capacity(num_tables);
    for (tag, checksum, tab_off, tab_len) in table_records {
        let tab_data = ttc_data.get(tab_off..tab_off + tab_len)?;
        let pad = (4 - (tab_data.len() % 4)) % 4;
        new_records.push((tag, checksum, current_off as u32, tab_len as u32));
        data_blocks.push((tab_data, pad));
        current_off += tab_data.len() + pad;
    }

    let mut out = Vec::with_capacity(current_off);
    out.extend_from_slice(sfnt_version);
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    let entry_selector = (usize::BITS - 1 - num_tables.leading_zeros()) as u16;
    let search_range = (1 << entry_selector) * 16;
    let range_shift = (num_tables as u16) * 16 - search_range;
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&range_shift.to_be_bytes());

    for (tag, checksum, tab_off, tab_len) in new_records {
        out.extend_from_slice(tag);
        out.extend_from_slice(checksum);
        out.extend_from_slice(&tab_off.to_be_bytes());
        out.extend_from_slice(&tab_len.to_be_bytes());
    }
    for (block, pad) in data_blocks {
        out.extend_from_slice(block);
        out.extend(std::iter::repeat_n(0, pad));
    }
    Some(out)
}

#[test]
fn test_empty_text() {
    let run = shape_text("", INTER_REGULAR, 12.0, None, None).expect("empty succeeds");
    assert!(run.glyphs.is_empty());
    assert_eq!(run.total_advance_width, 0.0);
}

#[test]
fn test_complex_thai_syllable() {
    // If craft-fonts thai font is available, test with it
    if let Some(thai_face) = pdfcraft_fonts::document_thai_font() {
        let text = "ที่น้ำ";
        let script = rustybuzz::Script::from_iso15924_tag(rustybuzz::ttf_parser::Tag::from_bytes(b"Thai"));
        let run = shape_text(text, thai_face.bytes, 14.0, script, Some(rustybuzz::Direction::LeftToRight)).expect("thai shaping succeeds");
        assert!(!run.glyphs.is_empty());
        assert!(run.total_advance_width > 0.0);
        // Ensure mark positioning / offsets were computed
        assert!(has_opentype_layout(thai_face.bytes));
    }
}

#[test]
fn test_complex_arabic_shaping() {
    if let Some(arabic_face) = pdfcraft_fonts::document_arabic_font() {
        let script = rustybuzz::Script::from_iso15924_tag(rustybuzz::ttf_parser::Tag::from_bytes(b"Arab"));
        let isolated = shape_text("ب", arabic_face.bytes, 14.0, script, Some(rustybuzz::Direction::RightToLeft)).expect("arabic isolated");
        let connected = shape_text("ببب", arabic_face.bytes, 14.0, script, Some(rustybuzz::Direction::RightToLeft)).expect("arabic connected");

        assert_eq!(isolated.glyphs.len(), 1);
        assert_eq!(connected.glyphs.len(), 3);
        // In Arabic contextual shaping, initial/medial/final glyph IDs differ from isolated
        assert_ne!(isolated.glyphs[0].glyph_id, connected.glyphs[0].glyph_id);
    }
}
