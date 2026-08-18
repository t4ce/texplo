#[cfg(any(target_os = "trueos", target_os = "zkvm"))]
pub use trueos::platform::path::{Path, PathBuf};

#[cfg(not(any(target_os = "trueos", target_os = "zkvm")))]
pub use std::path::{Path, PathBuf};
