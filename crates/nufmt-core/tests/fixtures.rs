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

macro_rules! fixture_tests {
    ($($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                test_fixture(stringify!($name));
            }
        )*
    };
}

fixture_tests!(
    simple,
    blocks,
    variables,
    comments,
    complex,
    empty,
    comments_only,
    deeply_nested,
    unicode,
);
