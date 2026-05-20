pub mod reference;
pub mod resolve;
pub mod resolved_update;

pub use reference::{ActionName, ByteSpan, Reference, ReferenceKind, Repository, SourceLocation};
pub use resolve::{PinStyle, ResolveOptions, UpdateMode};
pub use resolved_update::{ResolvedUpdate, UpdateSource, UpdateTarget, ValidationState};
