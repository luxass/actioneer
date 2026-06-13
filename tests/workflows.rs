#[macro_use]
#[path = "support/mod.rs"]
mod support;

#[path = "support/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;

#[path = "workflows/discover.rs"]
mod discover;

#[path = "workflows/patch.rs"]
mod patch;
