mod deps;
mod error;
mod precommit;

pub use deps::find_deps;
pub use error::Error;
pub use precommit::{is_typing_hook, update_config};

use std::path::Path;

/// Find dev deps from `cwd` and update typing hooks in `config_path`.
/// Returns `true` if the file was modified.
pub fn run(cwd: &Path, config_path: &Path) -> Result<bool, Error> {
    let deps = find_deps(cwd)?;
    update_config(config_path, &deps)
}
