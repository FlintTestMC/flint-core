use crate::test_spec::MinimalTestSpec;
use rayon::prelude::*;
use rustc_hash::{FxHashSet, FxHasher};
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions, create_dir_all};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use crate::utils::{get_default_tag, get_index_name};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub hash: u64,
    pub index: BTreeMap<String, Vec<String>>,
    #[serde(skip_serializing)]
    index_name: String,
}

impl Index {
    pub fn index_exists(&self) -> bool {
        Path::new(&self.index_name).exists()
    }

    pub fn open_index() -> anyhow::Result<Index> {
        let file = File::open(get_index_name())?;
        let reader = BufReader::new(file);
        let index = serde_json::from_reader(reader)?;
        Ok(index)
    }

    pub fn save_index(&self) -> anyhow::Result<()> {
        let index_string = to_string_pretty(&self)?;
        let path = Path::new(&self.index_name);

        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true) // optional, overwrite existing file
            .open(path)?;
        file.write_all(index_string.as_bytes())?;
        Ok(())
    }

    ///
    /// Creates the Index.
    /// If an index is found and valid, it loads it from the file.
    /// If not it will create a new index
    /// # Arguments
    ///
    /// * `all_files`: The test files which are the base for the index.
    ///
    /// returns: Result<Index, Error>
    ///
    pub fn load(all_files: &Vec<PathBuf>) -> anyhow::Result<Self> {
        if let Ok(index) = Index::open_index() {
            let hash = get_hash(all_files);
            if index.hash == hash {
                Ok(index)
            } else {
                let mut index = Index::empty();
                index.generate_index(all_files)?;
                Ok(index)
            }
        } else {
            let mut index = Index::empty();
            index.generate_index(all_files)?;
            Ok(index)
        }
    }

    ///
    /// Verifies if the index is still correct
    /// # Arguments
    ///
    /// * `files`: the current test files in the directory
    ///
    /// returns: bool
    ///
    pub fn verify(&self, files: &Vec<PathBuf>) -> bool {
        self.hash == get_hash(files)
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
    pub fn rebuild(&mut self, files: &Vec<PathBuf>) -> anyhow::Result<()> {
        self.index = BTreeMap::new();
        self.generate_index(files)?;
        Ok(())
    }

    /// Creates an empty Index
    pub(crate) fn empty() -> Self {
        Self {
            hash: 0,
            index: BTreeMap::new(),
            index_name: get_index_name(),
        }
    }

    /// Creates an index other all files
    ///
    /// returns: Result<Index, Error>
    ///
    /// # Environment Variables
    ///
    /// * `TEST_PATH` - Base directory for tests (default: "./test")
    /// * `INDEX_NAME` - Path to the index cache file (default: ".cache/index.json")
    /// * `DEFAULT_TAG` - Tag assigned to tests with no tags (default: "default")
    pub fn generate_index(&mut self, all_files: &Vec<PathBuf>) -> anyhow::Result<()> {
        let hash = get_hash(all_files);
        let default_tag = get_default_tag();

        // Phase 1: parse all files in parallel
        let parsed: Vec<anyhow::Result<(PathBuf, MinimalTestSpec)>> = all_files
            .par_iter()
            .map(|i| {
                let file = File::open(i)?;
                let reader = BufReader::new(file);
                let test: MinimalTestSpec = serde_json::from_reader(reader).map_err(|e| {
                    anyhow::anyhow!("{}:{}:{}: {}", i.display(), e.line(), e.column(), e)
                })?;
                Ok((i.clone(), test))
            })
            .collect();

        // Phase 2: merge into BTreeMap (sequential, cheap)
        for result in parsed {
            let (i, test) = result?;
            let path_str = i.to_string_lossy().into_owned();
            for tag in &test.tags {
                self.index
                    .entry(tag.clone())
                    .or_default()
                    .push(path_str.clone());
            }
            for id in &test.minecraft_ids {
                self.index
                    .entry(id.clone())
                    .or_default()
                    .push(path_str.clone());
            }
            if test.tags.is_empty() {
                self.index
                    .entry(default_tag.clone())
                    .or_default()
                    .push(path_str);
            }
        }

        self.hash = hash;
        self.save_index()?;
        Ok(())
    }

    /// Searches through the index all tests with specific tags
    /// Also validates if the index is correct or not and recreates the index if not.
    ///
    /// # Arguments
    ///
    /// * `scope`: all tags
    ///
    /// returns: Result<Vec<String, Global>, Error>
    ///
    /// # Environment Variables
    ///
    /// * `TEST_PATH` - Base directory for tests (default: "./test")
    /// * `INDEX_NAME` - Path to the index cache file (default: ".cache/index.json")
    /// * `DEFAULT_TAG` - Tag assigned to tests with no tags (default: "default")
    pub fn get_test_paths_from_scopes(&self, scope: &[String]) -> Vec<PathBuf> {
        let mut test_paths: Vec<PathBuf> = Vec::new();
        let mut seen: FxHashSet<&str> = FxHashSet::default();
        for tag in scope {
            match self.index.get(tag) {
                None => {} //TODO: add here a better and nicer warning/info message. Because it shouldn't silent fail but also not like before through an error
                Some(paths) => {
                    for path in paths {
                        if seen.insert(path.as_str()) {
                            test_paths.push(PathBuf::from(path));
                        }
                    }
                }
            }
        }
        test_paths
    }
}

/// Creates a hash of a vec of PathBuf
///
/// # Arguments
///
/// * `vec` - The vector of paths to hash
///
/// # Returns
///
/// The calculated hash as u64
pub(crate) fn get_hash(vec: &Vec<PathBuf>) -> u64 {
    let mut hasher = FxHasher::default();
    vec.len().hash(&mut hasher);
    for path in vec {
        path.hash(&mut hasher);
        if let Ok(meta) = std::fs::metadata(path)
            && let Ok(mtime) = meta.modified()
        {
            mtime.hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::TestLoader;
    use crate::utils::tests::{
        DirGuard, create_empty_file, create_non_tagged_file, create_tagged_file, to_relative_path,
    };
    use serial_test::serial;
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    fn generate_index_and_return_index(temp_dir: TempDir) -> serde_json::Value {
        let index_path = temp_dir.path().join("index.json");
        let mut index = Index::empty();
        let files = TestLoader::collect_test_files(temp_dir.path(), true).unwrap();
        let relative = to_relative_path(temp_dir.path(), &files);
        let _d = DirGuard::change_to(temp_dir.path());
        println!("new: {}", env::current_dir().unwrap().display());
        index.index_name = "./index.json".to_string();
        index.generate_index(relative.as_ref()).unwrap();
        let content = fs::read_to_string(&index_path).expect("Could not read index file");
        serde_json::from_str(&content).unwrap()
    }

    #[test]
    #[serial]
    fn test_generate_index() {
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

        // Generate the index and test!
        let v = generate_index_and_return_index(temp_dir);
        assert!(v["hash"].as_u64().unwrap() > 0);
        assert_eq!(
            v["index"],
            serde_json::json!({
                "test": ["./subdir1/nested/test3.json", "./subdir1/test2.json", "./test1.json"]
            })
        );
    }
    #[test]
    #[serial]
    fn test_generate_index_default_case() {
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

        let v = generate_index_and_return_index(temp_dir);
        assert!(v["hash"].as_u64().unwrap() > 0);
        assert_eq!(
            v["index"],
            serde_json::json!({
                "default": ["./subdir1/nested/test3.json", "./subdir1/test2.json", "./test1.json"]
            })
        );
    }

    #[test]
    #[serial]
    fn test_generate_index_multiple_tags() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(
            &sub_dir1,
            "test2.json",
            &["test".to_string(), "test3".to_string()],
        );

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);

        let v = generate_index_and_return_index(temp_dir);
        assert!(v["hash"].as_u64().unwrap() > 0);
        assert_eq!(
            v["index"],
            serde_json::json!({
                "test": ["./subdir1/nested/test3.json", "./subdir1/test2.json", "./test1.json"],
                "test3": ["./subdir1/test2.json"]
            })
        );
    }

    #[test]
    #[serial]
    fn test_generate_index_ignore_non_json() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["".to_string()]);

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(&sub_dir1, "test2.json", &["".to_string()]);

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["".to_string()]);
        create_tagged_file(&sub_dir2, "test4.jsonnet", &["".to_string()]);

        let v = generate_index_and_return_index(temp_dir);
        assert!(v["hash"].as_u64().unwrap() > 0);
        assert_eq!(
            v["index"],
            serde_json::json!({
                "": ["./subdir1/nested/test3.json", "./subdir1/test2.json", "./test1.json"]
            })
        );
    }

    #[test]
    #[serial]
    fn test_generate_index_all_combined() {
        let temp_dir = TempDir::new().unwrap();

        // Setup the directory
        // Create files in root
        create_tagged_file(temp_dir.path(), "test1.json", &["test1".to_string()]);
        create_non_tagged_file(temp_dir.path(), "test2.json");

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_tagged_file(
            &sub_dir1,
            "test2.json",
            &["test".to_string(), "test1".to_string()],
        );
        create_non_tagged_file(&sub_dir1, "test3.json");

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_tagged_file(&sub_dir2, "test3.json", &["test".to_string()]);
        create_tagged_file(&sub_dir2, "test4.jsonnet", &["".to_string()]);

        let v = generate_index_and_return_index(temp_dir);
        assert!(v["hash"].as_u64().unwrap() > 0);
        assert_eq!(
            v["index"],
            serde_json::json!({
                "default": ["./subdir1/test3.json", "./test2.json"],
                "test": ["./subdir1/nested/test3.json", "./subdir1/test2.json"],
                "test1": ["./subdir1/test2.json", "./test1.json"]
            })
        );
    }

    #[test]
    #[serial]
    fn test_hash() {
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
        let relative = to_relative_path(temp_dir.path(), &files);

        assert_eq!(files.len(), 3);
        let h1 = get_hash(&relative);
        assert_ne!(h1, 0);
        assert_eq!(h1, get_hash(&relative));
    }

    #[test]
    #[serial]
    fn test_hash_ignore_not_json() {
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
        create_empty_file(&sub_dir2, "test3.jsonnet");

        let files = TestLoader::collect_test_files(temp_dir.path(), true).unwrap();
        let relative = to_relative_path(temp_dir.path(), &files);

        assert_eq!(files.len(), 3);
        let h1 = get_hash(&relative);
        assert_ne!(h1, 0);
        assert_eq!(h1, get_hash(&relative));
    }
}
