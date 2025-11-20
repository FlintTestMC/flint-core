use std::{env, path::Path};

pub fn get_index_name() -> String {
    env::var("INDEX_NAME").unwrap_or(".cache/index.json".to_string())
}

pub fn get_default_tag() -> String {
    env::var("DEFAULT_TAG").unwrap_or("default".to_string())
}
pub fn get_test_path() -> String {
    env::var("TEST_PATH").unwrap_or("./test".to_string())
}

/// Check if a file is a JSON file by extension
pub fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
}
