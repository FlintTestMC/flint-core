use crate::test_spec::{TestSpec, TimelineEntry};
use std::collections::{HashMap, HashSet};

/// Represents an aggregated timeline from multiple tests
#[derive(Debug)]
pub struct TimelineAggregate<'a> {
    /// Merged timeline: tick -> Vec<(test_idx, timeline_entry, value_idx)>
    /// The value_idx is used for actions with multiple ticks (e.g., AssertState with multiple values)
    pub timeline: HashMap<u32, Vec<(usize, &'a TimelineEntry, usize)>>,

    /// Maximum tick across all tests
    pub max_tick: u32,

    /// All breakpoints from all tests
    pub breakpoints: HashSet<u32>,
}

impl<'a> TimelineAggregate<'a> {
    /// Aggregate timelines from multiple tests into a single merged timeline
    ///
    /// # Arguments
    ///
    /// * `tests_with_offsets` - Vector of (TestSpec, offset) tuples
    ///
    /// # Returns
    ///
    /// A TimelineAggregate containing the merged timeline, max tick, and all breakpoints
    pub fn from_tests(tests_with_offsets: &'a [(TestSpec, [i32; 3])]) -> Self {
        let mut timeline: HashMap<u32, Vec<(usize, &TimelineEntry, usize)>> = HashMap::new();
        let mut max_tick = 0;
        let mut breakpoints = HashSet::new();

        for (test_idx, (test, _offset)) in tests_with_offsets.iter().enumerate() {
            // Find the maximum tick for this test
            let test_max_tick = test.max_tick();
            if test_max_tick > max_tick {
                max_tick = test_max_tick;
            }

            // Collect breakpoints from this test
            for &bp in &test.breakpoints {
                breakpoints.insert(bp);
            }

            // Expand timeline entries with multiple ticks
            // For example, an AssertState action at ticks [0, 5, 10] will create 3 entries
            for entry in &test.timeline {
                let ticks = entry.at.to_vec();
                for (value_idx, tick) in ticks.iter().enumerate() {
                    timeline
                        .entry(*tick)
                        .or_default()
                        .push((test_idx, entry, value_idx));
                }
            }
        }

        TimelineAggregate {
            timeline,
            max_tick,
            breakpoints,
        }
    }

    /// Get the number of unique ticks with actions
    pub fn unique_tick_count(&self) -> usize {
        self.timeline.len()
    }

    /// Find the next tick with scheduled actions after the given tick
    pub fn next_action_tick(&self, current_tick: u32) -> Option<u32> {
        self.timeline
            .keys()
            .filter(|&&tick| tick > current_tick)
            .min()
            .copied()
    }

    /// Find the next breakpoint after the given tick
    pub fn next_breakpoint(&self, current_tick: u32) -> Option<u32> {
        self.breakpoints
            .iter()
            .filter(|&&tick| tick > current_tick)
            .min()
            .copied()
    }

    /// Find the next event (action or breakpoint) after the given tick
    pub fn next_event_tick(&self, current_tick: u32) -> Option<u32> {
        let next_action = self.next_action_tick(current_tick);
        let next_bp = self.next_breakpoint(current_tick);

        match (next_action, next_bp) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_spec::{ActionType, Block, TickSpec};

    fn create_test_spec(
        name: &str,
        timeline: Vec<TimelineEntry>,
        breakpoints: Vec<u32>,
    ) -> TestSpec {
        TestSpec {
            flint_version: None,
            name: name.to_string(),
            description: None,
            tags: vec![],
            dependencies: vec![],
            setup: None,
            timeline,
            breakpoints,
        }
    }

    #[test]
    fn test_single_test_aggregation() {
        let entry1 = TimelineEntry {
            at: TickSpec::Single(0),
            action_type: ActionType::Place {
                pos: [0, 0, 0],
                block: Block {
                    id: "stone".to_string(),
                    properties: Default::default(),
                },
            },
        };
        let entry2 = TimelineEntry {
            at: TickSpec::Single(5),
            action_type: ActionType::Place {
                pos: [1, 0, 0],
                block: Block {
                    id: "dirt".to_string(),
                    properties: Default::default(),
                },
            },
        };

        let test = create_test_spec("test1", vec![entry1, entry2], vec![]);
        let tests = vec![(test, [0, 0, 0])];

        let aggregate = TimelineAggregate::from_tests(&tests);

        assert_eq!(aggregate.max_tick, 5);
        assert_eq!(aggregate.unique_tick_count(), 2);
        assert_eq!(aggregate.breakpoints.len(), 0);
    }

    #[test]
    fn test_multiple_tests_aggregation() {
        let entry1 = TimelineEntry {
            at: TickSpec::Single(0),
            action_type: ActionType::Place {
                pos: [0, 0, 0],
                block: Block {
                    id: "stone".to_string(),
                    properties: Default::default(),
                },
            },
        };
        let entry2 = TimelineEntry {
            at: TickSpec::Single(10),
            action_type: ActionType::Place {
                pos: [1, 0, 0],
                block: Block {
                    id: "dirt".to_string(),
                    properties: Default::default(),
                },
            },
        };

        let test1 = create_test_spec("test1", vec![entry1], vec![5]);
        let test2 = create_test_spec("test2", vec![entry2], vec![]);

        let tests = vec![(test1, [0, 0, 0]), (test2, [10, 0, 0])];

        let aggregate = TimelineAggregate::from_tests(&tests);

        assert_eq!(aggregate.max_tick, 10);
        assert_eq!(aggregate.unique_tick_count(), 2);
        assert_eq!(aggregate.breakpoints.len(), 1);
        assert!(aggregate.breakpoints.contains(&5));
    }

    #[test]
    fn test_multiple_ticks_expansion() {
        let entry = TimelineEntry {
            at: TickSpec::Multiple(vec![0, 5, 10]),
            action_type: ActionType::AssertState {
                pos: [0, 0, 0],
                state: "power".to_string(),
                values: vec!["0".to_string(), "5".to_string(), "15".to_string()],
            },
        };

        let test = create_test_spec("test1", vec![entry], vec![]);
        let tests = vec![(test, [0, 0, 0])];

        let aggregate = TimelineAggregate::from_tests(&tests);

        assert_eq!(aggregate.max_tick, 10);
        assert_eq!(aggregate.unique_tick_count(), 3);

        // Verify each tick has an entry
        assert!(aggregate.timeline.contains_key(&0));
        assert!(aggregate.timeline.contains_key(&5));
        assert!(aggregate.timeline.contains_key(&10));

        // Verify value_idx is correct for each tick
        let tick0 = &aggregate.timeline[&0][0];
        assert_eq!(tick0.2, 0); // value_idx should be 0

        let tick5 = &aggregate.timeline[&5][0];
        assert_eq!(tick5.2, 1); // value_idx should be 1

        let tick10 = &aggregate.timeline[&10][0];
        assert_eq!(tick10.2, 2); // value_idx should be 2
    }

    #[test]
    fn test_next_event_tick() {
        let entry1 = TimelineEntry {
            at: TickSpec::Single(5),
            action_type: ActionType::Place {
                pos: [0, 0, 0],
                block: Block {
                    id: "stone".to_string(),
                    properties: Default::default(),
                },
            },
        };
        let entry2 = TimelineEntry {
            at: TickSpec::Single(15),
            action_type: ActionType::Place {
                pos: [1, 0, 0],
                block: Block {
                    properties: Default::default(),
                    id: "dirt".to_string(),
                },
            },
        };

        let test = create_test_spec("test1", vec![entry1, entry2], vec![10, 20]);
        let tests = vec![(test, [0, 0, 0])];

        let aggregate = TimelineAggregate::from_tests(&tests);

        // From tick 0, next event should be action at tick 5
        assert_eq!(aggregate.next_event_tick(0), Some(5));

        // From tick 5, next event should be breakpoint at tick 10
        assert_eq!(aggregate.next_event_tick(5), Some(10));

        // From tick 10, next event should be action at tick 15
        assert_eq!(aggregate.next_event_tick(10), Some(15));

        // From tick 15, next event should be breakpoint at tick 20
        assert_eq!(aggregate.next_event_tick(15), Some(20));

        // After all events, should return None
        assert_eq!(aggregate.next_event_tick(20), None);
    }
}
