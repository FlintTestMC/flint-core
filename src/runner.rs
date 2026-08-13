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

fn finish_with_error(
    mut result: TestResult,
    reason: impl Into<String>,
    total_ticks: u32,
    start_time: &Instant,
) -> TestResult {
    result.success = false;
    result.failure_reason = Some(reason.into());
    result.total_ticks = total_ticks;
    result.execution_time_ms = start_time.elapsed().as_millis() as u64;
    result
}

impl<A: FlintAdapter> TestRunner<A> {
    pub fn new(adapter: Arc<A>) -> Self {
        Self { adapter }
    }

    /// Run a single test
    pub fn run_test(&self, spec: &TestSpec) -> TestResult {
        let start_time = Instant::now();
        let mut result = TestResult::new(&spec.name);
        result.minecraft_ids = spec.minecraft_ids.clone();

        let mut world = match self.adapter.create_test_world() {
            Ok(world) => world,
            Err(error) => {
                return finish_with_error(result, error.to_string(), 0, &start_time);
            }
        };

        if let Some(setup) = &spec.setup
            && let Err(error) = world.configure_world(&setup.world)
        {
            return finish_with_error(result, error.to_string(), 0, &start_time);
        }

        let initial_tick = world.current_tick();
        if initial_tick != 0 {
            return finish_with_error(
                result,
                format!("new test world must start at tick 0, got {initial_tick}"),
                0,
                &start_time,
            );
        }

        // Build timeline for single test (no offset)
        let tests_with_offsets = vec![(spec.clone(), [0i32, 0, 0])];
        let timeline = TimelineAggregate::from_tests(&tests_with_offsets);

        // Player is created on demand when player actions are used
        let mut player: Option<Box<dyn FlintPlayer>> = None;

        // Initialize player from config if present (advanced mode)
        if let Some(setup) = &spec.setup
            && let Some(player_config) = setup.player.as_ref()
        {
            let p = match get_or_create_player(&mut *world, &mut player) {
                Ok(player) => player,
                Err(error) => {
                    return finish_with_error(result, error.to_string(), 0, &start_time);
                }
            };

            // Set initial inventory
            for (slot_name, item) in &player_config.inventory {
                if let Err(error) = p.set_slot(*slot_name, Some(item)) {
                    return finish_with_error(result, error.to_string(), 0, &start_time);
                }
            }

            // Set initial hotbar selection
            if let Err(error) = p.select_hotbar(player_config.selected_hotbar) {
                return finish_with_error(result, error.to_string(), 0, &start_time);
            }

            // Set game mode
            if let Err(error) = p.set_game_mode(player_config.game_mode) {
                return finish_with_error(result, error.to_string(), 0, &start_time);
            }
        }

        // Execute timeline tick by tick
        for tick in 0..=timeline.max_tick {
            let actual_tick = world.current_tick();
            if actual_tick != u64::from(tick) {
                return finish_with_error(
                    result,
                    format!("world tick mismatch before timeline tick {tick}: got {actual_tick}"),
                    tick,
                    &start_time,
                );
            }

            // Execute actions for this tick
            if let Some(actions) = timeline.timeline.get(&tick) {
                for (_test_idx, entry, _value_idx) in actions.iter() {
                    match execute_action(&mut *world, &mut player, &entry.action_type, tick) {
                        Err(error) => {
                            return finish_with_error(result, error.to_string(), tick, &start_time);
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

            // Timeline tick N observes exactly N completed game ticks. There is
            // no reason to advance the world after the final timeline entry.
            if tick == timeline.max_tick {
                break;
            }

            if let Err(error) = world.do_tick() {
                return finish_with_error(result, error.to_string(), tick, &start_time);
            }

            let expected_tick = u64::from(tick) + 1;
            let actual_tick = world.current_tick();
            if actual_tick != expected_tick {
                return finish_with_error(
                    result,
                    format!(
                        "world must advance exactly one tick: expected {expected_tick}, got {actual_tick}"
                    ),
                    tick,
                    &start_time,
                );
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

fn get_or_create_player<'a>(
    world: &mut dyn FlintWorld,
    player: &'a mut Option<Box<dyn FlintPlayer>>,
) -> Result<&'a mut Box<dyn FlintPlayer>> {
    if player.is_none() {
        player.replace(world.create_player()?);
    }

    player
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("player creation returned no player"))
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
                        let p = get_or_create_player(world, player)?;
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
                let p = get_or_create_player(world, player)?;
                p.teleport(*pos, *rot)?;
            } else {
                world.teleport_entity(entity_alias, *pos, *rot)?;
            }
            Ok(ActionOutcome::Action)
        }

        ActionType::Interact { item } => {
            let p = get_or_create_player(world, player)?;
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
            let p = get_or_create_player(world, player)?;
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
            let p = get_or_create_player(world, player)?;
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

#[cfg(test)]
mod runner_contract_tests {
    use super::*;
    use crate::test_spec::{
        BlockCheck, BlockSpec, CleanupSpec, GameMode, SetupSpec, TickSpec, TimelineEntry,
        WorldConfig,
    };
    use crate::traits::{BlockPos, ServerInfo};
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct ObservedState {
        configured: Vec<WorldConfig>,
        ticks: u64,
        players_created: usize,
        interactions: usize,
    }

    struct MockAdapter {
        observed: Arc<Mutex<ObservedState>>,
        tick_step: u64,
        fail_player_creation: bool,
    }

    impl MockAdapter {
        fn new(tick_step: u64) -> Self {
            Self {
                observed: Arc::new(Mutex::new(ObservedState::default())),
                tick_step,
                fail_player_creation: false,
            }
        }

        fn with_player_creation_failure(mut self) -> Self {
            self.fail_player_creation = true;
            self
        }
    }

    impl FlintAdapter for MockAdapter {
        fn create_test_world(&self) -> Result<Box<dyn FlintWorld>> {
            Ok(Box::new(MockWorld {
                observed: Arc::clone(&self.observed),
                tick_step: self.tick_step,
                fail_player_creation: self.fail_player_creation,
                blocks: HashMap::new(),
            }))
        }

        fn server_info(&self) -> ServerInfo {
            ServerInfo {
                minecraft_version: "test".to_string(),
            }
        }
    }

    struct MockWorld {
        observed: Arc<Mutex<ObservedState>>,
        tick_step: u64,
        fail_player_creation: bool,
        blocks: HashMap<BlockPos, Block>,
    }

    impl FlintWorld for MockWorld {
        fn configure_world(&mut self, config: &WorldConfig) -> Result<()> {
            self.observed
                .lock()
                .unwrap()
                .configured
                .push(config.clone());
            Ok(())
        }

        fn do_tick(&mut self) -> Result<()> {
            self.observed.lock().unwrap().ticks += self.tick_step;
            Ok(())
        }

        fn current_tick(&self) -> u64 {
            self.observed.lock().unwrap().ticks
        }

        fn get_time(&self) -> Result<u64> {
            Ok(1000)
        }

        fn get_block(&self, pos: BlockPos, _requested_nbt: &[String]) -> Result<Block> {
            Ok(self
                .blocks
                .get(&pos)
                .cloned()
                .unwrap_or_else(|| Block::new("minecraft:air")))
        }

        fn set_block(&mut self, pos: BlockPos, block: &Block) -> Result<()> {
            self.blocks.insert(pos, block.clone());
            Ok(())
        }

        fn create_player(&mut self) -> Result<Box<dyn FlintPlayer>> {
            if self.fail_player_creation {
                anyhow::bail!("player attachment failed");
            }
            self.observed.lock().unwrap().players_created += 1;
            Ok(Box::new(MockPlayer {
                observed: Arc::clone(&self.observed),
                selected_hotbar: 1,
            }))
        }
    }

    struct MockPlayer {
        observed: Arc<Mutex<ObservedState>>,
        selected_hotbar: u8,
    }

    impl FlintPlayer for MockPlayer {
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn set_slot(&mut self, _slot: PlayerSlot, _item: Option<&Item>) -> Result<()> {
            Ok(())
        }

        fn get_slot(
            &mut self,
            _slot: PlayerSlot,
            _requested_data: Vec<String>,
        ) -> Result<Option<Item>> {
            Ok(None)
        }

        fn select_hotbar(&mut self, slot: u8) -> Result<()> {
            self.selected_hotbar = slot;
            Ok(())
        }

        fn selected_hotbar(&self) -> u8 {
            self.selected_hotbar
        }

        fn teleport(&mut self, _pos: [f64; 3], _rot: Option<[f32; 2]>) -> Result<()> {
            Ok(())
        }

        fn interact(&mut self) -> Result<()> {
            self.observed.lock().unwrap().interactions += 1;
            Ok(())
        }

        fn set_game_mode(&mut self, _mode: GameMode) -> Result<()> {
            Ok(())
        }
    }

    fn test_spec(name: &str, world: WorldConfig, timeline: Vec<TimelineEntry>) -> TestSpec {
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
                world,
            }),
            timeline,
            breakpoints: vec![],
        }
    }

    fn place(at: u32, block: &str) -> TimelineEntry {
        TimelineEntry {
            at: TickSpec::Single(at),
            action_type: ActionType::Place {
                pos: [0, 0, 0],
                block: Block::new(block),
            },
        }
    }

    fn assert_block(at: u32, block: &str) -> TimelineEntry {
        TimelineEntry {
            at: TickSpec::Single(at),
            action_type: ActionType::Assert {
                checks: vec![AssertType::Block(BlockCheck {
                    pos: [0, 0, 0],
                    is: BlockSpec::Single(Block::new(block)),
                })],
            },
        }
    }

    #[test]
    fn positive_fixture_configures_world_and_observes_exact_tick_one() {
        let adapter = Arc::new(MockAdapter::new(1));
        let world_config = WorldConfig {
            time: "minecraft:noon".to_string(),
            ..WorldConfig::default()
        };
        let spec = test_spec(
            "positive",
            world_config.clone(),
            vec![
                place(0, "minecraft:stone"),
                assert_block(1, "minecraft:stone"),
            ],
        );

        let result = TestRunner::new(Arc::clone(&adapter)).run_test(&spec);

        assert!(result.success, "{:?}", result.failure_reason);
        assert_eq!(result.total_ticks, 1);
        let observed = adapter.observed.lock().unwrap();
        assert_eq!(observed.configured, vec![world_config]);
        assert_eq!(observed.ticks, 1);
    }

    #[test]
    fn tick_zero_fixture_does_not_advance_the_world() {
        let adapter = Arc::new(MockAdapter::new(1));
        let spec = test_spec(
            "tick zero",
            WorldConfig::default(),
            vec![assert_block(0, "minecraft:air")],
        );

        let result = TestRunner::new(Arc::clone(&adapter)).run_test(&spec);

        assert!(result.success, "{:?}", result.failure_reason);
        assert_eq!(result.total_ticks, 0);
        assert_eq!(adapter.observed.lock().unwrap().ticks, 0);
    }

    #[test]
    fn deliberately_wrong_assertion_returns_a_red_result() {
        let adapter = Arc::new(MockAdapter::new(1));
        let spec = test_spec(
            "negative control",
            WorldConfig::default(),
            vec![
                place(0, "minecraft:stone"),
                assert_block(1, "minecraft:dirt"),
            ],
        );

        let result = TestRunner::new(Arc::clone(&adapter)).run_test(&spec);

        assert!(!result.success);
        assert_eq!(result.total_ticks, 1);
        assert!(matches!(
            result.assertions.as_slice(),
            [AssertionResult::Failure(AssertFailure::Block(_))]
        ));
    }

    #[test]
    fn runner_rejects_a_world_that_does_not_advance_exactly_one_tick() {
        let adapter = Arc::new(MockAdapter::new(0));
        let spec = test_spec(
            "stalled clock",
            WorldConfig::default(),
            vec![assert_block(1, "minecraft:air")],
        );

        let result = TestRunner::new(adapter).run_test(&spec);

        assert!(!result.success);
        assert!(
            result
                .failure_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("advance exactly one tick"))
        );
    }

    #[test]
    fn interact_creates_a_player_on_demand() {
        let adapter = Arc::new(MockAdapter::new(1));
        let spec = test_spec(
            "interact",
            WorldConfig::default(),
            vec![TimelineEntry {
                at: TickSpec::Single(0),
                action_type: ActionType::Interact { item: None },
            }],
        );

        let result = TestRunner::new(Arc::clone(&adapter)).run_test(&spec);

        assert!(result.success, "{:?}", result.failure_reason);
        let observed = adapter.observed.lock().unwrap();
        assert_eq!(observed.players_created, 1);
        assert_eq!(observed.interactions, 1);
    }

    #[test]
    fn player_creation_failure_becomes_a_red_result() {
        let adapter = Arc::new(MockAdapter::new(1).with_player_creation_failure());
        let spec = test_spec(
            "player attachment failure",
            WorldConfig::default(),
            vec![TimelineEntry {
                at: TickSpec::Single(0),
                action_type: ActionType::Interact { item: None },
            }],
        );

        let result = TestRunner::new(adapter).run_test(&spec);

        assert!(!result.success);
        assert_eq!(result.total_ticks, 0);
        assert!(
            result
                .failure_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("player attachment failed"))
        );
    }

    #[test]
    fn unsupported_entity_action_fails_instead_of_silently_passing() {
        let adapter = Arc::new(MockAdapter::new(1));
        let spec = test_spec(
            "unsupported entity",
            WorldConfig::default(),
            vec![TimelineEntry {
                at: TickSpec::Single(0),
                action_type: ActionType::Summon {
                    entity_alias: "test".to_string(),
                    entity_type: "minecraft:pig".to_string(),
                    pos: [0.0, 0.0, 0.0],
                    nbt: None,
                },
            }],
        );

        let result = TestRunner::new(adapter).run_test(&spec);

        assert!(!result.success);
        assert!(
            result
                .failure_reason
                .as_deref()
                .is_some_and(|reason| reason.contains("does not support summoning entities"))
        );
    }
}
