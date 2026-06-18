use std::{fs, path::Path};

use crate::{cli::SharedArgs, discovery::ActionRef};

#[derive(Debug, Clone, Default)]
pub struct Config {
    pub offline: bool,
    pub no_cache: bool,
    policy_overrides: Vec<PolicyOverride>,
}

impl Config {
    pub fn effective_pin(&self, action_ref: &ActionRef) -> PinStyle {
        let mut pin = PinStyle::Sha;

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
    condition: Option<RuleCondition>,
    pin: Option<PinStyle>,
}

impl PolicyOverride {
    fn global(pin: PinStyle) -> Self {
        Self {
            condition: None,
            pin: Some(pin),
        }
    }

    fn matches(&self, action_ref: &ActionRef) -> bool {
        self.condition
            .as_ref()
            .is_none_or(|condition| condition.matches(action_ref))
    }
}

#[derive(Debug, Clone)]
struct PendingRule {
    name: Option<String>,
    condition: Option<RuleCondition>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
struct RuleCondition {
    field: RuleField,
    operator: RuleOperator,
    expected: String,
}

impl RuleCondition {
    fn matches(&self, action_ref: &ActionRef) -> bool {
        let actual = match self.field {
            RuleField::ActionRepoOwner => &action_ref.owner,
            RuleField::ActionRepoName => &action_ref.name,
            RuleField::ActionRepo => &action_ref.repo,
        };

        match self.operator {
            RuleOperator::Equals => actual == &self.expected,
            RuleOperator::NotEquals => actual != &self.expected,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RuleField {
    ActionRepoOwner,
    ActionRepoName,
    ActionRepo,
}

#[derive(Debug, Clone, Copy)]
enum RuleOperator {
    Equals,
    NotEquals,
}

pub fn load_for_command(shared: &SharedArgs) -> Result<Config, String> {
    let mut config = Config::default();

    apply_config_file(Path::new(".actioneer.toml"), &mut config)?;
    apply_config_file(Path::new(".github/actioneer.toml"), &mut config)?;

    if shared.offline {
        config.offline = true;
    }
    if shared.no_cache {
        config.no_cache = true;
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
        "pin" => config
            .policy_overrides
            .push(PolicyOverride::global(parse_pin(path, line, value)?)),
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
        "offline" | "no_cache" | "mode" => {
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

fn parse_condition(path: &Path, line: usize, value: &str) -> Result<RuleCondition, String> {
    let (operator, left, right) = if let Some((left, right)) = value.split_once("==") {
        (RuleOperator::Equals, left, right)
    } else if let Some((left, right)) = value.split_once("!=") {
        (RuleOperator::NotEquals, left, right)
    } else {
        return Err(format!(
            "failed to parse config {}:{line}: rule condition must use == or !=",
            path.display()
        ));
    };

    let field = match left.trim() {
        "ActionRepoOwner" => RuleField::ActionRepoOwner,
        "ActionRepoName" => RuleField::ActionRepoName,
        "ActionRepo" => RuleField::ActionRepo,
        other => {
            return Err(format!(
                "failed to parse config {}:{line}: unsupported rule field {other:?}",
                path.display()
            ));
        }
    };

    Ok(RuleCondition {
        field,
        operator,
        expected: parse_string(path, line, "when", right.trim())?,
    })
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
