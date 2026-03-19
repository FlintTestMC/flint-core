use crate::{
    get_supported_version,
    index::Index,
    test_spec::{TestSpec, TestSpecLoadResult},
    utils::is_json_file,
};
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Test file loader for discovering test files in the filesystem
pub struct TestLoader {
    path: PathBuf,
    recursive: bool,
    index: Index,
}

impl TestLoader {
    pub fn new(path: &Path, recursive: bool) -> Result<Self> {
        Ok(TestLoader {
            path: path.to_path_buf(),
            recursive,
            index: Index::load(&Self::collect_test_files(path, recursive)?)?,
        })
    }

    /// Collect test files from a path (file or directory)
    ///
    /// # Arguments
    ///
    /// * `path` - Path to a single test file or directory containing tests
    /// * `recursive` - Whether to search recursively
    ///
    /// # Returns
    ///
    /// A sorted vector of PathBuf pointing to test JSON files
    pub fn collect_test_files(path: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
        let mut test_files = Vec::new();

        if path.is_file() {
            if is_json_file(path) {
                test_files.push(path.to_path_buf());
            }
        } else if path.is_dir() {
            if recursive {
                Self::collect_recursive(path, &mut test_files)?;
            } else {
                Self::collect_non_recursive(path, &mut test_files)?;
            }
        }

        // Sort for consistent ordering
        test_files.sort();
        Ok(test_files)
    }
    ///
    /// Verifies if the index is still correct
    /// # Arguments
    ///
    /// * `files`: the current test files in the directory
    ///
    /// returns: bool
    ///
    pub fn verify_index(&self, files: &Vec<PathBuf>) -> bool {
        self.index.verify(files)
    }

    ///
    /// rebuilds the index and deletes the old index.
    /// Is forced
    /// # Arguments
    ///
    /// * `files`: The current test files in the directory
    ///
    /// returns: Result<(), Error>
    ///
    pub fn rebuild_index(&mut self, files: &Vec<PathBuf>) -> anyhow::Result<()> {
        self.index.rebuild(files)
    }

    ///
    /// Only rebuilds if the index is not intact anymore.
    /// returns: Result<(), Error>
    ///
    pub fn verify_and_rebuild_index(&mut self) -> Result<bool> {
        if let Ok(files) = TestLoader::collect_test_files(&self.path, self.recursive) {
            if !self.index.verify(&files) {
                self.index.rebuild(&files)?;
                Ok(false)
            } else {
                Ok(true)
            }
        } else {
            Ok(false)
        }
    }

    /// Collect all test files recursively from a directory
    pub fn collect_all_test_files(&self) -> Result<Vec<PathBuf>> {
        let test_files = Self::collect_test_files(&self.path, self.recursive)?;
        Ok(test_files)
    }

    /// Collect test files by tags using the index system
    ///
    /// This method uses the Index to efficiently load tests that match any of the provided tags.
    /// The Index automatically manages caching and regeneration based on file changes.
    ///
    /// # Arguments
    ///
    /// * `tags` - Slice of tag names to filter tests by
    ///
    /// # Returns
    ///
    /// A vector of PathBuf pointing to test JSON files that have at least one of the specified tags
    ///
    /// # Environment Variables
    ///
    /// * `TEST_PATH` - Base directory for tests (default: "./test")
    /// * `INDEX_NAME` - Path to the index cache file (default: ".cache/index.json")
    /// * `DEFAULT_TAG` - Tag assigned to tests with no tags (default: "default")
    pub fn collect_by_tags(&self, tags: &[String]) -> Result<Vec<PathBuf>> {
        let paths = self.index.get_test_paths_from_scopes(tags)?;
        Ok(paths)
    }

    /// Load and version-check test specs from a list of paths.
    /// Uses the crate's PROTOCOL_VERSION to filter incompatible tests.
    pub fn load_specs(&self, paths: &[PathBuf], validate_cleanup: bool) -> Result<Vec<TestSpecLoadResult>> {
        let mut results = Vec::new();
        for path in paths {
            let json = std::fs::read_to_string(path)?;
            let result = TestSpec::try_load(&json, get_supported_version(), validate_cleanup)
                .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
            results.push(result);
        }
        Ok(results)
    }

    /// Collect all test files and load them with version checking.
    /// Convenience wrapper around `collect_all_test_files` + `load_specs`.
    pub fn load_all_specs(&self, validate_cleanup: bool) -> Result<Vec<TestSpecLoadResult>> {
        let paths = self.collect_all_test_files()?;
        self.load_specs(&paths, validate_cleanup)
    }

    /// Collect test files by tags and load them with version checking.
    /// Convenience wrapper around `collect_by_tags` + `load_specs`.
    pub fn load_specs_by_tags(&self, tags: &[String], validate_cleanup: bool) -> Result<Vec<TestSpecLoadResult>> {
        let paths = self.collect_by_tags(tags)?;
        self.load_specs(&paths, validate_cleanup)
    }

    /// Collect JSON files from immediate directory only (non-recursive)
    fn collect_non_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && is_json_file(&path) {
                files.push(path);
            }
        }
        Ok(())
    }

    /// Collect JSON files recursively using stack-based iteration
    fn collect_recursive(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        let mut stack = vec![root.to_path_buf()];

        while let Some(current_dir) = stack.pop() {
            for entry in std::fs::read_dir(&current_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file() && is_json_file(&path) {
                    files.push(path);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::{
        DirGuard, create_empty_file, create_non_tagged_file, create_tagged_file,
        create_test_file_with_content,
    };
    use semver::VersionReq;
    use serial_test::serial;
    use std::{env, fs};
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_collect_single_file() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = create_empty_file(temp_dir.path(), "test.json");

        let files = TestLoader::collect_test_files(&test_file, false).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], test_file);
    }

    #[test]
    #[serial]
    fn test_collect_non_json_file() {
        let temp_dir = TempDir::new().unwrap();
        let txt_file = temp_dir.path().join("test.txt");
        fs::write(&txt_file, "test").unwrap();

        let files = TestLoader::collect_test_files(&txt_file, false).unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    #[serial]
    fn test_collect_non_recursive() {
        let temp_dir = TempDir::new().unwrap();

        // Create files in root
        create_empty_file(temp_dir.path(), "test1.json");
        create_empty_file(temp_dir.path(), "test2.json");

        // Create subdirectory with file
        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        create_empty_file(&sub_dir, "test3.json");

        let files = TestLoader::collect_test_files(temp_dir.path(), false).unwrap();

        // Should only find 2 files (not the one in subdir)
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.parent().unwrap() == temp_dir.path()));
    }

    #[test]
    #[serial]
    fn test_collect_recursive() {
        let temp_dir = TempDir::new().unwrap();

        // Create files in root
        create_empty_file(temp_dir.path(), "test1.json");

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_empty_file(&sub_dir1, "test2.json");

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_empty_file(&sub_dir2, "test3.json");

        let files = TestLoader::collect_test_files(temp_dir.path(), true).unwrap();

        // Should find all 3 files
        assert_eq!(files.len(), 3);
    }

    #[test]
    #[serial]
    fn test_collect_all_test_files() {
        let temp_dir = TempDir::new().unwrap();

        let basic_content = r#"
        {
            "name": "Test 1",
            "description": "A simple test",
            "tags": ["unit", "fast"],
            "timeline": []
        }
        "#;

        create_test_file_with_content(temp_dir.path(), "test1.json", basic_content);
        create_test_file_with_content(temp_dir.path(), "test2.json", basic_content);

        let sub_dir = temp_dir.path().join("subdir");
        fs::create_dir(&sub_dir).unwrap();
        create_test_file_with_content(&sub_dir, "test3.json", basic_content);

        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        let loader = TestLoader::new(Path::new("."), true).unwrap();

        let files = loader.collect_all_test_files().unwrap();

        // Should find all 3 files
        assert_eq!(files.len(), 3);
    }

    #[test]
    #[serial]
    fn test_files_are_sorted() {
        let temp_dir = TempDir::new().unwrap();

        // Create files in non-alphabetical order
        create_empty_file(temp_dir.path(), "z_test.json");
        create_empty_file(temp_dir.path(), "a_test.json");
        create_empty_file(temp_dir.path(), "m_test.json");

        let files = TestLoader::collect_test_files(temp_dir.path(), false).unwrap();

        assert_eq!(files.len(), 3);
        // Verify they're sorted
        assert!(
            files[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("a_")
        );
        assert!(
            files[1]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("m_")
        );
        assert!(
            files[2]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("z_")
        );
    }

    #[test]
    #[serial]
    fn test_mixed_file_types() {
        let temp_dir = TempDir::new().unwrap();

        // Create various file types
        create_empty_file(temp_dir.path(), "test.json");
        fs::write(temp_dir.path().join("test.txt"), "text").unwrap();
        fs::write(temp_dir.path().join("test.md"), "markdown").unwrap();
        fs::write(temp_dir.path().join("no_extension"), "data").unwrap();

        let files = TestLoader::collect_test_files(temp_dir.path(), false).unwrap();

        // Should only find the JSON file
        assert_eq!(files.len(), 1);
        assert!(
            files[0]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".json")
        );
    }

    #[test]
    #[serial]
    pub fn brake_index_add_file() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        let loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 8180331397721424639,
  "index": {
    "test": [
      "./subdir1/nested/test3.json",
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );

        // add file
        create_tagged_file(&sub_dir2, "test4.json", &["test".to_string()]);
        let files = TestLoader::collect_test_files(Path::new("."), true).unwrap();
        assert!(!loader.verify_index(&files));
    }

    #[test]
    #[serial]
    pub fn create_index_with_no_tags_field_in_json() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_non_tagged_file(temp_dir.path(), "test1.json");

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_non_tagged_file(&sub_dir1, "test2.json");

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_non_tagged_file(&sub_dir2, "test3.json");
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 8180331397721424639,
  "index": {
    "default": [
      "./subdir1/nested/test3.json",
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );
    }

    #[test]
    #[serial]
    pub fn create_empty_index() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_empty_file(temp_dir.path(), "test1.json");

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_empty_file(&sub_dir1, "test2.json");

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_empty_file(&sub_dir2, "test3.json");
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        assert!(TestLoader::new(Path::new("."), true).is_err());
    }

    #[test]
    #[serial]
    pub fn brake_index_remove_file() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        let delete = create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        let loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 8180331397721424639,
  "index": {
    "test": [
      "./subdir1/nested/test3.json",
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );

        // remove file
        fs::remove_file(delete).unwrap();
        let files = TestLoader::collect_test_files(Path::new("."), true).unwrap();
        assert!(!loader.verify_index(&files));
    }
    #[test]
    #[serial]
    pub fn verify_ignore_file() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        let loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 8180331397721424639,
  "index": {
    "test": [
      "./subdir1/nested/test3.json",
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );

        // add file
        create_tagged_file(&sub_dir2, "test4.jsonnet", &["test".to_string()]);
        let files = TestLoader::collect_test_files(Path::new("."), true).unwrap();
        assert!(loader.verify_index(&files));
    }
    #[test]
    #[serial]
    pub fn brake_index_add_file_and_rebuild() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);

        // set index name
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());

        // create index
        let mut loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let mut index_content = fs::read_to_string(&index_path).unwrap();

        assert_eq!(
            "{\n  \"hash\": 8180331397721424639,\n  \"index\": {\n    \"test\": [\n      \"./subdir1/nested/test3.json\",\n      \"./subdir1/test2.json\",\n      \"./test1.json\"\n    ]\n  }\n}",
            index_content
        );

        // add file
        create_tagged_file(&sub_dir2, "test4.json", &["test".to_string()]);

        // verify index
        let files = TestLoader::collect_test_files(Path::new("."), true).unwrap();
        assert!(!loader.verify_index(&files));

        // rebuild index
        assert!(loader.rebuild_index(&files).is_ok());

        index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 17571090526916378731,
  "index": {
    "test": [
      "./subdir1/nested/test3.json",
      "./subdir1/nested/test4.json",
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );
    }
    #[test]
    #[serial]
    pub fn brake_index_remove_file_and_rebuild() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        let delete = create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);

        // set index name
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());

        // create index
        let mut loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let mut index_content = fs::read_to_string(&index_path).unwrap();

        assert_eq!(
            "{\n  \"hash\": 8180331397721424639,\n  \"index\": {\n    \"test\": [\n      \"./subdir1/nested/test3.json\",\n      \"./subdir1/test2.json\",\n      \"./test1.json\"\n    ]\n  }\n}",
            index_content
        );

        // remove file
        fs::remove_file(delete).unwrap();

        // verify index
        let files = TestLoader::collect_test_files(Path::new("."), true).unwrap();
        assert!(!loader.verify_index(&files));

        // rebuild index
        assert!(loader.rebuild_index(&files).is_ok());

        index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 9213419820977342414,
  "index": {
    "test": [
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );
    }
    #[test]
    #[serial]
    pub fn brake_index_add_file_and_rebuild_one_command() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);

        // set index name
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());

        // create index
        let mut loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let mut index_content = fs::read_to_string(&index_path).expect("Could not read index file");

        assert_eq!(
            "{\n  \"hash\": 8180331397721424639,\n  \"index\": {\n    \"test\": [\n      \"./subdir1/nested/test3.json\",\n      \"./subdir1/test2.json\",\n      \"./test1.json\"\n    ]\n  }\n}",
            index_content
        );

        // add file
        create_tagged_file(&sub_dir2, "test4.json", &["test".to_string()]);

        // rebuild index
        assert!(loader.verify_and_rebuild_index().is_ok());

        index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 17571090526916378731,
  "index": {
    "test": [
      "./subdir1/nested/test3.json",
      "./subdir1/nested/test4.json",
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );
    }
    #[test]
    #[serial]
    pub fn brake_index_remove_file_and_rebuild_one_command() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["test".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        let delete = create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);

        // set index name
        let index_name = "index.json";
        unsafe {
            env::set_var("INDEX_NAME", "./".to_owned() + index_name);
        }

        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());

        // create index
        let mut loader = TestLoader::new(Path::new("."), true).unwrap();
        let index_path = temp_dir.path().join(index_name);
        let mut index_content = fs::read_to_string(&index_path).expect("Could not read index file");

        assert_eq!(
            "{\n  \"hash\": 8180331397721424639,\n  \"index\": {\n    \"test\": [\n      \"./subdir1/nested/test3.json\",\n      \"./subdir1/test2.json\",\n      \"./test1.json\"\n    ]\n  }\n}",
            index_content
        );

        // remove file
        fs::remove_file(delete).unwrap();

        // rebuild index
        assert!(loader.verify_and_rebuild_index().is_ok());

        index_content = fs::read_to_string(&index_path).expect("Could not read index file");
        assert_eq!(
            r#"{
  "hash": 9213419820977342414,
  "index": {
    "test": [
      "./subdir1/test2.json",
      "./test1.json"
    ]
  }
}"#,
            index_content
        );
    }

    #[test]
    #[serial]
    fn test_load_specs_skips_higher_version() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"{
            "name": "future-test",
            "description": "requires future version",
            "flintVersion": "2.0.0",
            "tags": ["unit"],
            "timeline": []
        }"#;
        let path = create_test_file_with_content(temp_dir.path(), "future.json", content);

        let index_name = "index.json";
        unsafe { env::set_var("INDEX_NAME", "./".to_owned() + index_name) };
        let _d = DirGuard::change_to(temp_dir.path());

        // Build loader manually using just the path (bypassing index which needs tagged files)
        let results = {
            let paths = vec![path];
            let mut out = Vec::new();
            for p in &paths {
                let json = fs::read_to_string(p).unwrap();
                let r = TestSpec::try_load(&json, VersionReq::parse("1.0.0").unwrap(), false).unwrap();
                out.push(r);
            }
            out
        };

        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0], TestSpecLoadResult::Skipped { .. }),
            "expected Skipped for flintVersion 2.0.0"
        );
    }

    #[test]
    #[serial]
    fn test_load_specs_loads_matching_version() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"{
            "name": "current-test",
            "description": "matches current version",
            "flintVersion": "1.0.0",
            "tags": ["unit"],
            "timeline": []
        }"#;
        let path = create_test_file_with_content(temp_dir.path(), "current.json", content);

        let index_name = "index.json";
        unsafe { env::set_var("INDEX_NAME", "./".to_owned() + index_name) };
        let _d = DirGuard::change_to(temp_dir.path());

        let json = fs::read_to_string(&path).unwrap();
        let result = TestSpec::try_load(&json, VersionReq::parse("1.0.0").unwrap(), false).unwrap();

        assert!(
            matches!(result, TestSpecLoadResult::Loaded(_)),
            "expected Loaded for flintVersion 1.0.0"
        );
    }

    #[test]
    #[serial]
    fn test_load_specs_loads_no_version() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"{
            "name": "no-version-test",
            "description": "no flintVersion field",
            "tags": ["unit"],
            "timeline": []
        }"#;
        let path = create_test_file_with_content(temp_dir.path(), "nover.json", content);

        let index_name = "index.json";
        unsafe { env::set_var("INDEX_NAME", "./".to_owned() + index_name) };
        let _d = DirGuard::change_to(temp_dir.path());

        let json = fs::read_to_string(&path).unwrap();
        let result = TestSpec::try_load(&json, VersionReq::parse("<=1.0.0").unwrap(), false).unwrap();

        assert!(
            matches!(result, TestSpecLoadResult::Loaded(_)),
            "expected Loaded when no flintVersion"
        );
    }

    #[test]
    #[serial]
    fn test_load_all_specs_skips_higher_version() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"{
            "name": "future-test",
            "description": "requires future version",
            "flintVersion": "99.0.0",
            "tags": ["unit"],
            "timeline": []
        }"#;
        create_test_file_with_content(temp_dir.path(), "future.json", content);

        let index_name = "index.json";
        unsafe { env::set_var("INDEX_NAME", "./".to_owned() + index_name) };
        let _d = DirGuard::change_to(temp_dir.path());

        let loader = TestLoader::new(Path::new("."), true).unwrap();
        let results = loader.load_all_specs(false).unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0], TestSpecLoadResult::Skipped { .. }),
            "expected Skipped for flintVersion 99.0.0"
        );
    }

    #[test]
    #[serial]
    fn test_load_specs_by_tags_skips_higher_version() {
        let temp_dir = TempDir::new().unwrap();
        let content = r#"{
            "name": "future-tagged-test",
            "description": "requires future version",
            "flintVersion": "2.0.0",
            "tags": ["unit"],
            "timeline": []
        }"#;
        create_test_file_with_content(temp_dir.path(), "future_tagged.json", content);

        let index_name = "index.json";
        unsafe { env::set_var("INDEX_NAME", "./".to_owned() + index_name) };
        let _d = DirGuard::change_to(temp_dir.path());

        let loader = TestLoader::new(Path::new("."), true).unwrap();
        let results = loader.load_specs_by_tags(&["unit".to_string()], false).unwrap();

        assert_eq!(results.len(), 1);
        assert!(
            matches!(results[0], TestSpecLoadResult::Skipped { .. }),
            "expected Skipped for flintVersion 2.0.0"
        );
    }
}
