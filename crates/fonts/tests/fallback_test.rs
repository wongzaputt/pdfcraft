use pdfcraft_fonts::INTER_REGULAR;
use pdfcraft_fonts::opentype::{CustomFontSource, FontFallbackChain, shape_text};
use std::sync::Arc;

#[test]
fn test_mixed_latin_and_non_latin_partitioning() {
    let inter_data = Arc::new(INTER_REGULAR.to_vec());
    let primary = CustomFontSource::new("Inter", Arc::clone(&inter_data), 0);

    // If craft-fonts faces are available, use Thai or CJK as fallbacks
    let mut fallbacks = Vec::new();
    if let Some(thai) = pdfcraft_fonts::document_thai_font() {
        fallbacks.push(CustomFontSource::new("ThaiFallback", Arc::new(thai.bytes.to_vec()), 0));
    }
    if let Some(cjk) = pdfcraft_fonts::document_japanese_font() {
        fallbacks.push(CustomFontSource::new("CJKFallback", Arc::new(cjk.bytes.to_vec()), 0));
    }

    let chain = FontFallbackChain::new(primary, fallbacks);

    // Case A: purely Latin text should stay in one primary run
    let runs = chain.partition_text("Hello World");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].font.name, "Inter");
    assert_eq!(runs[0].text, "Hello World");

    // Case B: mixed Latin + Thai (if Thai font is available)
    if chain.fallbacks.iter().any(|f| f.name == "ThaiFallback") {
        let mixed = "Hello สวัสดี";
        let runs = chain.partition_text(mixed);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].font.name, "Inter");
        assert_eq!(runs[0].text, "Hello ");
        assert_eq!(runs[1].font.name, "ThaiFallback");
        assert_eq!(runs[1].text, "สวัสดี");

        // Shape each partitioned run with its respective resolved font
        for run in &runs {
            let shaped = shape_text(&run.text, &run.font.data, 12.0, None, None).expect("shaping partitioned run");
            assert!(!shaped.glyphs.is_empty());
        }
    }
}

#[test]
fn test_fallback_exhaustion() {
    let inter_data = Arc::new(INTER_REGULAR.to_vec());
    let primary = CustomFontSource::new("Inter", inter_data, 0);

    // Fallback chain with only Latin font
    let chain = FontFallbackChain::new(primary, Vec::new());

    // Characters missing from Inter (e.g. Rare hieroglyphs or unassigned CJK / Thai)
    // should fall back to primary rather than panicking or dropping
    let text = "Hello 𓀀 World";
    let runs = chain.partition_text(text);
    // When no fallback covers it, it maps to primary
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].font.name, "Inter");
    assert_eq!(runs[0].text, text);

    let shaped = shape_text(&runs[0].text, &runs[0].font.data, 12.0, None, None).expect("shaping succeeds even with missing glyphs");
    assert!(!shaped.glyphs.is_empty());
}
