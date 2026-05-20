use clap::ValueEnum;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum UpdateMode {
    #[default]
    Major,
    Minor,
    Patch,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PinStyle {
    #[default]
    Sha,
    Tag,
}

/// Controls how GitHub references are turned into proposed updates.
#[derive(Clone, Debug, Default)]
pub struct ResolveOptions {
    /// Substring filters applied to the rendered action name, e.g. `actions/cache`.
    pub excludes: Vec<String>,
    /// Whether non-version symbolic refs like `main` should be considered updatable.
    pub include_branches: bool,
    /// How far updates are allowed to move from the current version.
    pub mode: UpdateMode,
    /// Whether rewritten refs should be pinned to the resolved SHA.
    pub style: PinStyle,
}
