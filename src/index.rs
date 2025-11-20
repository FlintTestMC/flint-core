use crate::test_spec::TestSpec;
use serde::{Deserialize, Serialize};
use serde_json::to_string_pretty;
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::fs::{File, OpenOptions, create_dir_all};
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use crate::utils::{get_default_tag, get_index_name};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Index {
    pub hash: u64,
    pub index: BTreeMap<String, Vec<String>>,
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

    /// Creates an empty Index
    fn empty() -> Self {
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

        for i in all_files {
            println!("Indexing test file: {}", i.to_string_lossy());
            let file = File::open(i.clone())?;
            let reader = BufReader::new(file);
            let test: TestSpec = serde_json::from_reader(reader)?;
            // Add test to all tags
            for tag in &test.tags {
                self.index
                    .entry(tag.clone())
                    .or_default()
                    .push(i.to_string_lossy().parse()?);
            }

            // add test to default tag if no tag specified
            if test.tags.is_empty() {
                self.index
                    .entry(get_default_tag())
                    .or_default()
                    .push(i.to_string_lossy().parse()?);
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
    pub fn get_test_paths_from_scopes(&self, scope: &[String]) -> anyhow::Result<Vec<PathBuf>> {
        let mut test_paths = vec![];
        for tag in scope {
            if !self.index.contains_key(tag) {
                return Err(anyhow::anyhow!("Tag '{}' not found in index", tag));
            } else {
                let paths = self.index.get(tag).unwrap();
                for path in paths {
                    // would load the same test more than once if the test will be in more scopes
                    if !test_paths.contains(&PathBuf::from(path)) {
                        test_paths.push(PathBuf::from(path));
                    }
                }
            }
        }
        Ok(test_paths)
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
fn get_hash(vec: &Vec<PathBuf>) -> u64 {
    let mut hasher = DefaultHasher::new();
    vec.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_emtpy_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "{}").unwrap();
        path
    }
    fn create_tagged_file(dir: &Path, name: &str, tags: &[String]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, "{}").unwrap();
        path
    }

    #[test]
    fn test_generate_index() {
        let temp_dir = TempDir::new().unwrap();

        // Create files in root
        create_emtpy_file(temp_dir.path(), "test1.json");

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_emtpy_file(&sub_dir1, "test2.json");

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_emtpy_file(&sub_dir2, "test3.json");

        let files = TestLoader::collect_test_files(temp_dir.path(), true).unwrap();

        // Should find all 3 files
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_hash() {
        let temp_dir = TempDir::new().unwrap();

        // Create files in root
        create_emtpy_file(temp_dir.path(), "test1.json");

        // Create nested subdirectories with files
        let sub_dir1 = temp_dir.path().join("subdir1");
        fs::create_dir(&sub_dir1).unwrap();
        create_emtpy_file(&sub_dir1, "test2.json");

        let sub_dir2 = sub_dir1.join("nested");
        fs::create_dir(&sub_dir2).unwrap();
        create_emtpy_file(&sub_dir2, "test3.json");

        let files = TestLoader::collect_test_files(temp_dir.path(), true).unwrap();

        // Should find all 3 files
        assert_eq!(files.len(), 3);
    }
}
