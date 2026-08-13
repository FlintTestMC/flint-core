pub mod format;
pub mod index;
pub mod loader;
pub mod results;
pub mod runner;
pub mod spatial;
pub mod test_spec;
pub mod timeline;
pub mod timeline_validation;
pub mod traits;
pub mod utils;
pub mod viz_link;

use semver::VersionReq;
// Re-export main types for convenience
pub use runner::{TestRunConfig, TestRunner};
pub use traits::{BlockPos, FlintAdapter, FlintPlayer, FlintWorld, ServerInfo};

// Re-export flint-core types commonly used with this library
pub use crate::loader::TestLoader;
pub use crate::test_spec::{Block, Item, PlayerSlot, TestSpec, TestSpecLoadResult, WorldConfig};

pub fn get_supported_version() -> VersionReq {
    VersionReq::parse("<=1.0.0").unwrap()
}
