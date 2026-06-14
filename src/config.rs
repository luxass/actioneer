use std::{fs, path::Path};

use crate::cli::SharedArgs;

#[derive(Debug, Clone, Copy, Default)]
pub struct ExecutionConfig {
    pub offline: bool,
    pub no_cache: bool,
}

pub fn load_for_command(shared: &SharedArgs) -> Result<ExecutionConfig, String> {
    let mut config = ExecutionConfig::default();

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

fn apply_config_file(path: &Path, config: &mut ExecutionConfig) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read config {}: {error}", path.display()))?;

    for (index, line) in contents.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "failed to parse config {}:{}: expected key = value",
                path.display(),
                index + 1
            ));
        };

        let key = key.trim();
        let value = value.trim();
        match key {
            "offline" => config.offline = parse_bool(path, index + 1, key, value)?,
            "no_cache" => config.no_cache = parse_bool(path, index + 1, key, value)?,
            _ => {}
        }
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
