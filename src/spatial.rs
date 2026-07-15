use crate::test_spec::TestSpec;

/// Spatial utilities for test positioning and layout.
///
/// Offsets are computed from each test's cleanup region so parallel runs do not
/// overlap, then arranged in a square grid centered at the origin.
const DEFAULT_BATCH_PADDING: i32 = 8;
const FALLBACK_HALF_EXTENT: i32 = 16;

/// Calculate test offsets from cleanup regions with explicit padding between tests.
pub fn calculate_test_offsets_for_batch(tests: &[TestSpec], padding: i32) -> Vec<[i32; 3]> {
    let total_tests = tests.len();
    if total_tests == 0 {
        return Vec::new();
    }

    let cols = (total_tests as f64).sqrt().ceil() as usize;
    let rows = total_tests.div_ceil(cols);

    let mut left_extent = vec![i32::MAX; cols];
    let mut right_extent = vec![i32::MIN; cols];
    let mut back_extent = vec![i32::MAX; rows];
    let mut front_extent = vec![i32::MIN; rows];

    for (i, test) in tests.iter().enumerate().take(total_tests) {
        let col = i % cols;
        let row = i / cols;

        let (x_min, x_max, z_min, z_max) = if let Some(setup) = &test.setup
            && let Some(cleanup) = &setup.cleanup
        {
            let r = cleanup.region;
            let x_min = r[0][0].min(r[1][0]);
            let x_max = r[0][0].max(r[1][0]);
            let z_min = r[0][2].min(r[1][2]);
            let z_max = r[0][2].max(r[1][2]);
            (x_min, x_max, z_min, z_max)
        } else {
            (
                -FALLBACK_HALF_EXTENT,
                FALLBACK_HALF_EXTENT - 1,
                -FALLBACK_HALF_EXTENT,
                FALLBACK_HALF_EXTENT - 1,
            )
        };

        left_extent[col] = left_extent[col].min(x_min);
        right_extent[col] = right_extent[col].max(x_max);

        back_extent[row] = back_extent[row].min(z_min);
        front_extent[row] = front_extent[row].max(z_max);
    }

    let mut x_offsets = vec![0; cols];
    for c in 1..cols {
        x_offsets[c] = x_offsets[c - 1] + right_extent[c - 1] - left_extent[c] + padding;
    }

    let mut z_offsets = vec![0; rows];
    for r in 1..rows {
        z_offsets[r] = z_offsets[r - 1] + front_extent[r - 1] - back_extent[r] + padding;
    }

    let x_center = (left_extent[0] + x_offsets[cols - 1] + right_extent[cols - 1]) / 2;
    let z_center = (back_extent[0] + z_offsets[rows - 1] + front_extent[rows - 1]) / 2;

    let mut offsets = Vec::with_capacity(total_tests);
    for i in 0..total_tests {
        let col = i % cols;
        let row = i / cols;
        let ox = x_offsets[col] - x_center;
        let oz = z_offsets[row] - z_center;
        offsets.push([ox, 0, oz]);
    }
    offsets
}

/// Calculate test offsets using the default batch padding.
pub fn calculate_test_offsets_for_batch_default(tests: &[TestSpec]) -> Vec<[i32; 3]> {
    calculate_test_offsets_for_batch(tests, DEFAULT_BATCH_PADDING)
}

/// Pair loaded tests with their layout offsets.
pub fn pair_tests_with_offsets(tests: Vec<TestSpec>) -> Vec<(TestSpec, [i32; 3])> {
    let offsets = calculate_test_offsets_for_batch_default(&tests);
    tests.into_iter().zip(offsets).collect()
}

/// Apply an offset to a position
pub fn apply_offset(pos: [i32; 3], offset: [i32; 3]) -> [i32; 3] {
    [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]]
}

/// Apply an offset to a region (pair of positions)
pub fn apply_offset_to_region(region: [[i32; 3]; 2], offset: [i32; 3]) -> [[i32; 3]; 2] {
    [
        apply_offset(region[0], offset),
        apply_offset(region[1], offset),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_spec::{CleanupSpec, SetupSpec, TestSpec};

    fn test_spec(name: &str, region: [[i32; 3]; 2]) -> TestSpec {
        TestSpec {
            flint_version: None,
            name: name.to_string(),
            description: None,
            tags: vec![],
            minecraft_ids: vec![],
            dependencies: vec![],
            setup: Some(SetupSpec {
                cleanup: Some(CleanupSpec { region }),
                player: None,
                world: Default::default(),
            }),
            timeline: vec![],
            breakpoints: vec![],
        }
    }

    #[test]
    fn batch_offsets_space_tests_by_cleanup_region() {
        let test1 = test_spec("test1", [[-5, 0, -5], [5, 0, 5]]);
        let test2 = test_spec("test2", [[-2, 0, -2], [2, 0, 2]]);

        let offsets = calculate_test_offsets_for_batch(&[test1, test2], 4);
        assert_eq!(offsets.len(), 2);
        assert_eq!(offsets[0][0], -4);
        assert_eq!(offsets[1][0], 7);
    }

    #[test]
    fn pair_tests_with_offsets_preserves_order() {
        let tests = vec![
            test_spec("a", [[0, 0, 0], [1, 0, 1]]),
            test_spec("b", [[0, 0, 0], [1, 0, 1]]),
        ];
        let paired = pair_tests_with_offsets(tests);
        assert_eq!(paired.len(), 2);
        assert_eq!(paired[0].0.name, "a");
        assert_eq!(paired[1].0.name, "b");
    }

    #[test]
    fn single_test_is_centered_at_origin() {
        let offsets = calculate_test_offsets_for_batch_default(&[test_spec(
            "solo",
            [[-4, 0, -4], [4, 0, 4]],
        )]);
        assert_eq!(offsets, vec![[0, 0, 0]]);
    }

    #[test]
    fn test_apply_offset() {
        let pos = [1, 2, 3];
        let offset = [10, 20, 30];
        let result = apply_offset(pos, offset);
        assert_eq!(result, [11, 22, 33]);
    }

    #[test]
    fn test_apply_offset_negative() {
        let pos = [10, 20, 30];
        let offset = [-5, -10, -15];
        let result = apply_offset(pos, offset);
        assert_eq!(result, [5, 10, 15]);
    }

    #[test]
    fn test_apply_offset_to_region() {
        let region = [[0, 0, 0], [10, 10, 10]];
        let offset = [5, 0, -5];
        let result = apply_offset_to_region(region, offset);
        assert_eq!(result, [[5, 0, -5], [15, 10, 5]]);
    }
}
