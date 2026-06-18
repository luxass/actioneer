use std::{fs, path::Path};

use crate::{
    cli::{Mode, SharedArgs, UpdateArgs},
    discovery::ActionRef,
};

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub offline: bool,
    pub no_cache: bool,
    pub recursive: bool,
    pub filter: Vec<String>,
    pub exclude: Vec<String>,
    pub pin: Option<PinStyle>,
    pub update_level: Option<UpdateLevel>,
    pub skip_branches: bool,
    pub min_release_age: Option<String>,
    pub mode: Option<Mode>,
    policy_overrides: Vec<PolicyOverride>,
}

impl Config {
    pub fn apply_update_args(&mut self, args: &UpdateArgs) {
        if let Some(pin) = args.pin {
            self.pin = Some(pin);
        }
        if let Some(level) = args.update {
            self.update_level = Some(level);
        }
        if args.skip_branches {
            self.skip_branches = true;
        }
        if let Some(age) = &args.min_release_age {
            self.min_release_age = Some(age.clone());
        }
    }

    pub fn effective_pin(&self, action_ref: &ActionRef) -> PinStyle {
        let mut pin = self.pin.unwrap_or(PinStyle::Sha);

        for policy_override in &self.policy_overrides {
            if policy_override.matches(action_ref) {
                if let Some(next_pin) = policy_override.pin {
                    pin = next_pin;
                }
            }
        }

        pin
    }
}

#[derive(Debug, Clone)]
struct PolicyOverride {
    condition: Option<Expression>,
    pin: Option<PinStyle>,
}

impl PolicyOverride {
    fn matches(&self, action_ref: &ActionRef) -> bool {
        self.condition
            .as_ref()
            .is_none_or(|condition| condition.matches(action_ref))
    }
}

#[derive(Debug, Clone)]
struct PendingRule {
    name: Option<String>,
    condition: Option<Expression>,
    pin: Option<PinStyle>,
}

impl PendingRule {
    fn new() -> Self {
        Self {
            name: None,
            condition: None,
            pin: None,
        }
    }

    fn finish(self, path: &Path) -> Result<PolicyOverride, String> {
        let Some(condition) = self.condition else {
            let label = self
                .name
                .as_deref()
                .map(|name| format!(" rule {name:?}"))
                .unwrap_or_default();
            return Err(format!(
                "failed to parse config {}:{label} missing required when condition",
                path.display()
            ));
        };

        Ok(PolicyOverride {
            condition: Some(condition),
            pin: self.pin,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum PinStyle {
    Sha,
    Tag,
}

impl PinStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha => "sha",
            Self::Tag => "tag",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum UpdateLevel {
    #[default]
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Clone)]
enum Expression {
    And(Vec<Expression>),
    Or(Vec<Expression>),
    Compare {
        field: RuleField,
        op: RuleOperator,
        expected: String,
    },
}

impl Expression {
    fn matches(&self, action_ref: &ActionRef) -> bool {
        match self {
            Self::And(parts) => parts.iter().all(|part| part.matches(action_ref)),
            Self::Or(parts) => parts.iter().any(|part| part.matches(action_ref)),
            Self::Compare {
                field,
                op,
                expected,
            } => {
                let actual = field_value(action_ref, *field);
                match op {
                    RuleOperator::Equals => actual == *expected,
                    RuleOperator::NotEquals => actual != *expected,
                    RuleOperator::StartsWith => actual.starts_with(expected),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RuleField {
    ActionRepoOwner,
    ActionRepoName,
    ActionRepo,
    ActionPath,
    WorkflowFile,
    CurrentRef,
    CurrentRefKind,
}

#[derive(Debug, Clone, Copy)]
enum RuleOperator {
    Equals,
    NotEquals,
    StartsWith,
}

fn field_value(action_ref: &ActionRef, field: RuleField) -> String {
    match field {
        RuleField::ActionRepoOwner => action_ref.owner.clone(),
        RuleField::ActionRepoName => action_ref.name.clone(),
        RuleField::ActionRepo => action_ref.repo.clone(),
        RuleField::ActionPath => action_ref.path.clone(),
        RuleField::WorkflowFile => action_ref.file.display().to_string(),
        RuleField::CurrentRef => action_ref.ref_name.clone(),
        RuleField::CurrentRefKind => current_ref_kind(action_ref),
    }
}

fn current_ref_kind(action_ref: &ActionRef) -> String {
    if action_ref.ref_name.len() == 40
        && action_ref
            .ref_name
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        "sha".to_string()
    } else if action_ref.ref_name.starts_with("v")
        && action_ref
            .ref_name
            .chars()
            .skip(1)
            .any(|character| character.is_ascii_digit())
        && action_ref
            .ref_name
            .chars()
            .skip(1)
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        "tag".to_string()
    } else if action_ref.ref_name.len() < 40
        && action_ref
            .ref_name
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        "short_sha".to_string()
    } else {
        "branch".to_string()
    }
}

pub fn load_for_command(shared: &SharedArgs) -> Result<Config, String> {
    let mut config = Config::default();

    apply_config_file(Path::new(".actioneer.toml"), &mut config)?;
    apply_config_file(Path::new(".github/actioneer.toml"), &mut config)?;

    if shared.recursive {
        config.recursive = true;
    }
    for filter in &shared.filter {
        config.filter.push(filter.clone());
    }
    for exclude in &shared.exclude {
        config.exclude.push(exclude.clone());
    }
    if shared.offline {
        config.offline = true;
    }
    if shared.no_cache {
        config.no_cache = true;
    }
    if let Some(mode) = shared.mode {
        config.mode = Some(mode);
    }

    if config.offline && config.no_cache {
        return Err("--offline and --no-cache cannot be used together".to_string());
    }

    Ok(config)
}

fn apply_config_file(path: &Path, config: &mut Config) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;
    let mut pending_rule: Option<PendingRule> = None;

    for (index, line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        if line == "[[rules]]" {
            if let Some(rule) = pending_rule.take() {
                config.policy_overrides.push(rule.finish(path)?);
            }
            pending_rule = Some(PendingRule::new());
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "failed to parse config {}:{line_number}: expected key = value",
                path.display()
            ));
        };

        let key = key.trim();
        let value = value.trim();

        if let Some(rule) = pending_rule.as_mut() {
            apply_rule_field(path, line_number, rule, key, value)?;
        } else {
            apply_global_field(path, line_number, config, key, value)?;
        }
    }

    if let Some(rule) = pending_rule.take() {
        config.policy_overrides.push(rule.finish(path)?);
    }

    Ok(())
}

fn apply_global_field(
    path: &Path,
    line: usize,
    config: &mut Config,
    key: &str,
    value: &str,
) -> Result<(), String> {
    match key {
        "offline" => config.offline = parse_bool(path, line, key, value)?,
        "no_cache" => config.no_cache = parse_bool(path, line, key, value)?,
        "recursive" => config.recursive = parse_bool(path, line, key, value)?,
        "filter" => config.filter.push(parse_string(path, line, key, value)?),
        "exclude" => config.exclude.push(parse_string(path, line, key, value)?),
        "pin" => config.pin = Some(parse_pin(path, line, value)?),
        "update" => config.update_level = Some(parse_update_level(path, line, value)?),
        "skip_branches" => config.skip_branches = parse_bool(path, line, key, value)?,
        "min_release_age" => config.min_release_age = Some(parse_string(path, line, key, value)?),
        "mode" => config.mode = Some(parse_mode(path, line, value)?),
        _ => {}
    }
    Ok(())
}

fn apply_rule_field(
    path: &Path,
    line: usize,
    rule: &mut PendingRule,
    key: &str,
    value: &str,
) -> Result<(), String> {
    match key {
        "name" => rule.name = Some(parse_string(path, line, key, value)?),
        "when" => {
            rule.condition = Some(parse_condition(
                path,
                line,
                &parse_string(path, line, key, value)?,
            )?)
        }
        "pin" => rule.pin = Some(parse_pin(path, line, value)?),
        "offline" | "no_cache" | "mode" | "recursive" | "filter" | "exclude"
        | "update" | "skip_branches" | "min_release_age" => {
            let label = rule
                .name
                .as_deref()
                .map(|name| format!(" in rule {name:?}"))
                .unwrap_or_default();
            return Err(format!(
                "failed to parse config {}:{line}: {key} is not valid in rules{label}",
                path.display()
            ));
        }
        _ => {}
    }
    Ok(())
}

fn parse_bool(path: &Path, line: usize, key: &str, value: &str) -> Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!(
            "failed to parse config {}:{line}: {key} must be true or false",
            path.display()
        )),
    }
}

fn parse_update_level(path: &Path, line: usize, value: &str) -> Result<UpdateLevel, String> {
    match parse_string(path, line, "update", value)?.as_str() {
        "major" => Ok(UpdateLevel::Major),
        "minor" => Ok(UpdateLevel::Minor),
        "patch" => Ok(UpdateLevel::Patch),
        other => Err(format!(
            "failed to parse config {}:{line}: update must be \"major\", \"minor\", or \"patch\", got {other:?}",
            path.display()
        )),
    }
}

fn parse_mode(path: &Path, line: usize, value: &str) -> Result<Mode, String> {
    match parse_string(path, line, "mode", value)?.as_str() {
        "tui" => Ok(Mode::Tui),
        "plain" => Ok(Mode::Plain),
        "json" => Ok(Mode::Json),
        other => Err(format!(
            "failed to parse config {}:{line}: mode must be \"tui\", \"plain\", or \"json\", got {other:?}",
            path.display()
        )),
    }
}

fn parse_pin(path: &Path, line: usize, value: &str) -> Result<PinStyle, String> {
    match parse_string(path, line, "pin", value)?.as_str() {
        "sha" => Ok(PinStyle::Sha),
        "tag" => Ok(PinStyle::Tag),
        other => Err(format!(
            "failed to parse config {}:{line}: pin must be \"sha\" or \"tag\", got {other:?}",
            path.display()
        )),
    }
}

fn parse_condition(path: &Path, line: usize, value: &str) -> Result<Expression, String> {
    let value = value.trim();
    parse_or_expr(path, line, value)
}

fn parse_or_expr(path: &Path, line: usize, value: &str) -> Result<Expression, String> {
    let parts = split_top_level(value, "||");
    if parts.len() == 1 {
        return parse_and_expr(path, line, parts[0]);
    }

    let mut parsed = Vec::new();
    for part in parts {
        parsed.push(parse_and_expr(path, line, part)?);
    }
    Ok(Expression::Or(parsed))
}

fn parse_and_expr(path: &Path, line: usize, value: &str) -> Result<Expression, String> {
    let parts = split_top_level(value, "&&");
    if parts.len() == 1 {
        return parse_primary(path, line, parts[0]);
    }

    let mut parsed = Vec::new();
    for part in parts {
        parsed.push(parse_primary(path, line, part)?);
    }
    Ok(Expression::And(parsed))
}

fn parse_primary(path: &Path, line: usize, value: &str) -> Result<Expression, String> {
    let value = value.trim();
    if value.starts_with('(') && value.ends_with(')') {
        return parse_condition(path, line, &value[1..value.len() - 1]);
    }

    if let Some(args) = value
        .strip_prefix("starts_with(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let (left, right) = args.split_once(',').ok_or_else(|| {
            format!(
                "failed to parse config {}:{line}: starts_with expects two arguments",
                path.display()
            )
        })?;
        return Ok(Expression::Compare {
            field: parse_field(path, line, left.trim())?,
            op: RuleOperator::StartsWith,
            expected: parse_string(path, line, "when", right.trim())?,
        });
    }

    let (op, left, right) = if let Some((left, right)) = value.split_once("==") {
        (RuleOperator::Equals, left, right)
    } else if let Some((left, right)) = value.split_once("!=") {
        (RuleOperator::NotEquals, left, right)
    } else {
        return Err(format!(
            "failed to parse config {}:{line}: rule condition must use ==, !=, or starts_with(...)",
            path.display()
        ));
    };

    Ok(Expression::Compare {
        field: parse_field(path, line, left.trim())?,
        op,
        expected: parse_string(path, line, "when", right.trim())?,
    })
}

fn parse_field(path: &Path, line: usize, value: &str) -> Result<RuleField, String> {
    match value {
        "ActionRepoOwner" => Ok(RuleField::ActionRepoOwner),
        "ActionRepoName" => Ok(RuleField::ActionRepoName),
        "ActionRepo" => Ok(RuleField::ActionRepo),
        "ActionPath" => Ok(RuleField::ActionPath),
        "WorkflowFile" => Ok(RuleField::WorkflowFile),
        "CurrentRef" => Ok(RuleField::CurrentRef),
        "CurrentRefKind" => Ok(RuleField::CurrentRefKind),
        other => Err(format!(
            "failed to parse config {}:{line}: unsupported rule field {other:?}",
            path.display()
        )),
    }
}

fn split_top_level<'source>(value: &'source str, delimiter: &str) -> Vec<&'source str> {
    let mut depth = 0;
    let mut start = 0;
    let mut parts = Vec::new();

    for (index, character) in value.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {
                if depth == 0 && value[index..].starts_with(delimiter) {
                    parts.push(value[start..index].trim());
                    start = index + delimiter.len();
                }
            }
        }
    }

    parts.push(value[start..].trim());
    parts
}

fn parse_string(path: &Path, line: usize, key: &str, value: &str) -> Result<String, String> {
    if let Some(value) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        return Ok(value.to_string());
    }
    if let Some(value) = value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
    {
        return Ok(value.to_string());
    }

    Err(format!(
        "failed to parse config {}:{line}: {key} must be a quoted string",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn parse(value: &str) -> Expression {
        parse_condition(Path::new("test.toml"), 1, value).expect("parse condition")
    }

    fn matches(condition: &Expression, owner: &str, name: &str, path: &str, file: &str, current_ref: &str) -> bool {
        condition.matches(&ActionRef {
            file: PathBuf::from(file),
            line: 1,
            owner: owner.to_string(),
            name: name.to_string(),
            repo: format!("{owner}/{name}"),
            path: path.to_string(),
            ref_name: current_ref.to_string(),
            version_comment: None,
        })
    }

    #[test]
    fn equality_operator_matches_exact_value() {
        let condition = parse("ActionRepo == \"actions/checkout\"");
        assert!(matches(
            &condition,
            "actions",
            "checkout",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
        assert!(!matches(
            &condition,
            "actions",
            "setup-node",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
    }

    #[test]
    fn not_equals_operator_rejects_value() {
        let condition = parse("ActionRepoName != \"checkout\"");
        assert!(matches(
            &condition,
            "actions",
            "setup-node",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
        assert!(!matches(
            &condition,
            "actions",
            "checkout",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
    }

    #[test]
    fn starts_with_operator_matches_prefix() {
        let condition = parse("starts_with(ActionRepo, \"actions/\")");
        assert!(matches(
            &condition,
            "actions",
            "checkout",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
        assert!(!matches(
            &condition,
            "github",
            "codeql",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
    }

    #[test]
    fn and_operator_requires_both_sides() {
        let condition = parse("ActionRepo == \"actions/checkout\" && CurrentRefKind == \"tag\"");
        assert!(matches(
            &condition,
            "actions",
            "checkout",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
        assert!(!matches(
            &condition,
            "actions",
            "checkout",
            "",
            ".github/workflows/ci.yml",
            "abc123"
        ));
    }

    #[test]
    fn or_operator_allows_either_side() {
        let condition = parse("ActionRepo == \"actions/checkout\" || ActionRepo == \"actions/setup-node\"");
        assert!(matches(
            &condition,
            "actions",
            "setup-node",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
        assert!(!matches(
            &condition,
            "actions",
            "upload-artifact",
            "",
            ".github/workflows/ci.yml",
            "v4"
        ));
    }

    #[test]
    fn parentheses_override_precedence() {
        let condition = parse("(ActionRepo == \"a/b\" || ActionRepo == \"c/d\") && CurrentRefKind == \"sha\"");
        assert!(matches(&condition, "a", "b", "", ".github/workflows/ci.yml", "0123456789012345678901234567890123456789"));
        assert!(!matches(&condition, "a", "b", "", ".github/workflows/ci.yml", "v4"));
        assert!(!matches(&condition, "x", "y", "", ".github/workflows/ci.yml", "0123456789012345678901234567890123456789"));
    }

    #[test]
    fn unsupported_field_fails() {
        let result = parse_condition(Path::new("test.toml"), 1, "UnknownField == \"x\"");
        assert!(result.is_err());
    }
}
