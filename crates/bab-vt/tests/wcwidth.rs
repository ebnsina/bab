//! Our cell width must equal the `wcwidth()` the application on the other end called.
//!
//! This is the width contract, checked against the actual system function rather than
//! against a crate's opinion of it. When they diverged, `bab` allocated nine cells for
//! a Bengali word that zsh had laid out in eight, and the line corrupted itself.

#![cfg(unix)]

use bab_vt::{char_cells, cluster_cells};

// `libc` does not bind `wcwidth`, so declare it. It is POSIX on every unix.
unsafe extern "C" {
    fn wcwidth(c: libc::wchar_t) -> libc::c_int;
}

/// The platform's `wcwidth`, which is what a TUI application calls.
fn system_wcwidth(c: char) -> usize {
    // The C library needs a UTF-8 locale, or every non-ASCII character reports -1.
    unsafe {
        libc::setlocale(libc::LC_ALL, c"en_US.UTF-8".as_ptr());
        wcwidth(c as libc::wchar_t).max(0) as usize
    }
}

fn system_width(text: &str) -> usize {
    text.chars().map(system_wcwidth).sum()
}

const SAMPLES: &[&str] = &[
    "প্রধানমন্ত্রী",
    "বাংলা",
    "ব্ল",
    "ক্ষ",
    "কি",
    "র্ক",
    "স্ত্র",
    "hello",
    "héllo",
    "世界",
    "e\u{301}",
];

#[test]
fn our_width_agrees_with_the_system_wcwidth() {
    for text in SAMPLES {
        assert_eq!(
            cluster_cells(text),
            system_width(text),
            "width mismatch for {text:?}: bab={} wcwidth={}",
            cluster_cells(text),
            system_width(text),
        );
    }
}

/// The specific disagreement that corrupted the screen. `unicode-width` calls this
/// character one cell wide; every application calls it zero.
#[test]
fn a_bengali_spacing_vowel_sign_occupies_no_cell() {
    assert_eq!(
        char_cells('\u{09C0}'),
        0,
        "BENGALI VOWEL SIGN II must be zero width"
    );
    assert_eq!(
        char_cells('\u{09BE}'),
        0,
        "BENGALI VOWEL SIGN AA must be zero width"
    );
    assert_eq!(
        char_cells('\u{09CD}'),
        0,
        "BENGALI SIGN VIRAMA must be zero width"
    );
}

#[test]
fn zero_width_joiners_occupy_no_cell() {
    assert_eq!(char_cells('\u{200D}'), 0);
    assert_eq!(char_cells('\u{200C}'), 0);
}

/// The word that exposed the bug: eight cells, not nine.
#[test]
fn the_word_that_broke_it_is_eight_cells() {
    assert_eq!(cluster_cells("প্রধানমন্ত্রী"), 8);
}

#[test]
fn wide_characters_still_take_two_cells() {
    assert_eq!(char_cells('世'), 2);
    assert_eq!(cluster_cells("世界"), 4);
}
