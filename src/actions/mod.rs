pub mod reference;
pub mod resolution;
pub mod version;

pub use reference::{ActionReference, ActionUpdate};
pub use resolution::{PinStyle, ResolveConfig, Tag, UpdateMode, resolve};
pub use version::{Version, is_likely_sha, parse_version, sha_matches};
