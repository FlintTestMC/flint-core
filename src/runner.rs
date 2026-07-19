//! Test execution engine.
//!
//! The `TestRunner` loads tests and executes them against a server adapter.

use crate::results::{
    ActionOutcome, AssertEntityFail, AssertFailure, AssertTimeFail, AssertionResult, TestResult,
    TestSummary,
};
use crate::test_spec::{ActionType, AssertType, EntityCheck, EntityNbt, Item, PlayerSlot};
use crate::timeline::TimelineAggregate;
use crate::traits::{EntityState, FlintAdapter, FlintPlayer, FlintWorld};
use crate::{Block, TestSpec, TestSpecLoadResult};
use anyhow::Result;
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
        let mut world = match self.adapter.create_test_world() {
            Ok(world) => world,
            Err(error) => {
                return TestResult::new(&spec.name).with_failure_reason(error.to_string());
            }
        };

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
                if let Err(error) = p.set_slot(*slot_name, Some(item)) {
                    result.success = false;
                    result.failure_reason = Some(error.to_string());
                    return result;
                }
            }

            // Set initial hotbar selection
            if let Err(error) = p.select_hotbar(player_config.selected_hotbar) {
                result.success = false;
                result.failure_reason = Some(error.to_string());
                return result;
            }

            // Set game mode
            if let Err(error) = p.set_game_mode(player_config.game_mode) {
                result.success = false;
                result.failure_reason = Some(error.to_string());
                return result;
            }
        }

        // Execute timeline tick by tick
        for tick in 0..=timeline.max_tick {
            // Execute actions for this tick
            if let Some(actions) = timeline.timeline.get(&tick) {
                for (_test_idx, entry, _value_idx) in actions.iter() {
                    match execute_action(&mut *world, &mut player, &entry.action_type, tick) {
                        Err(error) => {
                            result.success = false;
                            result.failure_reason = Some(error.to_string());
                            result.total_ticks = tick;
                            result.execution_time_ms = start_time.elapsed().as_millis() as u64;
                            return result;
                        }
                        Ok(ActionOutcome::Action) => {}
                        Ok(ActionOutcome::AssertPassed) => {
                            result.add_assertion(AssertionResult::Success(tick));
                        }
                        Ok(ActionOutcome::AssertFailed(fail)) => {
                            result.add_assertion(AssertionResult::Failure(fail));
                            result.success = false;
                            result.total_ticks = tick;
                            result.execution_time_ms = start_time.elapsed().as_millis() as u64;
                            return result;
                        }
                    }
                }
            }

            // Advance game tick
            if let Err(error) = world.do_tick() {
                result.success = false;
                result.failure_reason = Some(error.to_string());
                result.total_ticks = tick;
                result.execution_time_ms = start_time.elapsed().as_millis() as u64;
                return result;
            }
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
}

/// Execute one test action against any Flint world adapter.
pub fn execute_action(
    world: &mut dyn FlintWorld,
    player: &mut Option<Box<dyn FlintPlayer>>,
    action: &ActionType,
    tick: u32,
) -> Result<ActionOutcome> {
    match action {
        ActionType::Place { pos, block } => {
            let pos = [pos[0], pos[1], pos[2]];
            world.set_block(pos, block)?;
            Ok(ActionOutcome::Action)
        }

        ActionType::PlaceEach { blocks } => {
            for placement in blocks {
                let pos = [placement.pos[0], placement.pos[1], placement.pos[2]];
                world.set_block(pos, &placement.block)?;
            }
            Ok(ActionOutcome::Action)
        }

        ActionType::Fill { region, with } => {
            world.fill(*region, with)?;
            Ok(ActionOutcome::Action)
        }

        ActionType::Remove { pos } => {
            let pos = [pos[0], pos[1], pos[2]];
            let air = Block {
                id: "minecraft:air".to_string(),
                properties: Default::default(),
                nbt: None,
            };
            world.set_block(pos, &air)?;
            Ok(ActionOutcome::Action)
        }

        ActionType::Summon {
            entity_alias,
            entity_type,
            pos,
            nbt,
        } => {
            world.summon_entity(entity_alias, entity_type, *pos, nbt.as_ref())?;
            Ok(ActionOutcome::Action)
        }

        ActionType::Assert { checks } => {
            for check in checks {
                match check {
                    AssertType::Block(block) => {
                        let pos = [block.pos[0], block.pos[1], block.pos[2]];
                        let expected_blocks = block.is.to_vec();
                        let requested_nbt = expected_blocks
                            .iter()
                            .filter_map(|block| block.nbt.as_ref())
                            .flat_map(EntityNbt::requested_paths)
                            .collect::<Vec<_>>();
                        let actual = world.get_block(pos, &requested_nbt)?;

                        if !expected_blocks
                            .iter()
                            .any(|expected| block_matches(&actual, expected))
                        {
                            return Ok(ActionOutcome::AssertFailed(AssertFailure::new_block(
                                tick,
                                expected_blocks,
                                actual,
                                pos,
                            )));
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
                        let actual = p.get_slot(inv.slot, data)?.unwrap_or(Item::empty());
                        let expected = inv.is.clone().unwrap_or(Item::empty());
                        if !item_matches(&actual, &expected) {
                            return Ok(ActionOutcome::AssertFailed(AssertFailure::new_item(
                                tick, &expected, &actual, inv.slot,
                            )));
                        }
                    }
                    AssertType::Time(time) => {
                        let actual = world.get_time()?;
                        if actual != time.time {
                            return Ok(ActionOutcome::AssertFailed(
                                AssertTimeFail::new(tick, time.time, actual).into(),
                            ));
                        }
                    }
                    AssertType::Entity(entity) => {
                        let requested_nbt = entity.nbt.requested_paths();
                        let actual = if let Some(alias) = entity.entity_alias.as_deref() {
                            world.get_entity(alias, &requested_nbt)?
                        } else {
                            world.find_entity(
                                entity
                                    .entity_type
                                    .as_deref()
                                    .expect("entity check requires an alias or entity type"),
                                &requested_nbt,
                            )?
                        };
                        if !entity_matches(&actual, entity) {
                            return Ok(ActionOutcome::AssertFailed(
                                AssertEntityFail::new(tick, entity, &actual).into(),
                            ));
                        }
                    }
                    #[allow(unused)]
                    _ => {
                        println!("Unsupported assertion type: {:?}", check);
                    }
                }
            }
            Ok(ActionOutcome::AssertPassed)
        }

        ActionType::Tp {
            entity_alias,
            pos,
            rot,
        } => {
            if entity_alias == "player" {
                let p = player.get_or_insert_with(|| world.create_player());
                p.teleport(*pos, *rot)?;
            } else {
                world.teleport_entity(entity_alias, *pos, *rot)?;
            }
            Ok(ActionOutcome::Action)
        }

        ActionType::Interact { item } => {
            let p = player
                .as_mut()
                .expect("interact requires an existing player");
            if let Some(item_id) = item {
                let item = Item::new(item_id);
                p.set_slot(PlayerSlot::Hotbar1, Some(&item))?;
                p.select_hotbar(1)?;
            }
            p.interact()?;
            Ok(ActionOutcome::Action)
        }

        ActionType::SetSlot { slot, item, count } => {
            // Create player on demand if not already created
            let p = player.get_or_insert_with(|| world.create_player());
            if let Some(item_id) = item {
                let item = Item::with_count(item_id, *count);
                p.set_slot(*slot, Some(&item))?;
            } else {
                p.set_slot(*slot, None)?;
            }
            Ok(ActionOutcome::Action)
        }

        ActionType::SelectHotbar { slot } => {
            // Create player on demand if not already created
            let p = player.get_or_insert_with(|| world.create_player());
            p.select_hotbar(*slot)?;
            Ok(ActionOutcome::Action)
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
pub fn block_matches(actual: &Block, expected: &Block) -> bool {
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

    let Some(expected_nbt) = expected.nbt.as_ref() else {
        return true;
    };
    let Some(actual_nbt) = actual.nbt.as_ref() else {
        return false;
    };
    let actual_values = actual_nbt.expected_values();
    expected_nbt
        .expected_values()
        .into_iter()
        .all(|(key, expected)| {
            actual_values.get(&key).is_some_and(|actual| {
                normalize_entity_nbt_value(actual) == normalize_entity_nbt_value(&expected)
            })
        })
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

pub fn entity_matches(actual: &[EntityState], expected: &EntityCheck) -> bool {
    if !expected.exists {
        return actual.is_empty();
    }
    if let Some(expected_count) = expected.count
        && actual.len() != expected_count
    {
        return false;
    }
    if actual.is_empty() {
        return false;
    }
    actual.iter().all(|actual| {
        if let Some(expected_type) = expected.entity_type.as_deref()
            && actual.entity_type.as_deref() != Some(expected_type)
        {
            return false;
        }
        if let Some(expected_pos) = expected.pos {
            let Some(actual_pos) = actual.pos else {
                return false;
            };
            let position_tolerance = expected.position_tolerance.unwrap_or(0.25);
            let distance = actual_pos
                .into_iter()
                .zip(expected_pos)
                .map(|(actual, expected)| (actual - expected).powi(2))
                .sum::<f64>()
                .sqrt();
            if distance > position_tolerance {
                return false;
            }
        }
        if let Some(expected_rot) = expected.rot {
            let Some(actual_rot) = actual.rot else {
                return false;
            };
            let rotation_tolerance = expected.rotation_tolerance.unwrap_or(0.5);
            let yaw_delta = (actual_rot[0] - expected_rot[0]).rem_euclid(360.0);
            let yaw_delta = yaw_delta.min(360.0 - yaw_delta);
            let pitch_delta = (actual_rot[1] - expected_rot[1]).abs();
            if yaw_delta > rotation_tolerance || pitch_delta > rotation_tolerance {
                return false;
            }
        }
        for (key, expected) in expected.nbt.expected_values() {
            let Some(actual) = actual.nbt.get(&key) else {
                return false;
            };
            if normalize_entity_nbt_value(actual) != normalize_entity_nbt_value(&expected) {
                return false;
            }
        }
        true
    })
}

/// Canonicalizes an NBT value string for comparison.
///
/// NBT has no boolean type: the vanilla SNBT parser turns the `true`/`false`
/// keywords into the bytes `1b`/`0b`, so that's what servers report where a
/// test spec writes `NoGravity: true`. Both keywords are mapped to their byte
/// form (after trimming whitespace and surrounding quotes); everything else,
/// including numeric type suffixes, is compared as written.
fn normalize_entity_nbt_value(value: &str) -> String {
    let value = value.trim().trim_matches('"').trim();
    if value.eq_ignore_ascii_case("true") {
        return "1b".to_string();
    }
    if value.eq_ignore_ascii_case("false") {
        return "0b".to_string();
    }
    value.to_string()
}

#[cfg(test)]
mod entity_match_tests {
    use super::*;
    use crate::test_spec::EntityCheck;

    fn check_with_rotation(rot: [f32; 2], tolerance: f32) -> EntityCheck {
        EntityCheck {
            entity_alias: Some("entity".to_string()),
            entity_type: None,
            exists: true,
            count: None,
            pos: None,
            position_tolerance: None,
            rot: Some(rot),
            rotation_tolerance: Some(tolerance),
            nbt: Default::default(),
        }
    }

    #[test]
    fn nbt_values_normalize_booleans() {
        // Spec-side booleans match the byte form the vanilla SNBT parser
        // produces and servers report.
        assert_eq!(
            normalize_entity_nbt_value("true"),
            normalize_entity_nbt_value("1b")
        );
        assert_eq!(
            normalize_entity_nbt_value("false"),
            normalize_entity_nbt_value("0b")
        );
        // Quoted strings still normalize.
        assert_eq!(normalize_entity_nbt_value("\"oak\""), "oak");
        // Guard: type suffixes must survive normalization unchanged.
        assert_ne!(
            normalize_entity_nbt_value("0.5f"),
            normalize_entity_nbt_value("0.5")
        );
    }

    #[test]
    fn entity_nbt_boolean_matches_byte_report() {
        use crate::test_spec::EntityNbt;

        let expected = EntityCheck {
            entity_alias: Some("entity".to_string()),
            entity_type: None,
            exists: true,
            count: None,
            pos: None,
            position_tolerance: None,
            rot: None,
            rotation_tolerance: None,
            nbt: EntityNbt::from_string_values([("NoGravity".to_string(), "true".to_string())]),
        };

        let actual = vec![EntityState {
            nbt: [("NoGravity".to_string(), "1b".to_string())]
                .into_iter()
                .collect(),
            ..EntityState::default()
        }];

        assert!(entity_matches(&actual, &expected));
    }

    #[test]
    fn yaw_comparison_wraps_at_180_degrees() {
        let actual = vec![EntityState {
            rot: Some([-179.0, 0.0]),
            ..EntityState::default()
        }];

        assert!(entity_matches(
            &actual,
            &check_with_rotation([179.0, 0.0], 2.0)
        ));
    }

    #[test]
    fn entity_count_must_match_when_requested() {
        let expected: EntityCheck =
            serde_json::from_str(r#"{"is":"minecraft:snowball","count":1}"#).unwrap();
        let matching = vec![EntityState {
            entity_type: Some("minecraft:snowball".to_string()),
            ..EntityState::default()
        }];
        let too_many = vec![matching[0].clone(), matching[0].clone()];

        assert!(entity_matches(&matching, &expected));
        assert!(!entity_matches(&too_many, &expected));
    }

    #[test]
    fn every_entity_must_match_requested_state() {
        let expected: EntityCheck = serde_json::from_str(
            r#"{"is":"minecraft:snowball","count":2,"pos":[0.0,64.0,0.0],"position_tolerance":1.0}"#,
        )
        .unwrap();
        let matching = EntityState {
            entity_type: Some("minecraft:snowball".to_string()),
            pos: Some([0.0, 64.0, 0.0]),
            ..EntityState::default()
        };
        let outside_tolerance = EntityState {
            pos: Some([3.0, 64.0, 0.0]),
            ..matching.clone()
        };

        assert!(!entity_matches(&[matching, outside_tolerance], &expected));
    }
}
