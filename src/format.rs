use crate::results::{AssertFailure, AssertionResult, TestResult};
use colored::Colorize;
use std::collections::BTreeMap;
use std::time::Duration;

/// Extract failures from test results as (test_name, AssertFailure) pairs
fn extract_failures(results: &[TestResult]) -> Vec<(String, AssertFailure)> {
    results
        .iter()
        .flat_map(|r| {
            r.assertions.iter().filter_map(move |a| {
                if let AssertionResult::Failure(f) = a {
                    Some((r.test_name.clone(), f.clone()))
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Print results as JSON to stdout
pub fn print_json(results: &[TestResult], elapsed: Duration) {
    let total = results.len();
    let skipped = results.iter().filter(|r| r.skipped).count();
    let failed = results.iter().filter(|r| !r.success && !r.skipped).count();
    let passed = total - skipped - failed;

    let failures = extract_failures(results);
    let failure_objects: Vec<serde_json::Value> = failures
        .iter()
        .map(|(name, detail)| {
            serde_json::json!({
                "test": name,
                "tick": detail.tick,
                "expected": String::from(&detail.expected),
                "actual": String::from(&detail.actual),
                "position": detail.position,
            })
        })
        .collect();

    let test_objects: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            let mut obj = serde_json::json!({
                "name": r.test_name,
                "success": r.success,
                "skipped": r.skipped,
                "total_ticks": r.total_ticks,
                "execution_time_ms": r.execution_time_ms,
            });
            if let Some(reason) = &r.skip_reason {
                obj["skip_reason"] = serde_json::Value::String(reason.clone());
            }
            obj
        })
        .collect();

    let output = serde_json::json!({
        "summary": {
            "total": total,
            "passed": passed,
            "failed": failed,
            "skipped": skipped,
            "duration_secs": elapsed.as_secs_f64(),
        },
        "tests": test_objects,
        "failures": failure_objects,
    });

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}

/// Print results in TAP (Test Anything Protocol) version 13 format
pub fn print_tap(results: &[TestResult]) {
    println!("TAP version 13");
    println!("1..{}", results.len());

    let failures = extract_failures(results);
    // Build a lookup from test name to failure detail
    let failure_map: std::collections::HashMap<&str, &AssertFailure> = failures
        .iter()
        .map(|(name, detail)| (name.as_str(), detail))
        .collect();

    for (i, result) in results.iter().enumerate() {
        let number = i + 1;
        if result.skipped {
            let reason = result.skip_reason.as_deref().unwrap_or("skipped");
            println!("ok {} - {} # SKIP {}", number, result.test_name, reason);
        } else if result.success {
            println!("ok {} - {}", number, result.test_name);
        } else {
            println!("not ok {} - {}", number, result.test_name);
            if let Some(detail) = failure_map.get(result.test_name.as_str()) {
                println!("  ---");
                println!(
                    "  message: \"expected {}, got {}\"",
                    String::from(&detail.expected),
                    String::from(&detail.actual)
                );
                println!("  at: {}", detail.position);
                println!("  tick: {}", detail.tick);
                println!("  ...");
            }
        }
    }
}

/// Print results in JUnit XML format
pub fn print_junit(results: &[TestResult], elapsed: Duration) {
    let total = results.len();
    let failed = results.iter().filter(|r| !r.success && !r.skipped).count();
    let skipped = results.iter().filter(|r| r.skipped).count();

    let failures = extract_failures(results);
    let failure_map: std::collections::HashMap<&str, &AssertFailure> = failures
        .iter()
        .map(|(name, detail)| (name.as_str(), detail))
        .collect();

    println!(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    println!(
        r#"<testsuites tests="{}" failures="{}" skipped="{}" time="{:.3}">"#,
        total,
        failed,
        skipped,
        elapsed.as_secs_f64()
    );
    println!(
        r#"  <testsuite name="flintmc" tests="{}" failures="{}" skipped="{}" time="{:.3}">"#,
        total,
        failed,
        skipped,
        elapsed.as_secs_f64()
    );

    for result in results {
        // Split test name into classname (directory path) and name (leaf)
        let (classname, name) = match result.test_name.rsplit_once('/') {
            Some((prefix, leaf)) => (prefix, leaf),
            None => ("", result.test_name.as_str()),
        };

        let time = result.execution_time_ms as f64 / 1000.0;

        if result.skipped {
            println!(
                r#"    <testcase classname="{}" name="{}" time="{:.3}">"#,
                xml_escape(classname),
                xml_escape(name),
                time
            );
            if let Some(reason) = &result.skip_reason {
                println!(r#"      <skipped message="{}"/>"#, xml_escape(reason));
            } else {
                println!(r#"      <skipped/>"#);
            }
            println!("    </testcase>");
        } else if result.success {
            println!(
                r#"    <testcase classname="{}" name="{}" time="{:.3}" />"#,
                xml_escape(classname),
                xml_escape(name),
                time
            );
        } else {
            println!(
                r#"    <testcase classname="{}" name="{}" time="{:.3}">"#,
                xml_escape(classname),
                xml_escape(name),
                time
            );
            if let Some(detail) = failure_map.get(result.test_name.as_str()) {
                println!(
                    r#"      <failure message="expected {}, got {} at {} tick {}"/>"#,
                    xml_escape(&String::from(&detail.expected)),
                    xml_escape(&String::from(&detail.actual)),
                    detail.position,
                    detail.tick
                );
            } else {
                println!(r#"      <failure message="assertion failed"/>"#);
            }
            println!("    </testcase>");
        }
    }

    println!("  </testsuite>");
    println!("</testsuites>");
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Print a separator line
fn print_separator(separator_width: usize) {
    println!("{}", "═".repeat(separator_width).dimmed());
}

/// Print verbose test summary (used in -v mode)
pub fn print_test_summary(results: &[TestResult], separator_width: usize) {
    println!("\n{}", "═".repeat(separator_width).dimmed());
    println!("{}", "Test Summary".cyan().bold());
    print_separator(separator_width);

    let total_skipped = results.iter().filter(|r| r.skipped).count();
    let total_failed = results.iter().filter(|r| !r.success && !r.skipped).count();
    let total_passed = results.len() - total_skipped - total_failed;

    for result in results {
        let status = if result.skipped {
            "SKIP".yellow().bold()
        } else if result.success {
            "PASS".green().bold()
        } else {
            "FAIL".red().bold()
        };
        println!("  [{}] {}", status, result.test_name);
    }

    if total_skipped > 0 {
        println!(
            "\n{} tests run: {} passed, {} failed, {} skipped\n",
            results.len(),
            total_passed.to_string().green(),
            total_failed.to_string().red(),
            total_skipped.to_string().yellow()
        );
    } else {
        println!(
            "\n{} tests run: {} passed, {} failed\n",
            results.len(),
            total_passed.to_string().green(),
            total_failed.to_string().red()
        );
    }
}

/// Format concise summary as a plain string (no ANSI colors)
pub fn format_concise_summary(results: &[TestResult], elapsed: Duration) -> String {
    let total = results.len();
    let total_skipped = results.iter().filter(|r| r.skipped).count();
    let total_failed = results.iter().filter(|r| !r.success && !r.skipped).count();
    let total_passed = total - total_skipped - total_failed;
    let secs = elapsed.as_secs_f64();

    let mut out = String::new();
    out.push('\n');
    if total_failed == 0 {
        let skip_note = if total_skipped > 0 {
            format!(", {} skipped", format_number(total_skipped))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "✓ All {} tests passed ({:.3}s){}\n",
            format_number(total),
            secs,
            skip_note
        ));
    } else {
        out.push_str(&format!(
            "{} of {} tests failed ({:.3}s)\n",
            format_number(total_failed),
            format_number(total),
            secs
        ));
        out.push('\n');
        let failures = extract_failures(results);
        format_failure_tree(&failures, &mut out);
        out.push('\n');
        if total_skipped > 0 {
            out.push_str(&format!(
                "{} passed, {} failed, {} skipped\n",
                format_number(total_passed),
                format_number(total_failed),
                format_number(total_skipped)
            ));
        } else {
            out.push_str(&format!(
                "{} passed, {} failed\n",
                format_number(total_passed),
                format_number(total_failed)
            ));
        }
    }
    out.push('\n');
    out
}

/// Print concise summary (default mode, with colors)
pub fn print_concise_summary(results: &[TestResult], elapsed: Duration) {
    let total = results.len();
    let total_skipped = results.iter().filter(|r| r.skipped).count();
    let total_failed = results.iter().filter(|r| !r.success && !r.skipped).count();
    let total_passed = total - total_skipped - total_failed;
    let secs = elapsed.as_secs_f64();

    println!();
    if total_failed == 0 {
        if total_skipped > 0 {
            println!(
                "{} All {} tests passed ({:.3}s), {} skipped",
                "✓".green().bold(),
                format_number(total),
                secs,
                format_number(total_skipped).yellow()
            );
        } else {
            println!(
                "{} All {} tests passed ({:.3}s)",
                "✓".green().bold(),
                format_number(total),
                secs
            );
        }
    } else {
        println!(
            "{} of {} tests failed ({:.3}s)",
            format_number(total_failed).red().bold(),
            format_number(total),
            secs
        );
        println!();
        let failures = extract_failures(results);
        print_failure_tree(&failures);
        println!();
        if total_skipped > 0 {
            println!(
                "{} passed, {} failed, {} skipped",
                format_number(total_passed).green(),
                format_number(total_failed).red(),
                format_number(total_skipped).yellow()
            );
        } else {
            println!(
                "{} passed, {} failed",
                format_number(total_passed).green(),
                format_number(total_failed).red()
            );
        }
    }
    println!();
}

pub fn create_ci_output(results: &[TestResult], empty: bool) -> String {
    let test_objects: Vec<serde_json::Value> = results
        .iter()
        .filter(|r| empty || !r.minecraft_ids.is_empty())
        .map(|r| {
            serde_json::json!({
                "name": r.test_name,
                "ids": r.minecraft_ids,
                "success": r.success,
            })
        })
        .collect();
    serde_json::to_string_pretty(&test_objects).unwrap()
}

/// Format a number with comma separators (e.g., 1247 -> "1,247")
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            result.push(',');
        }
        result.push(c);
    }
    result
}

// ── Failure tree rendering ──────────────────────────────────

/// A tree node for grouping failures by path segments
pub struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    failure: Option<AssertFailure>,
}

impl TreeNode {
    fn new() -> Self {
        Self {
            children: BTreeMap::new(),
            failure: None,
        }
    }

    fn insert(&mut self, segments: &[&str], detail: AssertFailure) {
        if segments.is_empty() {
            self.failure = Some(detail);
            return;
        }
        let child = self
            .children
            .entry(segments[0].to_string())
            .or_insert_with(TreeNode::new);
        if segments.len() == 1 {
            child.failure = Some(detail);
        } else {
            child.insert(&segments[1..], detail);
        }
    }
}

/// Format the failure tree into a string (no ANSI colors)
fn format_failure_tree(failures: &[(String, AssertFailure)], out: &mut String) {
    let mut root = TreeNode::new();

    for (name, detail) in failures {
        let segments: Vec<&str> = name.split('/').collect();
        root.insert(&segments, detail.clone());
    }

    let keys: Vec<_> = root.children.keys().cloned().collect();
    for (i, key) in keys.iter().enumerate() {
        let is_last = i == keys.len() - 1;
        let child = root.children.get(key).unwrap();
        format_tree_node(key, child, "", is_last, out);
    }
}

fn format_tree_node(name: &str, node: &TreeNode, prefix: &str, is_last: bool, out: &mut String) {
    let connector = if is_last { "└── " } else { "├── " };
    let child_prefix = if is_last { "    " } else { "│   " };

    if node.children.is_empty() {
        if let Some(ref detail) = node.failure {
            out.push_str(&format!("{}{}{}\n", prefix, connector, name));
            let detail_connector = if is_last { "    " } else { "│   " };
            out.push_str(&format!(
                "{}{}└─ t{}: expected {}, got {} @ {}\n",
                prefix,
                detail_connector,
                detail.tick,
                String::from(&detail.expected),
                String::from(&detail.actual),
                detail.position
            ));
        } else {
            out.push_str(&format!("{}{}{}\n", prefix, connector, name));
        }
    } else {
        out.push_str(&format!("{}{}{}\n", prefix, connector, name));
        let new_prefix = format!("{}{}", prefix, child_prefix);
        let keys: Vec<_> = node.children.keys().cloned().collect();
        for (i, key) in keys.iter().enumerate() {
            let child_is_last = i == keys.len() - 1;
            let child = node.children.get(key).unwrap();
            format_tree_node(key, child, &new_prefix, child_is_last, out);
        }
    }
}

/// Print the failure tree
fn print_failure_tree(failures: &[(String, AssertFailure)]) {
    let mut root = TreeNode::new();

    for (name, detail) in failures {
        let segments: Vec<&str> = name.split('/').collect();
        root.insert(&segments, detail.clone());
    }

    // Render each top-level child
    let keys: Vec<_> = root.children.keys().cloned().collect();
    for (i, key) in keys.iter().enumerate() {
        let is_last = i == keys.len() - 1;
        let child = root.children.get(key).unwrap();
        render_tree_node(key, child, "", is_last);
    }
}

fn render_tree_node(name: &str, node: &TreeNode, prefix: &str, is_last: bool) {
    let connector = if is_last { "└── " } else { "├── " };
    let child_prefix = if is_last { "    " } else { "│   " };

    if node.children.is_empty() {
        // Leaf node: print name with failure detail
        if let Some(ref detail) = node.failure {
            println!("{}{}{}", prefix, connector, name);
            let detail_connector = if is_last { "    " } else { "│   " };
            println!(
                "{}{}└─ t{}: expected {}, got {} @ {}",
                prefix,
                detail_connector,
                detail.tick,
                String::from(&detail.expected).green(),
                String::from(&detail.actual).red(),
                detail.position
            );
        } else {
            println!("{}{}{}", prefix, connector, name);
        }
    } else {
        // Branch node
        println!("{}{}{}", prefix, connector, name);
        let new_prefix = format!("{}{}", prefix, child_prefix);
        let keys: Vec<_> = node.children.keys().cloned().collect();
        for (i, key) in keys.iter().enumerate() {
            let child_is_last = i == keys.len() - 1;
            let child = node.children.get(key).unwrap();
            render_tree_node(key, child, &new_prefix, child_is_last);
        }
    }
}
