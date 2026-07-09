//! Configuration values, file loading, CLI overrides, and validation.

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::Deserialize;

/// How to pin an action reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinMode {
    /// Write a full commit SHA and, when known, a human-readable tag comment.
    #[default]
    Sha,
    /// Write the selected release tag directly.
    Tag,
}

impl fmt::Display for PinMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sha => write!(f, "sha"),
            Self::Tag => write!(f, "tag"),
        }
    }
}

impl FromStr for PinMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "sha" => Ok(Self::Sha),
            "tag" => Ok(Self::Tag),
            _ => Err(format!(
                "unknown pin mode {s:?}; expected \"sha\" or \"tag\""
            )),
        }
    }
}

/// Which semver component to target when updating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateLevel {
    /// Allow updates across major versions.
    Major,
    /// Allow updates within the current major version.
    #[default]
    Minor,
    /// Allow updates within the current major and minor version.
    Patch,
}

impl fmt::Display for UpdateLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Major => write!(f, "major"),
            Self::Minor => write!(f, "minor"),
            Self::Patch => write!(f, "patch"),
        }
    }
}

impl FromStr for UpdateLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "major" => Ok(Self::Major),
            "minor" => Ok(Self::Minor),
            "patch" => Ok(Self::Patch),
            _ => Err(format!(
                "unknown update level {s:?}; expected \"major\", \"minor\", or \"patch\""
            )),
        }
    }
}

/// A duration with a unit suffix: `d` (days), `h` (hours), `m` (minutes).
///
/// Parsed from strings like `"7d"`, `"4h"`, `"30m"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelativeDuration {
    /// Number of duration units.
    pub amount: u64,
    /// Unit applied to [`Self::amount`].
    pub unit: DurationUnit,
}

/// Units accepted by [`RelativeDuration`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    /// Calendar-independent 24-hour periods.
    Days,
    /// Hours.
    Hours,
    /// Minutes.
    Minutes,
}

impl fmt::Display for RelativeDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suffix = match self.unit {
            DurationUnit::Days => 'd',
            DurationUnit::Hours => 'h',
            DurationUnit::Minutes => 'm',
        };
        write!(f, "{}{suffix}", self.amount)
    }
}

impl FromStr for RelativeDuration {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unit_char = s
            .chars()
            .last()
            .ok_or_else(|| "duration cannot be empty".to_string())?;

        let num_part = &s[..s.len() - unit_char.len_utf8()];

        let unit = match unit_char {
            'd' => DurationUnit::Days,
            'h' => DurationUnit::Hours,
            'm' => DurationUnit::Minutes,
            _ => {
                return Err(format!(
                    "unknown duration unit {unit_char:?} in {s:?}; expected 'd', 'h', or 'm'"
                ));
            }
        };

        let amount = num_part.parse::<u64>().map_err(|_| {
            format!("invalid duration amount in {s:?}: expected a positive integer before the unit")
        })?;

        Ok(Self { amount, unit })
    }
}

impl<'de> Deserialize<'de> for RelativeDuration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Output format for actioneer commands.
///
/// When absent, `update` launches the TUI. Set to `plain` or `json` to use
/// non-interactive output instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputMode {
    /// Human-readable non-interactive output.
    Plain,
    /// Machine-readable JSON output.
    Json,
}

impl fmt::Display for OutputMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plain => write!(f, "plain"),
            Self::Json => write!(f, "json"),
        }
    }
}

impl FromStr for OutputMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "plain" => Ok(Self::Plain),
            "json" => Ok(Self::Json),
            _ => Err(format!(
                "unknown output mode {s:?}; expected \"plain\" or \"json\""
            )),
        }
    }
}

/// Top-level configuration for actioneer.
///
/// All fields have sensible defaults so the config file is entirely optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActioneerConfig {
    /// Output pin representation used when applying updates.
    pub pin: PinMode,
    /// Largest semver component that may change during routine updates.
    pub update: UpdateLevel,
    /// Whether branch-pinned references are excluded from processing.
    pub skip_branches: bool,
    /// Minimum age required before a release is eligible.
    #[serde(rename = "min-release-age")]
    pub min_release_age: Option<RelativeDuration>,
    /// Whether network access is disabled in favor of cached data.
    pub offline: bool,
    /// Whether cache reads and writes are bypassed.
    pub no_cache: bool,
    /// Non-interactive output mode; `None` selects the update TUI.
    pub mode: Option<OutputMode>,
    /// Whether all planned changes should be applied.
    #[serde(default)]
    pub apply: bool,
    /// Whether apply results should be previewed without writing files.
    #[serde(default)]
    pub dry_run: bool,
}

impl ActioneerConfig {
    /// Apply CLI overrides on top of this config.
    ///
    /// Only fields present in `args` (i.e. `Some(...)`) override the current value.
    pub fn apply_overrides(&mut self, args: &crate::cli::ConfigArgs) {
        if let Some(pin) = args.pin {
            self.pin = pin;
        }
        if let Some(update) = args.update {
            self.update = update;
        }
        if let Some(skip_branches) = args.skip_branches {
            self.skip_branches = skip_branches;
        }
        if let Some(min_release_age) = args.min_release_age {
            self.min_release_age = Some(min_release_age);
        }
        if let Some(offline) = args.offline {
            self.offline = offline;
        }
        if let Some(no_cache) = args.no_cache {
            self.no_cache = no_cache;
        }
        if args.mode.is_some() {
            self.mode = args.mode;
        }
        if let Some(apply) = args.apply {
            self.apply = apply;
        }
        if let Some(dry_run) = args.dry_run {
            self.dry_run = dry_run;
        }
    }

    /// Validate the resolved configuration.
    ///
    /// Returns an error if any combination of settings is logically contradictory.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.offline && self.no_cache {
            return Err(ConfigError::Conflict(
                "offline and no_cache are mutually exclusive: offline mode relies on the cache, \
                 but no_cache disables it entirely"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

/// Errors that can occur when loading or validating the configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// The configuration file could not be read.
    Io(std::io::Error),
    /// The configuration file is not valid actioneer TOML.
    Parse(toml::de::Error),
    /// Two or more resolved settings cannot be used together.
    Conflict(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error reading config: {e}"),
            Self::Parse(e) => write!(f, "invalid config: {e}"),
            Self::Conflict(msg) => write!(f, "conflicting options: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Parse(e) => Some(e),
            Self::Conflict(_) => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self {
        Self::Parse(e)
    }
}

/// Look for `.github/actioneer.toml` directly under `root`.
///
/// Returns `Some(path)` when the file exists, `None` otherwise.
pub fn find_config(root: &Path) -> Option<PathBuf> {
    let candidate = root.join(".github").join("actioneer.toml");
    candidate.is_file().then_some(candidate)
}

/// Parse a config file at `path`.
pub fn load_config(path: &Path) -> Result<ActioneerConfig, ConfigError> {
    let contents = std::fs::read_to_string(path)?;
    let config = toml::from_str(&contents)?;
    Ok(config)
}

/// Locate and load the config from `root`, falling back to defaults if not found.
pub fn load(root: &Path) -> Result<ActioneerConfig, ConfigError> {
    match find_config(root) {
        Some(path) => load_config(&path),
        None => Ok(ActioneerConfig::default()),
    }
}
