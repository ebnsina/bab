//! Parsing configuration. Every field optional, every mistake survivable.

use bab_config::{Color, Config, CursorShape};

#[test]
fn an_empty_file_is_the_default_config() {
    assert_eq!(Config::parse("").unwrap(), Config::default());
}

#[test]
fn a_missing_file_is_the_default_config() {
    let (config, warning) = Config::load(std::path::Path::new("/nonexistent/bab.toml"));
    assert_eq!(config, Config::default());
    assert!(
        warning.is_none(),
        "a missing config is normal, not a problem"
    );
}

/// A typo in a colour must not leave the user without a terminal to fix it in.
#[test]
fn a_malformed_file_falls_back_with_an_explanation() {
    let dir = std::env::temp_dir().join("bab-config-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.toml");
    std::fs::write(&path, "[colors]\nforeground = \"not a colour\"\n").unwrap();

    let (config, warning) = Config::load(&path);
    assert_eq!(config, Config::default());
    assert!(warning.is_some(), "the user should be told what went wrong");
}

#[test]
fn partial_sections_keep_the_other_defaults() {
    let config = Config::parse("[font]\nsize = 18.0\n").unwrap();
    assert!((config.font.size - 18.0).abs() < f32::EPSILON);
    assert_eq!(config.window, Config::default().window);
    assert_eq!(config.colors, Config::default().colors);
}

#[test]
fn a_full_config_round_trips() {
    let text = r##"
        [font]
        size = 16.0
        file = "/tmp/mono.ttf"
        fallback = ["/tmp/bengali.ttf"]

        [window]
        padding = 20.0
        opacity = 0.8

        [cursor]
        shape = "bar"
        blink = false

        [colors]
        foreground = "#ffffff"
        background = "#000"
    "##;
    let config = Config::parse(text).unwrap();

    assert!((config.font.size - 16.0).abs() < f32::EPSILON);
    assert_eq!(config.font.file.unwrap().to_str().unwrap(), "/tmp/mono.ttf");
    assert_eq!(config.font.fallback.unwrap().len(), 1);
    assert_eq!(config.cursor.shape, CursorShape::Bar);
    assert!(!config.cursor.blink);
    assert_eq!(config.colors.foreground, Color::new(0xff, 0xff, 0xff));
    // Three-digit hex expands, so "#000" is black rather than an error.
    assert_eq!(config.colors.background, Color::new(0, 0, 0));
}

/// An unknown key is a typo. Silently ignoring it means the setting never applies and
/// the user never learns why.
#[test]
fn an_unknown_key_is_an_error() {
    assert!(Config::parse("[font]\nsizee = 18.0\n").is_err());
}

// ---- colors ----------------------------------------------------------------

#[test]
fn colors_parse_in_every_common_spelling() {
    assert_eq!(
        Color::parse("#7aa2f7").unwrap(),
        Color::new(0x7a, 0xa2, 0xf7)
    );
    assert_eq!(
        Color::parse("7aa2f7").unwrap(),
        Color::new(0x7a, 0xa2, 0xf7)
    );
    assert_eq!(Color::parse("#ABC").unwrap(), Color::new(0xaa, 0xbb, 0xcc));
    assert_eq!(
        Color::parse("  #7AA2F7  ").unwrap(),
        Color::new(0x7a, 0xa2, 0xf7)
    );
}

#[test]
fn nonsense_is_not_a_color() {
    assert!(Color::parse("").is_err());
    assert!(Color::parse("#12345").is_err());
    assert!(Color::parse("#gggggg").is_err());
    assert!(Color::parse("rebeccapurple").is_err());
}

// ---- sanitising ------------------------------------------------------------

/// A zero font size divides by zero downstream. Correcting quietly beats not starting.
#[test]
fn absurd_values_are_clamped() {
    let config =
        Config::parse("[font]\nsize = 0.0\n\n[window]\nopacity = 5.0\npadding = -3.0\n").unwrap();
    assert!(config.font.size >= 4.0);
    assert!((config.window.opacity - 1.0).abs() < f32::EPSILON);
    assert!(config.window.padding >= 0.0);
}

/// Indexing a short palette would panic. A wrong-length one is discarded whole.
#[test]
fn a_palette_of_the_wrong_length_falls_back() {
    let config = Config::parse("[colors]\nansi = [\"#fff\", \"#000\"]\n").unwrap();
    assert_eq!(config.colors.ansi.len(), 16);
    assert_eq!(config.colors.ansi, Config::default().colors.ansi);
}

#[test]
fn a_sixteen_colour_palette_is_kept() {
    let entries = (0..16)
        .map(|_| "\"#123456\"")
        .collect::<Vec<_>>()
        .join(", ");
    let config = Config::parse(&format!("[colors]\nansi = [{entries}]\n")).unwrap();
    assert_eq!(config.colors.ansi.len(), 16);
    assert_eq!(config.colors.ansi[0], Color::new(0x12, 0x34, 0x56));
}
