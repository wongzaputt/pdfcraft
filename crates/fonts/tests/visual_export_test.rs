use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::Write as IoWrite;
use std::path::Path;
use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, Object, Stream};
use pdfcraft_fonts::INTER_REGULAR;
use pdfcraft_fonts::embedding::embed_direct_truetype;
use pdfcraft_fonts::opentype::{CustomFontSource, FontFallbackChain, shape_text};
use skrifa::GlyphId;

fn find_system_fonts() -> Vec<(String, Vec<u8>)> {
    let mut fonts = Vec::new();

    // Check Devanagari (Hindi) fonts
    let deva_candidates = [
        ("Nirmala", r"C:\Windows\Fonts\Nirmala.ttc"),
        ("Nirmala", r"C:\Windows\Fonts\Nirmala.ttf"),
        ("Mangal", r"C:\Windows\Fonts\mangal.ttf"),
        ("NotoDevanagari", "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf"),
        ("LohitDevanagari", "/usr/share/fonts/truetype/lohit-devanagari/Lohit-Devanagari.ttf"),
    ];

    for (name, path_str) in deva_candidates {
        let p = Path::new(path_str);
        if let Ok(bytes) = fs::read(p) {
            if bytes.starts_with(b"ttcf") {
                if let Some(ttf) = extract_ttf_from_ttc(&bytes, 0) {
                    fonts.push((name.to_string(), ttf));
                    break;
                }
            } else {
                fonts.push((name.to_string(), bytes));
                break;
            }
        }
    }

    // Check Multi-script (Thai, Hebrew, Arabic) host fonts
    let general_candidates = [
        ("Tahoma", r"C:\Windows\Fonts\tahoma.ttf"),
        ("Leelawadee", r"C:\Windows\Fonts\leelawad.ttf"),
        ("Arial", r"C:\Windows\Fonts\arial.ttf"),
        ("Times", r"C:\Windows\Fonts\times.ttf"),
        ("DejaVuSans", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        ("NotoThai", "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf"),
        ("NotoArabic", "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf"),
    ];

    for (name, path_str) in general_candidates {
        let p = Path::new(path_str);
        if let Ok(bytes) = fs::read(p) {
            fonts.push((name.to_string(), bytes));
        }
    }

    fonts
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
fn test_visual_export_complex_scripts() {
    let host_fonts = find_system_fonts();
    if host_fonts.is_empty() {
        eprintln!("skipping visual_export_complex_scripts: no suitable host fonts found");
        return;
    }

    println!("Found {} host font(s) for visual export test.", host_fonts.len());

    // Build fallback chain: Inter (primary Latin) -> Host Fonts (Thai, Arabic, Hindi, Hebrew)
    let inter_source = CustomFontSource::new("Inter", Arc::new(INTER_REGULAR.to_vec()), 0);
    let fallback_sources: Vec<_> = host_fonts.into_iter().map(|(name, bytes)| CustomFontSource::new(name, Arc::new(bytes), 0)).collect();
    let chain = FontFallbackChain::new(inter_source, fallback_sources);

    let test_phrases = [
        "Thai: ที่นี่มีน้ำผึ้งและเสือป่า (Testing 2026)",
        "Arabic: مرحبا بك في عالم البرمجة",
        "Hindi: नमस्ते दुनिया (Welcome to the world)",
        "Vietnamese: Chào mừng đến với Tiếng Việt",
        "Hebrew: שָׁלוֹם עֲלֵיכֶם (Peace be upon you)",
        "Ligatures: The official profile offered difficult scientific workflows (fi, fl, ffi)",
    ];

    let mut doc = Document::new_empty();
    let mut fonts_res = Dict::new();

    let mut content_stream = Vec::new();
    let _ = writeln!(content_stream, "BT\n1 0 0 1 50 750 Tm");

    let mut y_offset = 720.0f64;

    for phrase in &test_phrases {
        let partitioned_runs = chain.partition_text(phrase);
        assert!(!partitioned_runs.is_empty(), "partitioned runs must not be empty");

        let mut x_pen = 50.0f64;

        for run in partitioned_runs {
            let font_data = &run.font.data;
            let font_size = 18.0f32;

            let shaped = shape_text(&run.text, font_data, font_size, None, None).expect("shaping succeeded");
            assert!(shaped.total_advance_width > 0.0, "shaped run should have positive advance width");

            // Build glyph mappings for Identity-H embed_mapping.
            // Clusters are byte offsets into `run.text`; a cluster shows the characters up to the next one.
            let mut starts: Vec<usize> = shaped.glyphs.iter().map(|g| (g.cluster as usize).min(run.text.len())).collect();
            starts.sort_unstable();
            starts.dedup();

            let mut mappings: Vec<(String, GlyphId)> = Vec::with_capacity(shaped.glyphs.len());
            for g in &shaped.glyphs {
                let start = (g.cluster as usize).min(run.text.len());
                let next_idx = starts.partition_point(|s| *s <= start);
                let end = starts.get(next_idx).copied().unwrap_or(run.text.len());
                let cluster_slice = run.text.get(start..end).unwrap_or("");
                let cluster_text = if cluster_slice.is_empty() { " ".to_string() } else { cluster_slice.to_string() };
                mappings.push((cluster_text, GlyphId::new(u32::from(g.glyph_id))));
            }

            let res_tag = match embed_direct_truetype(&mut doc, &mut fonts_res, font_data, &run.font.name, &mappings) {
                Ok(tag) => tag,
                Err(err) => {
                    eprintln!("embed_direct_truetype failed for {}: {err}", run.font.name);
                    continue;
                }
            };

            // Write font selection & positioning operators
            let _ = writeln!(content_stream, "/{res_tag} 18 Tf");
            let _ = writeln!(content_stream, "1 0 0 1 {x_pen:.3} {y_offset:.3} Tm");

            // Emit TJ with native TrueType GIDs (<044A>, ...) and GPOS offsets
            let mut tj_array = String::from("[");
            for glyph in &shaped.glyphs {
                let gid = glyph.glyph_id;
                // If there's an x_offset, apply adjustment in thousandths of unit
                if glyph.x_offset.abs() > 0.01 {
                    let adj = -glyph.x_offset * (1000.0 / font_size);
                    let _ = FmtWrite::write_fmt(&mut tj_array, format_args!(" {adj:.1} "));
                }
                let _ = FmtWrite::write_fmt(&mut tj_array, format_args!("<{gid:04X}>"));
            }
            tj_array.push(']');

            let _ = writeln!(content_stream, "{tj_array} TJ");
            x_pen += shaped.total_advance_width as f64;
        }

        y_offset -= 40.0;
    }

    let _ = writeln!(content_stream, "ET");

    // Add page to doc
    let pages = doc.root().and_then(|r| doc.get(r).as_dict().and_then(|d| d.reference(b"Pages"))).expect("pages tree exists");

    let mut page = Dict::new();
    page.set(b"Type".to_vec(), Object::name("Page"));
    page.set(b"Parent".to_vec(), Object::Ref(pages));
    page.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]));
    let mut page_resources = Dict::new();
    page_resources.set(b"Font".to_vec(), Object::Dict(fonts_res));
    page.set(b"Resources".to_vec(), Object::Dict(page_resources));

    let content_obj = doc.add(Object::Stream(Stream::from_raw(Dict::new(), content_stream.clone())));
    page.set(b"Contents".to_vec(), Object::Ref(content_obj));

    let page_ref = doc.add(page);
    let _ = doc.update_dict(pages, |d| {
        let mut kids = d.get(b"Kids").and_then(|k| k.as_array().cloned()).unwrap_or_default();
        kids.push(Object::Ref(page_ref));
        d.set(b"Count".to_vec(), Object::Int(kids.len() as i64));
        d.set(b"Kids".to_vec(), Object::Array(kids));
    });

    // Save output PDF to target/shaped_test_output.pdf in workspace root
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out_dir = workspace_root.join("target");
    let _ = fs::create_dir_all(&out_dir);
    let out_path = out_dir.join("shaped_test_output.pdf");

    let save_opts = pdfcraft_cos::SaveOptions { object_streams: false, ..pdfcraft_cos::SaveOptions::default() };
    let pdf_bytes = pdfcraft_cos::write_full(&doc, &save_opts).expect("write_full succeeds");
    if let Err(e) = fs::write(&out_path, &pdf_bytes) {
        if e.raw_os_error() == Some(32) {
            let alt_path = out_dir.join("shaped_test_output_direct.pdf");
            fs::write(&alt_path, &pdf_bytes).expect("writing fallback direct test PDF file");
            println!("\n✅ Target file locked by viewer; wrote to:\n{}", alt_path.display());
            return;
        }
        panic!("writing test output PDF file: {e:?}");
    }

    assert!(out_path.exists(), "target/shaped_test_output.pdf must exist");
    assert!(!pdf_bytes.is_empty(), "PDF file must not be empty");

    let abs_path = fs::canonicalize(&out_path).unwrap_or(out_path);
    println!("\n✅ Successfully generated test PDF with complex scripts at:\n{}", abs_path.display());
}
