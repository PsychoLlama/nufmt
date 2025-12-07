use nufmt_core::{Config, format_source};
use std::fs;
use std::path::Path;

fn test_fixture(name: &str) {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let input_path = fixtures_dir.join(format!("{name}.nu"));
    let expected_path = fixtures_dir.join(format!("{name}.expected.nu"));

    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", input_path.display()));
    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", expected_path.display()));

    let config = Config::default();
    let result = format_source(&input, &config)
        .unwrap_or_else(|e| panic!("Failed to format {}: {e}", input_path.display()));

    assert_eq!(
        result, expected,
        "Fixture {name} did not match expected output"
    );
}

#[test]
fn test_fixture_simple() {
    test_fixture("simple");
}

#[test]
fn test_fixture_blocks() {
    test_fixture("blocks");
}

#[test]
fn test_fixture_variables() {
    test_fixture("variables");
}

#[test]
fn test_fixture_comments() {
    test_fixture("comments");
}

#[test]
fn test_fixture_complex() {
    test_fixture("complex");
}

#[test]
fn test_fixture_empty() {
    test_fixture("empty");
}

#[test]
fn test_fixture_comments_only() {
    test_fixture("comments_only");
}

#[test]
fn test_fixture_deeply_nested() {
    test_fixture("deeply_nested");
}

#[test]
fn test_fixture_unicode() {
    test_fixture("unicode");
}
