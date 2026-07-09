//! Does the shaper actually handle Bengali, and does the width contract hold?
//!
//! `harfrust`'s Indic coverage is newer than upstream HarfBuzz's. These tests are how
//! that claim gets checked rather than assumed. If one fails, the `Shaper` trait exists
//! so a `harfbuzz-sys` backend can replace it.

use std::path::PathBuf;
use std::sync::Arc;

use bab_text::{CellMetrics, Face, FontStack, HarfRustShaper, Shaper, place};

const BENGALI: &str = "NotoSansBengali-Regular.ttf";
const MONO: &str = "GeistMono-Regular.ttf";

fn load(file: &str) -> Face {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "assets",
        "fonts",
        file,
    ]
    .iter()
    .collect();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    Face::new(file, Arc::new(bytes)).expect("parsing font")
}

/// Geist Mono first, Noto Sans Bengali behind it: the shipped default chain.
fn stack() -> FontStack {
    FontStack::new(vec![load(MONO), load(BENGALI)]).unwrap()
}

fn shape(cluster: &str, face: &Face) -> bab_text::ShapedCluster {
    HarfRustShaper.shape(cluster, face, 0).expect("shaping")
}

// ---- fallback --------------------------------------------------------------

/// The premise of the whole font chain: our default mono face has no Bengali.
#[test]
fn geist_mono_has_no_bengali() {
    let mono = load(MONO);
    assert!(mono.has_glyph('a'));
    assert!(
        !mono.has_glyph('ব'),
        "Geist Mono unexpectedly covers Bengali"
    );
}

#[test]
fn latin_resolves_to_the_primary_face() {
    let stack = stack();
    let (index, face) = stack.resolve("a");
    assert_eq!(index, 0);
    assert_eq!(face.name(), MONO);
}

#[test]
fn bengali_falls_through_to_noto() {
    let stack = stack();
    let (index, face) = stack.resolve("বাং");
    assert_eq!(index, 1);
    assert_eq!(face.name(), BENGALI);
}

/// Requiring coverage of joiners would push every conjunct to a fallback face.
#[test]
fn zero_width_joiners_do_not_affect_coverage() {
    let bengali = load(BENGALI);
    assert!(bengali.covers("ব্\u{200D}ল"));
}

/// Nothing covers this, so we render tofu from the primary face rather than nothing.
#[test]
fn uncovered_text_falls_back_to_the_primary_face() {
    let stack = stack();
    let (index, _) = stack.resolve("世");
    assert_eq!(index, 0);
}

// ---- Bengali shaping -------------------------------------------------------

/// A conjunct occupies the width of its base alone: `ব` + hasant + `ল` draws `ল` as a
/// zero-advance below-form. Glyph count stays at three; only one glyph advances.
#[test]
fn bengali_conjunct_collapses_to_one_advance() {
    let face = load(BENGALI);
    let conjunct = shape("ব্ল", &face);
    let base = shape("ব", &face);

    assert!(
        !conjunct.has_tofu(),
        "shaper produced .notdef for a conjunct"
    );
    assert_eq!(
        conjunct.advancing_glyphs(),
        1,
        "conjunct should advance once"
    );
    assert_eq!(
        conjunct.advance, base.advance,
        "conjunct should be as wide as its base"
    );
}

/// Some conjuncts fuse into a single glyph outright. `ক` + hasant + `ষ` is the classic.
#[test]
fn bengali_conjunct_can_fuse_into_one_glyph() {
    let face = load(BENGALI);
    let shaped = shape("ক্ষ", &face);

    assert!(!shaped.has_tofu());
    assert_eq!(
        shaped.glyphs.len(),
        1,
        "expected a single fused ligature glyph"
    );
}

/// Reph: `র` + hasant becomes a mark above the *following* consonant. So the consonant
/// is drawn first and the reph trails it as a zero-advance glyph.
#[test]
fn reph_reorders_above_the_following_consonant() {
    let face = load(BENGALI);
    let shaped = shape("র্ক", &face);
    let ka = shape("ক", &face);

    assert!(!shaped.has_tofu());
    assert_eq!(
        shaped.glyphs[0].glyph_id, ka.glyphs[0].glyph_id,
        "consonant should draw first"
    );
    assert!(
        shaped.glyphs[1].is_mark(),
        "reph should be a zero-advance mark"
    );
    assert_eq!(shaped.advance, ka.advance);
}

/// A pre-base matra is stored *after* its consonant and drawn *before* it. If the
/// shaper does not reorder, the terminal renders Bengali in the wrong order — which is
/// the bug this project exists to fix.
#[test]
fn pre_base_matra_is_drawn_before_its_consonant() {
    let face = load(BENGALI);
    let shaped = shape("কি", &face);
    let ka = shape("ক", &face);

    assert!(!shaped.has_tofu());
    assert_eq!(shaped.glyphs.len(), 2);
    assert_ne!(
        shaped.glyphs[0].glyph_id, ka.glyphs[0].glyph_id,
        "matra must be drawn first"
    );
    assert_eq!(
        shaped.glyphs[1].glyph_id, ka.glyphs[0].glyph_id,
        "consonant follows the matra"
    );
}

/// Marks carry no width, so glyph count says nothing about how wide a cluster is.
#[test]
fn marks_do_not_advance() {
    let face = load(BENGALI);
    let shaped = shape("স্ত্র", &face);

    assert!(!shaped.has_tofu());
    assert!(shaped.glyphs.len() > shaped.advancing_glyphs());
}

#[test]
fn khanda_ta_shapes_without_tofu() {
    let face = load(BENGALI);
    let shaped = shape("ৎ", &face);
    assert!(!shaped.has_tofu());
}

/// A combining mark contributes no advance of its own.
#[test]
fn combining_mark_has_zero_advance() {
    let face = load(BENGALI);
    let base = shape("ক", &face);
    let with_mark = shape("কঁ", &face);
    assert_eq!(base.advance, with_mark.advance);
}

// ---- the width contract ----------------------------------------------------

/// Bengali is drawn in a proportional fallback face, so it will not fill its cells.
/// We centre it and keep the slack. We never scale to fit.
#[test]
fn a_narrow_cluster_is_centred_in_its_span() {
    let face = load(BENGALI);
    let shaped = shape("ক", &face);
    let cell = CellMetrics {
        width: 100.0,
        height: 200.0,
    };

    // A span far wider than the glyph, to make the arithmetic unambiguous.
    let placement = place(&shaped, face.units_per_em(), 10.0, 8, cell);

    assert!(placement.slack > 0.0);
    assert!(!placement.overhangs());
    assert!((placement.x_offset - placement.slack / 2.0).abs() < f32::EPSILON);
}

/// When the glyph is wider than its allocated cells, it overhangs symmetrically.
/// The grid does not widen: the application already decided the span.
#[test]
fn a_wide_cluster_overhangs_rather_than_reflowing() {
    let face = load(BENGALI);
    let shaped = shape("ক", &face);
    let cell = CellMetrics {
        width: 1.0,
        height: 2.0,
    };

    let placement = place(&shaped, face.units_per_em(), 100.0, 1, cell);

    assert!(placement.overhangs());
    assert!(placement.x_offset < 0.0);
}

/// Font tables report descent as negative. If it is not negated on the way in,
/// `line_height` subtracts instead of adding and lines overlap on screen.
#[test]
fn descent_is_positive_and_line_height_exceeds_the_font_size() {
    let face = load(MONO);
    let size = 18.0;
    let metrics = face.metrics(size);

    assert!(metrics.ascent > 0.0);
    assert!(metrics.descent > 0.0, "descent must be stored positive");
    assert!(
        metrics.line_height() > size,
        "line height {} should exceed the font size {size}",
        metrics.line_height()
    );
}

#[test]
fn font_units_convert_to_pixels() {
    let face = load(BENGALI);
    let upem = face.units_per_em();
    assert!(upem > 0);
    assert!((bab_text::to_px(i32::from(upem), upem, 16.0) - 16.0).abs() < f32::EPSILON);
}
