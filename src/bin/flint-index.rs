use dotenvy::dotenv;
use flint_core::utils::get_test_path;
use std::path::Path;

fn main() {
    dotenv().ok();
    flint_core::loader::TestLoader::new(Path::new(&get_test_path()), true)
        .ok()
        .unwrap()
        .verify_and_rebuild_index()
        .expect("Verify wasn't successfull and rebuild also failed");
}
