//! The example config we ship must parse, or it teaches the wrong syntax.

use bab_config::Config;

#[test]
fn the_shipped_example_parses() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../bab.example.toml");
    let text = std::fs::read_to_string(path).expect("reading bab.example.toml");

    let config = Config::parse(&text).expect("the example config must parse");
    // It documents the defaults, so it should produce them.
    assert_eq!(config, Config::default());
}
