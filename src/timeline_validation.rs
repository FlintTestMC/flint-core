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

/// Validate that assert actions appear before other actions on each tick.
///
/// Flint executes timeline entries in JSON order. When several actions share a
/// tick, asserts must run first so they observe the world before mutations on
/// that tick. Post-mutation checks belong on a later tick instead.
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

pub fn validate_test_file(path: &Path) -> Result<()> {
    let spec = TestSpec::from_file(&path.to_path_buf(), false)
        .with_context(|| format!("failed to load {}", path.display()))?;
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
            }),
            timeline,
            breakpoints: vec![],
        }
    }

    #[test]
    fn accepts_assert_before_place_on_same_tick() {
        let spec = spec_with_timeline(
            "valid",
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
            "invalid",
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
            "later_tick",
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
}
