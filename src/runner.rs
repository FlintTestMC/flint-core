//! Test execution engine.
//!
//! The `TestRunner` loads tests and executes them against a server adapter.

use crate::results::{
    ActionOutcome, AssertFailure, AssertPosition, AssertionResult, InfoType, TestResult,
    TestSummary,
};
use crate::test_spec::{ActionType, AssertType, Item, PlayerSlot};
use crate::timeline::TimelineAggregate;
use crate::traits::{FlintAdapter, FlintPlayer, FlintWorld};
use crate::{Block, TestSpec, TestSpecLoadResult};
use std::sync::Arc;
use std::time::Instant;

/// Configuration for test execution
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct TestRunConfig {
    /// Enable debug mode with breakpoints
    pub debug_enabled: bool,
    /// Run tests in parallel
    pub parallel: bool,
    /// Maximum parallel test worlds
    pub max_parallel_worlds: usize,
}

#[allow(dead_code)]
impl Default for TestRunConfig {
    fn default() -> Self {
        Self {
            debug_enabled: false,
            parallel: false,
            max_parallel_worlds: 4,
        }
    }
}

/// Test execution engine
pub struct TestRunner<A: FlintAdapter> {
    adapter: Arc<A>,
    // config: TestRunConfig,
}

impl<A: FlintAdapter> TestRunner<A> {
    pub fn new(adapter: Arc<A>) -> Self {
        Self { adapter }
    }

    /// Run a single test
    pub fn run_test(&self, spec: &TestSpec) -> TestResult {
        let start_time = Instant::now();
        let mut world = self.adapter.create_test_world();

        // Build timeline for single test (no offset)
        let tests_with_offsets = vec![(spec.clone(), [0i32, 0, 0])];
        let timeline = TimelineAggregate::from_tests(&tests_with_offsets);

        let mut result = TestResult::new(&spec.name);
        result.minecraft_ids = spec.minecraft_ids.clone();

        // Player is created on demand when player actions are used
        let mut player: Option<Box<dyn FlintPlayer>> = None;

        // Initialize player from config if present (advanced mode)
        if let Some(setup) = &spec.setup
            && let Some(player_config) = setup.player.as_ref()
        {
            let p = player.get_or_insert_with(|| world.create_player());

            // Set initial inventory
            for (slot_name, item) in &player_config.inventory {
                p.set_slot(*slot_name, Some(item));
            }

            // Set initial hotbar selection
            p.select_hotbar(player_config.selected_hotbar);
        }

        // Execute timeline tick by tick
        for tick in 0..=timeline.max_tick {
            // Execute actions for this tick
            if let Some(actions) = timeline.timeline.get(&tick) {
                for (_test_idx, entry, _value_idx) in actions.iter() {
                    match self.execute_action(&mut *world, &mut player, &entry.action_type, tick) {
                        ActionOutcome::Action => {}
                        ActionOutcome::AssertPassed => {
                            result.add_assertion(AssertionResult::Success(tick));
                        }
                        ActionOutcome::AssertFailed(failures) => {
                            for fail in failures {
                                result.add_assertion(AssertionResult::Failure(fail));
                            }
                        }
                    }
                }
            }

            // Advance game tick
            world.do_tick();
        }

        result.total_ticks = timeline.max_tick;
        result.execution_time_ms = start_time.elapsed().as_millis() as u64;
        result
    }

    /// Run multiple tests. Uses parallel execution when `config.parallel` is true.
    pub fn run_tests(&self, specs: &[TestSpecLoadResult]) -> TestSummary {
        let results: Vec<TestResult> = specs
            .iter()
            .map(|load_result| match load_result {
                TestSpecLoadResult::Loaded(spec) => self.run_test(spec),
                TestSpecLoadResult::Skipped { spec, reason } => {
                    TestResult::skipped(&spec.name, reason)
                }
            })
            .collect();
        TestSummary::from_results(results)
    }

    /// Execute a single action
    fn execute_action(
        &self,
        world: &mut dyn FlintWorld,
        player: &mut Option<Box<dyn FlintPlayer>>,
        action: &ActionType,
        _tick: u32,
    ) -> ActionOutcome {
        match action {
            ActionType::Place { pos, block } => {
                let pos = [pos[0], pos[1], pos[2]];
                world.set_block(pos, block);
                ActionOutcome::Action
            }

            ActionType::PlaceEach { blocks } => {
                for placement in blocks {
                    let pos = [placement.pos[0], placement.pos[1], placement.pos[2]];
                    world.set_block(pos, &placement.block);
                }
                ActionOutcome::Action
            }

            ActionType::Fill { region, with } => {
                // Flint handles fill by iterating set_block
                // Handle potentially inverted coordinates
                let min_x = region[0][0].min(region[1][0]);
                let max_x = region[0][0].max(region[1][0]);
                let min_y = region[0][1].min(region[1][1]);
                let max_y = region[0][1].max(region[1][1]);
                let min_z = region[0][2].min(region[1][2]);
                let max_z = region[0][2].max(region[1][2]);

                for x in min_x..=max_x {
                    for y in min_y..=max_y {
                        for z in min_z..=max_z {
                            world.set_block([x, y, z], with);
                        }
                    }
                }
                ActionOutcome::Action
            }

            ActionType::Remove { pos } => {
                let pos = [pos[0], pos[1], pos[2]];
                let air = Block {
                    id: "minecraft:air".to_string(),
                    properties: Default::default(),
                };
                world.set_block(pos, &air);
                ActionOutcome::Action
            }

            ActionType::Assert { checks } => {
                let mut failures = Vec::new();
                for check in checks {
                    match check {
                        AssertType::Block(block) => {
                            let pos = [block.pos[0], block.pos[1], block.pos[2]];
                            let actual = world.get_block(pos);
                            let expected_blocks = block.is.to_vec();

                            if !expected_blocks
                                .iter()
                                .any(|expected| block_matches(&actual, expected))
                            {
                                let expected_str = expected_blocks
                                    .iter()
                                    .map(|b| b.to_command())
                                    .collect::<Vec<_>>()
                                    .join(" or ");
                                failures.push(AssertFailure {
                                    tick: _tick,
                                    error_message: format!(
                                        "Block mismatch at {:?}: expected '{}', got '{}'",
                                        pos,
                                        expected_str,
                                        actual.to_command(),
                                    ),
                                    position: AssertPosition::from_array(pos),
                                    execution_time_ms: None,
                                    expected: InfoType::Blocks(expected_blocks),
                                    actual: InfoType::Block(actual),
                                });
                            }
                        }
                        AssertType::Inventory(inv) => {
                            let p = player.get_or_insert_with(|| world.create_player());
                            let data: Vec<String>;
                            if let Some(item) = inv.is.clone() {
                                data = item.data.keys().cloned().collect();
                            } else {
                                data = vec![]
                            }
                            let actual = p.get_slot(inv.slot, data).unwrap_or(Item::empty());
                            let expected = inv.is.clone().unwrap_or(Item::empty());
                            if !item_matches(&actual, &expected) {
                                failures.push(AssertFailure::new_item(
                                    _tick, &expected, &actual, inv.slot,
                                ));
                            }
                        }
                        #[allow(unused)]
                        _ => {
                            println!("Unsupported assertion type: {:?}", check);
                        }
                    }
                }
                if failures.is_empty() {
                    ActionOutcome::AssertPassed
                } else {
                    ActionOutcome::AssertFailed(failures)
                }
            }

            ActionType::UseItemOn { pos, face, item } => {
                // Create player on demand if not already created
                let p = player.get_or_insert_with(|| world.create_player());
                let pos = [pos[0], pos[1], pos[2]];

                // Simple mode: if item is specified, set it in hotbar1 and select it
                if let Some(item_id) = item {
                    let item = Item::new(item_id);
                    p.set_slot(PlayerSlot::Hotbar1, Some(&item));
                    p.select_hotbar(1);
                }

                p.use_item_on(pos, face);
                ActionOutcome::Action
            }

            ActionType::SetSlot { slot, item, count } => {
                // Create player on demand if not already created
                let p = player.get_or_insert_with(|| world.create_player());
                if let Some(item_id) = item {
                    let item = Item::with_count(item_id, *count);
                    p.set_slot(*slot, Some(&item));
                } else {
                    p.set_slot(*slot, None);
                }
                ActionOutcome::Action
            }

            ActionType::SelectHotbar { slot } => {
                // Create player on demand if not already created
                let p = player.get_or_insert_with(|| world.create_player());
                p.select_hotbar(*slot);
                ActionOutcome::Action
            }
        }
    }
}

fn check_id(actual: &str, expected: &str) -> bool {
    if actual != expected {
        // Also try without minecraft: prefix
        let expected_id = if let Some(stripped) = expected.strip_prefix("minecraft:") {
            stripped
        } else {
            expected
        };
        let actual_id = if let Some(stripped) = actual.strip_prefix("minecraft:") {
            stripped
        } else {
            actual
        };
        if actual_id != expected_id {
            return false;
        }
    }
    true
}

/// Check if actual block matches expected.
fn block_matches(actual: &Block, expected: &Block) -> bool {
    // Check block ID
    if !check_id(&actual.id, &expected.id) {
        return false;
    }

    // Check properties if specified in expected
    for (key, expected_value) in &expected.properties {
        if let Some(actual_value) = actual.properties.get(key) {
            if actual_value != expected_value {
                return false;
            }
        } else {
            // Property expected but not found in actual block - this is a mismatch
            return false;
        }
    }

    true
}

fn item_matches(actual: &Item, expected: &Item) -> bool {
    // Check item ID
    if !check_id(&actual.id, &expected.id) {
        return false;
    }
    if actual.count != expected.count {
        return false;
    }
    for (key, expected_value) in &expected.data {
        if let Some(actual_value) = actual.data.get(key) {
            if actual_value != expected_value {
                return false;
            }
        } else {
            // Property expected but not found in actual item - this is a mismatch
            return false;
        }
    }
    true
}
