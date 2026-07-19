use crate::test_spec::{ActionType, TestSpec, TimelineEntry};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::Path;

/// A timeline ordering violation on a single tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineOrderViolation {
    pub tick: u32,
    pub timeline_index: usize,
    pub action: &'static str,
}

/// Convert a human-readable test name to a snake_case slug for file matching.
pub fn slugify_test_name(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut prev_underscore = false;

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            for lower in ch.to_lowercase() {
                slug.push(lower);
            }
            prev_underscore = false;
        } else if !prev_underscore {
            slug.push('_');
            prev_underscore = true;
        }
    }

    slug.trim_matches('_').to_string()
}

/// Expected slug derived from a JSON file path (stem, `-` → `_`, lowercase).
pub fn expected_slug_from_path(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .with_context(|| format!("invalid test file name: {}", path.display()))?;
    Ok(stem.replace('-', "_").to_ascii_lowercase())
}

/// Validate that assert actions appear before other actions on each tick.
pub fn validate_timeline_order(spec: &TestSpec) -> Result<()> {
    let violations = timeline_order_violations(spec);
    if violations.is_empty() {
        return Ok(());
    }

    let mut message = format!(
        "Test '{}': timeline ordering violations (assert must come before other actions on the same tick):\n",
        spec.name
    );
    for violation in &violations {
        message.push_str(&format!(
            "  tick {} at timeline[{}] ({}) appears after a non-assert action\n",
            violation.tick, violation.timeline_index, violation.action
        ));
    }
    message.push_str(
        "Move post-mutation asserts to a later tick, or reorder entries so asserts come first.",
    );
    bail!(message)
}

pub fn timeline_order_violations(spec: &TestSpec) -> Vec<TimelineOrderViolation> {
    let mut by_tick: BTreeMap<u32, Vec<(usize, &TimelineEntry)>> = BTreeMap::new();

    for (index, entry) in spec.timeline.iter().enumerate() {
        for tick in entry.at.to_vec() {
            by_tick.entry(tick).or_default().push((index, entry));
        }
    }

    let mut violations = Vec::new();
    for (tick, entries) in by_tick {
        if entries.len() < 2 {
            continue;
        }

        let mut seen_non_assert = false;
        for (index, entry) in entries {
            if matches!(entry.action_type, ActionType::Assert { .. }) {
                if seen_non_assert {
                    violations.push(TimelineOrderViolation {
                        tick,
                        timeline_index: index,
                        action: "assert",
                    });
                }
            } else {
                seen_non_assert = true;
            }
        }
    }

    violations
}

/// Validate that `spec.name` slugifies to the file stem in snake_case.
pub fn validate_test_name(spec: &TestSpec, path: &Path) -> Result<()> {
    let expected = expected_slug_from_path(path)?;
    let actual = slugify_test_name(&spec.name);
    if actual == expected {
        return Ok(());
    }

    bail!(
        "Test name mismatch in {}: name '{}' slugs to '{}', expected '{}' from file name",
        path.display(),
        spec.name,
        actual,
        expected
    )
}

/// Validate that the test's cleanup region is anchored around the world origin.
pub fn validate_cleanup_region_contains_origin(spec: &TestSpec) -> Result<()> {
    let region = spec.cleanup_region();
    let min = region[0];
    let max = region[1];

    if (0..3).all(|axis| min[axis] <= 0 && max[axis] >= 0) {
        return Ok(());
    }

    bail!(
        "Test '{}': cleanup region [{},{},{}] to [{},{},{}] must contain the origin [0,0,0]",
        spec.name,
        min[0],
        min[1],
        min[2],
        max[0],
        max[1],
        max[2]
    )
}

pub fn validate_test_file(path: &Path) -> Result<()> {
    let spec = TestSpec::from_file(&path.to_path_buf(), false)
        .with_context(|| format!("failed to load {}", path.display()))?;
    validate_test_name(&spec, path)?;
    validate_cleanup_region_contains_origin(&spec)?;
    validate_timeline_order(&spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_spec::{
        AssertType, Block, BlockCheck, BlockSpec, CleanupSpec, SetupSpec, TickSpec,
    };

    fn spec_with_timeline(name: &str, timeline: Vec<TimelineEntry>) -> TestSpec {
        TestSpec {
            flint_version: None,
            name: name.to_string(),
            description: None,
            tags: vec![],
            minecraft_ids: vec![],
            dependencies: vec![],
            setup: Some(SetupSpec {
                cleanup: Some(CleanupSpec {
                    region: [[0, 0, 0], [1, 1, 1]],
                }),
                player: None,
                world: Default::default(),
            }),
            timeline,
            breakpoints: vec![],
        }
    }

    #[test]
    fn slugify_human_readable_name() {
        assert_eq!(
            slugify_test_name("Fence Row Connections"),
            "fence_row_connections"
        );
        assert_eq!(
            slugify_test_name("Create Nether Portal X 21 21 All"),
            "create_nether_portal_x_21_21_all"
        );
    }

    #[test]
    fn expected_slug_converts_hyphens_and_case() {
        assert_eq!(
            expected_slug_from_path(Path::new("tests/wall-2-to-3-length.json")).unwrap(),
            "wall_2_to_3_length"
        );
        assert_eq!(
            expected_slug_from_path(Path::new("tests/create_nether_portal_X_21_21_All.json"))
                .unwrap(),
            "create_nether_portal_x_21_21_all"
        );
    }

    #[test]
    fn accepts_assert_before_place_on_same_tick() {
        let spec = spec_with_timeline(
            "Valid Test",
            vec![
                TimelineEntry {
                    at: TickSpec::Single(1),
                    action_type: ActionType::Assert {
                        checks: vec![AssertType::Block(BlockCheck {
                            pos: [0, 0, 0],
                            is: BlockSpec::Single(Block::new("minecraft:air")),
                        })],
                    },
                },
                TimelineEntry {
                    at: TickSpec::Single(1),
                    action_type: ActionType::Place {
                        pos: [0, 0, 0],
                        block: Block::new("minecraft:stone"),
                    },
                },
            ],
        );

        validate_timeline_order(&spec).unwrap();
    }

    #[test]
    fn rejects_assert_after_place_on_same_tick() {
        let spec = spec_with_timeline(
            "Invalid Test",
            vec![
                TimelineEntry {
                    at: TickSpec::Single(1),
                    action_type: ActionType::Place {
                        pos: [0, 0, 0],
                        block: Block::new("minecraft:stone"),
                    },
                },
                TimelineEntry {
                    at: TickSpec::Single(1),
                    action_type: ActionType::Assert {
                        checks: vec![AssertType::Block(BlockCheck {
                            pos: [0, 0, 0],
                            is: BlockSpec::Single(Block::new("minecraft:stone")),
                        })],
                    },
                },
            ],
        );

        assert!(validate_timeline_order(&spec).is_err());
    }

    #[test]
    fn allows_post_mutation_assert_on_later_tick() {
        let spec = spec_with_timeline(
            "Later Tick",
            vec![
                TimelineEntry {
                    at: TickSpec::Single(0),
                    action_type: ActionType::Place {
                        pos: [0, 0, 0],
                        block: Block::new("minecraft:stone"),
                    },
                },
                TimelineEntry {
                    at: TickSpec::Single(1),
                    action_type: ActionType::Assert {
                        checks: vec![AssertType::Block(BlockCheck {
                            pos: [0, 0, 0],
                            is: BlockSpec::Single(Block::new("minecraft:stone")),
                        })],
                    },
                },
            ],
        );

        validate_timeline_order(&spec).unwrap();
    }

    #[test]
    fn rejects_test_name_mismatch() {
        let spec = spec_with_timeline("Wrong Name", vec![]);
        let err =
            validate_test_name(&spec, Path::new("tests/fence_row_connections.json")).unwrap_err();
        assert!(err.to_string().contains("fence_row_connections"));
    }

    #[test]
    fn accepts_matching_human_name() {
        let spec = spec_with_timeline("Fence Row Connections", vec![]);
        validate_test_name(&spec, Path::new("tests/fence_row_connections.json")).unwrap();
    }

    #[test]
    fn accepts_cleanup_region_around_origin() {
        let mut spec = spec_with_timeline("At Origin", vec![]);
        spec.setup
            .as_mut()
            .unwrap()
            .cleanup
            .as_mut()
            .unwrap()
            .region = [[-10, -2, -5], [4, 8, 12]];

        validate_cleanup_region_contains_origin(&spec).unwrap();
    }

    #[test]
    fn rejects_cleanup_region_in_the_sky() {
        let mut spec = spec_with_timeline("In The Sky", vec![]);
        spec.setup
            .as_mut()
            .unwrap()
            .cleanup
            .as_mut()
            .unwrap()
            .region = [[-2, 100, -2], [2, 104, 2]];

        let err = validate_cleanup_region_contains_origin(&spec).unwrap_err();
        assert!(err.to_string().contains("must contain the origin [0,0,0]"));
    }

    #[test]
    fn rejects_cleanup_region_away_from_origin_on_horizontal_axis() {
        let mut spec = spec_with_timeline("Far Away", vec![]);
        spec.setup
            .as_mut()
            .unwrap()
            .cleanup
            .as_mut()
            .unwrap()
            .region = [[100, -2, -2], [104, 2, 2]];

        assert!(validate_cleanup_region_contains_origin(&spec).is_err());
    }
}
