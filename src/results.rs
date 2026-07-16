use crate::results::AssertionResult::Failure;
use crate::test_spec::{Block, EntityCheck};
use crate::traits::EntityState;
use crate::{Item, PlayerSlot, format};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::time::Duration;

/// Outcome of executing a single action
pub enum ActionOutcome {
    /// Non-assertion action completed (place, fill, remove)
    Action,
    /// Assertion passed
    AssertPassed,
    /// Assertion failed with details
    AssertFailed(AssertFailure),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InfoType {
    String(String),
    Block(Block),
    Blocks(Vec<Block>),
    Item(Item),
    Slot(PlayerSlot),
    EntityCheck(Box<EntityCheck>),
    EntityState(Box<Vec<EntityState>>),
}

impl InfoType {
    pub fn get_string(&self) -> Option<String> {
        match self {
            InfoType::String(s) => Some(s.clone()),
            InfoType::Block(_)
            | InfoType::Blocks(_)
            | InfoType::Item(_)
            | InfoType::Slot(_)
            | InfoType::EntityCheck(_)
            | InfoType::EntityState(_) => None,
        }
    }
    fn type_string_generator(val: &InfoType) -> String {
        match val {
            InfoType::String(s) => s.clone(),
            InfoType::Block(b) => b.to_command(),
            InfoType::Blocks(blocks) => blocks
                .iter()
                .map(|b| b.to_command())
                .collect::<Vec<_>>()
                .join(" or "),
            InfoType::Slot(slot) => slot.to_string(),
            InfoType::Item(item) => item.to_command(),
            InfoType::EntityCheck(entity) => format!("{entity:?}"),
            InfoType::EntityState(entity) => format!("{entity:?}"),
        }
    }
}
impl From<InfoType> for String {
    fn from(val: InfoType) -> String {
        InfoType::type_string_generator(&val)
    }
}
impl From<&InfoType> for String {
    fn from(val: &InfoType) -> String {
        InfoType::type_string_generator(val)
    }
}

/// Result of executing a single assertion or action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssertionResult {
    Success(u32),
    Failure(AssertFailure),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssertFailure {
    Block(AssertBlockFail),
    Inventory(AssertInventoryFail),
    Time(AssertTimeFail),
    Entity(Box<AssertEntityFail>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertBlockFail {
    pub tick: u32,
    pub expected: Vec<Block>,
    pub actual: Block,
    pub position: [i32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertInventoryFail {
    pub tick: u32,
    pub expected: Option<Item>,
    pub actual: Option<Item>,
    pub slot: PlayerSlot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertEntityFail {
    pub tick: u32,
    pub expected: EntityCheck,
    pub actual: Vec<EntityState>,
}

impl AssertEntityFail {
    pub fn new(tick: u32, expected: &EntityCheck, actual: &[EntityState]) -> Self {
        Self {
            tick,
            expected: expected.clone(),
            actual: actual.to_vec(),
        }
    }
}

impl From<AssertEntityFail> for AssertFailure {
    fn from(failure: AssertEntityFail) -> Self {
        Self::Entity(Box::new(failure))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AssertTimeFail {
    pub tick: u32,
    pub expected: u64,
    pub actual: u64,
}

impl AssertTimeFail {
    pub fn new(tick: u32, expected: u64, actual: u64) -> Self {
        Self {
            tick,
            expected,
            actual,
        }
    }
}

impl From<AssertTimeFail> for AssertFailure {
    fn from(failure: AssertTimeFail) -> Self {
        Self::Time(failure)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum AssertPosition {
    Coordinate { x: i32, y: i32, z: i32 },
    Slot { slot: PlayerSlot },
}

impl AssertPosition {
    pub fn from_array(array: [i32; 3]) -> Self {
        Self::Coordinate {
            x: array[0],
            y: array[1],
            z: array[2],
        }
    }
    pub fn from_slot(slot: PlayerSlot) -> Self {
        Self::Slot { slot }
    }
}
impl Display for AssertPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Coordinate { x, y, z } => write!(f, "({},{},{})", x, y, z),
            Self::Slot { slot } => write!(f, "{}", slot),
        }
    }
}

impl AssertFailure {
    pub fn new_item(tick: u32, expected: &Item, actual: &Item, slot: PlayerSlot) -> AssertFailure {
        Self::Inventory(AssertInventoryFail {
            tick,
            expected: Some(expected.clone()),
            actual: Some(actual.clone()),
            slot,
        })
    }

    pub fn new_block(tick: u32, expected: Vec<Block>, actual: Block, position: [i32; 3]) -> Self {
        Self::Block(AssertBlockFail {
            tick,
            expected,
            actual,
            position,
        })
    }

    pub fn new_inventory(
        tick: u32,
        expected: Option<Item>,
        actual: Option<Item>,
        slot: PlayerSlot,
    ) -> Self {
        Self::Inventory(AssertInventoryFail {
            tick,
            expected,
            actual,
            slot,
        })
    }

    pub fn tick(&self) -> u32 {
        match self {
            Self::Block(failure) => failure.tick,
            Self::Inventory(failure) => failure.tick,
            Self::Time(failure) => failure.tick,
            Self::Entity(failure) => failure.tick,
        }
    }

    pub fn expected(&self) -> InfoType {
        match self {
            Self::Block(failure) => InfoType::Blocks(failure.expected.clone()),
            Self::Inventory(failure) => failure
                .expected
                .clone()
                .map(InfoType::Item)
                .unwrap_or_else(|| InfoType::String("empty".to_string())),
            Self::Time(failure) => InfoType::String(failure.expected.to_string()),
            Self::Entity(failure) => InfoType::EntityCheck(Box::new(failure.expected.clone())),
        }
    }

    pub fn actual(&self) -> InfoType {
        match self {
            Self::Block(failure) => InfoType::Block(failure.actual.clone()),
            Self::Inventory(failure) => failure
                .actual
                .clone()
                .map(InfoType::Item)
                .unwrap_or_else(|| InfoType::String("empty".to_string())),
            Self::Time(failure) => InfoType::String(failure.actual.to_string()),
            Self::Entity(failure) => InfoType::EntityState(Box::new(failure.actual.clone())),
        }
    }

    pub fn position(&self) -> AssertPosition {
        match self {
            Self::Block(failure) => AssertPosition::from_array(failure.position),
            Self::Inventory(failure) => AssertPosition::from_slot(failure.slot),
            Self::Time(_) => AssertPosition::from_array([0, 0, 0]),
            Self::Entity(failure) => AssertPosition::from_array(
                failure
                    .expected
                    .pos
                    .map(|pos| {
                        [
                            pos[0].floor() as i32,
                            pos[1].floor() as i32,
                            pos[2].floor() as i32,
                        ]
                    })
                    .unwrap_or([0, 0, 0]),
            ),
        }
    }

    pub fn error_message(&self) -> String {
        match self {
            Self::Block(_) => "Block was different".to_string(),
            Self::Inventory(_) => "Inventory slot content was different".to_string(),
            Self::Time(failure) => format!(
                "Time mismatch: expected {}, got {}",
                failure.expected, failure.actual
            ),
            Self::Entity(failure) => format!(
                "Entity mismatch for {}",
                failure
                    .expected
                    .entity_alias
                    .as_deref()
                    .or(failure.expected.entity_type.as_deref())
                    .unwrap_or("unknown entity")
            ),
        }
    }
}

/// Result of executing a complete test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Name of the test
    pub test_name: String,

    // Minecraft block_id, item_id or other ids of the targets in this test
    pub minecraft_ids: Vec<String>,

    /// Overall success status (true if all assertions passed)
    pub success: bool,

    /// Whether the test was skipped (e.g. due to version mismatch)
    pub skipped: bool,

    /// Reason for skipping, if applicable
    pub skip_reason: Option<String>,

    /// Individual assertion results
    pub assertions: Vec<AssertionResult>,

    /// Total number of ticks executed
    pub total_ticks: u32,

    /// Total execution time in milliseconds
    pub execution_time_ms: u64,

    /// Reason for test failure, if applicable
    pub failure_reason: Option<String>,

    /// Test offset used for spatial positioning
    pub test_offset: Option<[i32; 3]>,
}

impl TestResult {
    /// Create a new test result
    pub fn new(test_name: impl Into<String>) -> Self {
        Self {
            test_name: test_name.into(),
            success: true,
            skipped: false,
            skip_reason: None,
            assertions: Vec::new(),
            total_ticks: 0,
            execution_time_ms: 0,
            failure_reason: None,
            test_offset: None,
            minecraft_ids: Vec::new(),
        }
    }

    /// Create a skipped test result
    pub fn skipped(test_name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            test_name: test_name.into(),
            success: true,
            skipped: true,
            skip_reason: Some(reason.into()),
            assertions: Vec::new(),
            total_ticks: 0,
            execution_time_ms: 0,
            failure_reason: None,
            test_offset: None,
            minecraft_ids: Vec::new(),
        }
    }

    /// Add an assertion result to this test result
    pub fn add_assertion(&mut self, assertion: AssertionResult) {
        if let Failure(_) = assertion {
            self.success = false;
        }
        self.assertions.push(assertion);
    }

    /// Set the total number of ticks executed
    pub fn with_total_ticks(mut self, ticks: u32) -> Self {
        self.total_ticks = ticks;
        self
    }

    /// Set the total execution time
    pub fn with_execution_time(mut self, ms: u64) -> Self {
        self.execution_time_ms = ms;
        self
    }

    /// Set the test offset
    pub fn with_offset(mut self, offset: [i32; 3]) -> Self {
        self.test_offset = Some(offset);
        self
    }

    /// Set a custom failure reason
    pub fn with_failure_reason(mut self, reason: impl Into<String>) -> Self {
        self.success = false;
        self.failure_reason = Some(reason.into());
        self
    }

    /// Get the number of passed assertions
    pub fn passed_count(&self) -> usize {
        self.assertions
            .iter()
            .filter(|a| matches!(a, AssertionResult::Success(_)))
            .count()
    }

    /// Get the number of failed assertions
    pub fn failed_count(&self) -> usize {
        self.assertions
            .iter()
            .filter(|a| !matches!(a, AssertionResult::Success(_)))
            .count()
    }

    /// Get the total number of assertions
    pub fn total_assertions(&self) -> usize {
        self.assertions.len()
    }
}

/// Summary of multiple test results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSummary {
    /// All test results
    pub results: Vec<TestResult>,

    /// Total number of tests
    pub total_tests: usize,

    /// Number of tests that passed (excludes skipped)
    pub passed_tests: usize,

    /// Number of tests that failed
    pub failed_tests: usize,

    /// Number of tests that were skipped
    pub skipped_tests: usize,

    /// Total execution time for all tests in milliseconds
    pub total_execution_time_ms: u64,
}

impl TestSummary {
    /// Create a test summary from a collection of test results
    pub fn from_results(results: Vec<TestResult>) -> Self {
        let total_tests = results.len();
        let skipped_tests = results.iter().filter(|r| r.skipped).count();
        let failed_tests = results.iter().filter(|r| !r.success && !r.skipped).count();
        let passed_tests = total_tests - skipped_tests - failed_tests;
        let total_execution_time_ms = results.iter().map(|r| r.execution_time_ms).sum();

        Self {
            results,
            total_tests,
            passed_tests,
            failed_tests,
            skipped_tests,
            total_execution_time_ms,
        }
    }

    /// Get all failed tests
    pub fn failed_tests(&self) -> Vec<&TestResult> {
        self.results
            .iter()
            .filter(|r| !r.success && !r.skipped)
            .collect()
    }

    /// Get all passed tests
    pub fn passed_tests(&self) -> Vec<&TestResult> {
        self.results
            .iter()
            .filter(|r| r.success && !r.skipped)
            .collect()
    }

    /// Check if all tests passed (skipped tests do not count as failures)
    pub fn all_passed(&self) -> bool {
        self.failed_tests == 0
    }

    /// Get success rate as a percentage (skipped tests excluded from denominator)
    pub fn success_rate(&self) -> f64 {
        let effective = self.total_tests - self.skipped_tests;
        if effective == 0 {
            0.0
        } else {
            (self.passed_tests as f64 / effective as f64) * 100.0
        }
    }

    /// Get total execution time as Duration
    fn elapsed(&self) -> Duration {
        Duration::from_millis(self.total_execution_time_ms)
    }

    /// Format concise summary as a plain string (no ANSI colors)
    pub fn format_concise_summary(&self) -> String {
        format::format_concise_summary(&self.results, self.elapsed())
    }

    /// Print concise summary (default mode)
    pub fn print_concise_summary(&self) {
        format::print_concise_summary(&self.results, self.elapsed());
    }

    /// Print verbose test summary (used in -v mode)
    pub fn print_test_summary(&self, separator_width: usize) {
        format::print_test_summary(&self.results, separator_width);
    }

    /// Print results in JUnit XML format
    pub fn print_junit(&self) {
        format::print_junit(&self.results, self.elapsed());
    }

    /// Print results in TAP (Test Anything Protocol) format
    pub fn print_tap(&self) {
        format::print_tap(&self.results);
    }

    /// Print results as JSON
    pub fn print_json(&self) {
        format::print_json(&self.results, self.elapsed());
    }
    /// returns the json. empty if true also tests without a minecraft id will be returned.
    pub fn create_ci_output(&self, empty: bool) -> String {
        format::create_ci_output(&self.results, empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_success(tick: u32) -> AssertionResult {
        AssertionResult::Success(tick)
    }

    fn make_failure(tick: u32, position: [i32; 3]) -> AssertionResult {
        Failure(AssertFailure::new_block(
            tick,
            vec![Block::new("minecraft:stone")],
            Block::new("minecraft:air"),
            position,
        ))
    }

    #[test]
    fn test_assertion_result_success() {
        let result = make_success(5);
        assert!(matches!(result, AssertionResult::Success(5)));
    }

    #[test]
    fn test_assertion_result_failure() {
        let result = make_failure(10, [5, 6, 7]);

        if let Failure(f) = result {
            assert_eq!(f.tick(), 10);
            assert_eq!(f.error_message(), "Block was different");
            assert_eq!(f.position(), AssertPosition::from_array([5, 6, 7]));
        } else {
            panic!("Expected Failure variant");
        }
    }

    #[test]
    fn time_failure_uses_typed_variant() {
        let failure: AssertFailure = AssertTimeFail::new(12, 1_000, 1_001).into();

        assert!(matches!(failure, AssertFailure::Time(_)));
        assert_eq!(failure.tick(), 12);
        assert_eq!(
            failure.error_message(),
            "Time mismatch: expected 1000, got 1001"
        );
        assert_eq!(failure.position(), AssertPosition::from_array([0, 0, 0]));
        assert_eq!(failure.expected().get_string().as_deref(), Some("1000"));
        assert_eq!(failure.actual().get_string().as_deref(), Some("1001"));
    }

    #[test]
    fn block_failure_uses_typed_variant() {
        let failure = AssertFailure::new_block(
            3,
            vec![Block::new("minecraft:stone")],
            Block::new("minecraft:air"),
            [4, 5, 6],
        );

        let AssertFailure::Block(block) = failure else {
            panic!("expected block failure variant");
        };
        assert_eq!(block.tick, 3);
        assert_eq!(block.position, [4, 5, 6]);
        assert_eq!(block.expected[0].id, "minecraft:stone");
        assert_eq!(block.actual.id, "minecraft:air");
    }

    #[test]
    fn test_test_result_all_pass() {
        let mut result = TestResult::new("test1")
            .with_total_ticks(20)
            .with_execution_time(5000)
            .with_offset([0, 0, 0]);

        result.add_assertion(make_success(5));
        result.add_assertion(make_success(10));

        assert!(result.success);
        assert_eq!(result.passed_count(), 2);
        assert_eq!(result.failed_count(), 0);
        assert_eq!(result.total_assertions(), 2);
        assert!(result.failure_reason.is_none());
    }

    #[test]
    fn test_test_result_with_failure() {
        let mut result = TestResult::new("test2");

        result.add_assertion(make_success(5));
        result.add_assertion(make_failure(10, [0, 0, 0]));
        result.add_assertion(make_success(15));

        assert!(!result.success);
        assert_eq!(result.passed_count(), 2);
        assert_eq!(result.failed_count(), 1);
        assert_eq!(result.total_assertions(), 3);
    }

    #[test]
    fn test_test_summary() {
        let result1 = TestResult::new("test1").with_execution_time(1000);
        let mut result2 = TestResult::new("test2").with_execution_time(2000);
        result2.add_assertion(make_failure(5, [0, 0, 0]));

        let summary = TestSummary::from_results(vec![result1, result2]);

        assert_eq!(summary.total_tests, 2);
        assert_eq!(summary.passed_tests, 1);
        assert_eq!(summary.failed_tests, 1);
        assert_eq!(summary.total_execution_time_ms, 3000);
        assert_eq!(summary.success_rate(), 50.0);
        assert!(!summary.all_passed());
    }

    #[test]
    fn test_test_summary_all_passed() {
        let result1 = TestResult::new("test1");
        let result2 = TestResult::new("test2");

        let summary = TestSummary::from_results(vec![result1, result2]);

        assert_eq!(summary.total_tests, 2);
        assert_eq!(summary.passed_tests, 2);
        assert_eq!(summary.failed_tests, 0);
        assert_eq!(summary.success_rate(), 100.0);
        assert!(summary.all_passed());
    }

    #[test]
    fn test_test_summary_empty() {
        let summary = TestSummary::from_results(vec![]);

        assert_eq!(summary.total_tests, 0);
        assert_eq!(summary.passed_tests, 0);
        assert_eq!(summary.failed_tests, 0);
        assert_eq!(summary.success_rate(), 0.0);
        assert!(summary.all_passed());
    }
}
