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

/// Check if a file is a JSON file by extension and excludes the index.json
pub fn is_json_file(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
        && path.file_name().is_some()
        && path.file_name().unwrap().to_string_lossy() != "index.json"
}

#[cfg(test)]
pub mod tests {
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::{env, fs};

    pub struct DirGuard {
        original: PathBuf,
    }

    impl DirGuard {
        pub fn change_to<P: AsRef<Path>>(new_dir: P) -> Self {
            let original = env::current_dir().unwrap();
            env::set_current_dir(new_dir).unwrap();
            println!("original: {}", original.display());
            DirGuard { original }
        }
    }

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
        }
    }
    pub fn create_empty_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "{}").unwrap();
        path
    }
    pub fn create_tagged_file(dir: &Path, name: &str, tags: &[String]) -> PathBuf {
        let path = dir.join(name);
        let obj = json!({
            "name": "Test 1",
            "description": "A simple test",
            "tags": tags,
            "timeline": []
        });

        fs::write(&path, obj.to_string()).unwrap();
        path
    }
    pub fn create_non_tagged_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let obj = json!({
            "name": "Test 1",
            "description": "A simple test",
            "tags": [],
            "timeline": []
        });

        fs::write(&path, obj.to_string()).unwrap();
        path
    }
    pub fn create_non_tag_field_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        let obj = json!({
            "name": "Test 1",
            "description": "A simple test",
            "timeline": []
        });

        fs::write(&path, obj.to_string()).unwrap();
        path
    }
    pub fn create_test_file_with_content(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    pub fn to_relative_path(root: &Path, files: &[PathBuf]) -> Vec<PathBuf> {
        files
            .iter()
            .map(|p| {
                let rel = p.strip_prefix(root).unwrap();
                PathBuf::from(".").join(rel)
            })
            .collect::<Vec<_>>()
    }
}
