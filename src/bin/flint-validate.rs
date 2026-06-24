use anyhow::{Context, Result, bail};
use flint_core::loader::TestLoader;
use flint_core::timeline_validation::validate_test_file;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        bail!("usage: flint-validate <PATH> [--recursive]");
    }

    let recursive = args.iter().any(|arg| arg == "--recursive");
    let paths: Vec<PathBuf> = args
        .into_iter()
        .filter(|arg| arg != "--recursive")
        .map(PathBuf::from)
        .collect();

    if paths.is_empty() {
        bail!("usage: flint-validate <PATH> [--recursive]");
    }

    let mut files = Vec::new();
    for path in paths {
        files.extend(collect_files(&path, recursive)?);
    }

    files.sort();
    files.dedup();

    if files.is_empty() {
        bail!("no JSON test files found");
    }

    let mut failures = 0;
    for file in files {
        if let Err(error) = validate_test_file(&file) {
            failures += 1;
            eprintln!("{error:#}");
        }
    }

    if failures > 0 {
        bail!("{failures} test file(s) failed timeline validation");
    }

    Ok(())
}

fn collect_files(path: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    TestLoader::collect_test_files(path, recursive)
        .with_context(|| format!("failed to collect tests from {}", path.display()))
}
