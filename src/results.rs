use crate::format;
use crate::results::AssertionResult::Failure;
use crate::test_spec::Block;
use serde::{Deserialize, Serialize};
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
}

impl InfoType {
    pub fn get_string(&self) -> Option<String> {
        match self {
            InfoType::String(s) => Some(s.clone()),
            InfoType::Block(_) => None,
        }
    }
}
impl From<InfoType> for String {
    fn from(val: InfoType) -> String {
        match val {
            InfoType::String(s) => s.clone(),
            InfoType::Block(b) => b.to_command(),
        }
    }
}
impl From<&InfoType> for String {
    fn from(val: &InfoType) -> String {
        match val {
            InfoType::String(s) => s.clone(),
            InfoType::Block(b) => b.to_command(),
        }
    }
}

/// Result of executing a single assertion or action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssertionResult {
    Success(u32),
    Failure(AssertFailure),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertFailure {
    /// The tick at which this assertion was executed
    pub tick: u32,
    /// Error message if the assertion failed
    pub error_message: String,
    /// Position involved in the assertion, if applicable
    pub position: [i32; 3],
    /// Time taken to execute this assertion in milliseconds
    pub execution_time_ms: Option<u64>,
    /// What as expected
    pub expected: InfoType,
    /// What was found
    pub actual: InfoType,
}

impl AssertFailure {
    /// Create a failed assertion result
    pub fn new(
        tick: u32,
        error: impl Into<String>,
        position: [i32; 3],
        expected: InfoType,
        actual: InfoType,
    ) -> AssertFailure {
        Self {
            tick,
            error_message: error.into(),
            position,
            execution_time_ms: None,
            expected,
            actual,
        }
    }

    /// Add position information to the assertion result
    pub fn with_position(mut self, pos: [i32; 3]) -> Self {
        self.position = pos;
        self
    }

    /// Add execution timing information
    pub fn with_timing(mut self, ms: u64) -> Self {
        self.execution_time_ms = Some(ms);
        self
    }
    /// adds the expected and the actual found data
    pub fn with_expected_actual(mut self, expected: InfoType, actual: InfoType) -> Self {
        self.actual = actual;
        self.expected = expected;
        self
    }
}

/// Result of executing a complete test
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Name of the test
    pub test_name: String,

    /// Overall success status (true if all assertions passed)
    pub success: bool,

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
            assertions: Vec::new(),
            total_ticks: 0,
            execution_time_ms: 0,
            failure_reason: None,
            test_offset: None,
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

    /// Number of tests that passed
    pub passed_tests: usize,

    /// Number of tests that failed
    pub failed_tests: usize,

    /// Total execution time for all tests in milliseconds
    pub total_execution_time_ms: u64,
}

impl TestSummary {
    /// Create a test summary from a collection of test results
    pub fn from_results(results: Vec<TestResult>) -> Self {
        let total_tests = results.len();
        let passed_tests = results.iter().filter(|r| r.success).count();
        let failed_tests = total_tests - passed_tests;
        let total_execution_time_ms = results.iter().map(|r| r.execution_time_ms).sum();

        Self {
            results,
            total_tests,
            passed_tests,
            failed_tests,
            total_execution_time_ms,
        }
    }

    /// Get all failed tests
    pub fn failed_tests(&self) -> Vec<&TestResult> {
        self.results.iter().filter(|r| !r.success).collect()
    }

    /// Get all passed tests
    pub fn passed_tests(&self) -> Vec<&TestResult> {
        self.results.iter().filter(|r| r.success).collect()
    }

    /// Check if all tests passed
    pub fn all_passed(&self) -> bool {
        self.failed_tests == 0
    }

    /// Get success rate as a percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_tests == 0 {
            0.0
        } else {
            (self.passed_tests as f64 / self.total_tests as f64) * 100.0
        }
    }

    /// Get total execution time as Duration
    fn elapsed(&self) -> Duration {
        Duration::from_millis(self.total_execution_time_ms)
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_success(tick: u32) -> AssertionResult {
        AssertionResult::Success(tick)
    }

    fn make_failure(tick: u32, error: &str, position: [i32; 3]) -> AssertionResult {
        Failure(AssertFailure::new(
            tick,
            error,
            position,
            InfoType::String("expected".to_string()),
            InfoType::String("actual".to_string()),
        ))
    }

    #[test]
    fn test_assertion_result_success() {
        let result = make_success(5);
        assert!(matches!(result, AssertionResult::Success(5)));
    }

    #[test]
    fn test_assertion_result_failure() {
        let result = make_failure(10, "Block mismatch", [5, 6, 7]);

        if let Failure(f) = result {
            assert_eq!(f.tick, 10);
            assert_eq!(f.error_message, "Block mismatch");
            assert_eq!(f.position, [5, 6, 7]);
        } else {
            panic!("Expected Failure variant");
        }
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
        result.add_assertion(make_failure(10, "Expected stone, got dirt", [0, 0, 0]));
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
        result2.add_assertion(make_failure(5, "Failed", [0, 0, 0]));

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
