use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::{
    audit::{Finding, FindingKind},
    config::{Config, PinStyle},
    github::{GitHubTag, GitHubTags},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFix {
    pub finding_id: String,
    pub file: String,
    pub line: usize,
    pub applied: bool,
    pub new_ref: String,
    pub new_version_comment: String,
    old_uses: String,
    new_uses: String,
}

pub fn plan_fixes(findings: &[Finding], config: &Config, github_tags: &GitHubTags) -> Result<Vec<AuditFix>, String> {
    let mut fixes = Vec::new();

    for finding in findings {
        if !finding.fixable {
            continue;
        }

        let fix = match &finding.kind {
            FindingKind::MutableRef => plan_mutable_ref_fix(finding, config, github_tags)?,
            FindingKind::ShaCommentMismatch | FindingKind::ShortSha => plan_sha_comment_fix(finding),
        };

        if let Some(fix) = fix {
            fixes.push(fix);
        }
    }

    Ok(fixes)
}

fn plan_mutable_ref_fix(
    finding: &Finding,
    config: &Config,
    github_tags: &GitHubTags,
) -> Result<Option<AuditFix>, String> {
    let tags = github_tags.tags_for_repo(&finding.action.owner, &finding.action.name)?;
    let Some(tag) = newest_version_tag(&tags) else {
        return Ok(None);
    };

    let pin = config.effective_pin(&finding.action);
    let action_name = action_name(&finding.action.repo, &finding.action.path);

    let (new_ref, new_version_comment, new_uses) = match pin {
        PinStyle::Sha => {
            let new_uses = format!("{action_name}@{} # {}", tag.sha, tag.name);
            (tag.sha.clone(), tag.name.clone(), new_uses)
        }
        PinStyle::Tag => {
            let new_uses = format!("{action_name}@{}", tag.name);
            (tag.name.clone(), String::new(), new_uses)
        }
    };

    Ok(Some(AuditFix {
        finding_id: finding.id.clone(),
        file: finding.action.file.display().to_string(),
        line: finding.action.line,
        applied: false,
        new_ref,
        new_version_comment,
        old_uses: format!("{action_name}@{}", finding.action.ref_name),
        new_uses,
    }))
}

fn plan_sha_comment_fix(finding: &Finding) -> Option<AuditFix> {
    let expected_sha = finding.expected_sha.as_ref()?;
    let action_name = action_name(&finding.action.repo, &finding.action.path);
    let comment = finding.action.version_comment.as_deref().unwrap_or("");

    let old_uses = if comment.is_empty() {
        format!("{action_name}@{}", finding.action.ref_name)
    } else {
        format!("{action_name}@{} # {}", finding.action.ref_name, comment)
    };

    let new_uses = format!("{action_name}@{expected_sha} # {comment}");

    Some(AuditFix {
        finding_id: finding.id.clone(),
        file: finding.action.file.display().to_string(),
        line: finding.action.line,
        applied: false,
        new_ref: expected_sha.clone(),
        new_version_comment: comment.to_string(),
        old_uses,
        new_uses,
    })
}

pub fn apply_fixes(fixes: &mut [AuditFix]) -> Result<(), String> {
    let mut fixes_by_file = BTreeMap::<PathBuf, Vec<usize>>::new();
    for (index, fix) in fixes.iter().enumerate() {
        fixes_by_file
            .entry(PathBuf::from(&fix.file))
            .or_default()
            .push(index);
    }

    for (file, fix_indexes) in fixes_by_file {
        let contents = fs::read_to_string(&file)
            .map_err(|error| format!("failed to read {} for patching: {error}", file.display()))?;
        let mut lines = contents.lines().map(str::to_string).collect::<Vec<_>>();
        let had_trailing_newline = contents.ends_with('\n');

        for fix_index in fix_indexes {
            let fix = &fixes[fix_index];
            let line_index = fix
                .line
                .checked_sub(1)
                .ok_or_else(|| format!("invalid patch line {} in {}", fix.line, fix.file))?;
            let line = lines.get_mut(line_index).ok_or_else(|| {
                format!(
                    "cannot patch {}:{} because the line no longer exists",
                    fix.file, fix.line
                )
            })?;

            if !line.contains(&fix.old_uses) {
                return Err(format!(
                    "cannot patch {}:{} because {:?} is no longer present",
                    fix.file, fix.line, fix.old_uses
                ));
            }

            *line = line.replacen(&fix.old_uses, &fix.new_uses, 1);
            fixes[fix_index].applied = true;
        }

        let mut patched = lines.join("\n");
        if had_trailing_newline {
            patched.push('\n');
        }
        fs::write(&file, patched)
            .map_err(|error| format!("failed to write patched file {}: {error}", file.display()))?;
    }

    Ok(())
}

fn newest_version_tag(tags: &[GitHubTag]) -> Option<&GitHubTag> {
    tags.iter().max_by_key(|tag| version_key(&tag.name))
}

fn version_key(tag: &str) -> Vec<u64> {
    tag.strip_prefix('v')
        .unwrap_or(tag)
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn action_name(repo: &str, path: &str) -> String {
    if path.is_empty() {
        repo.to_string()
    } else {
        format!("{repo}/{path}")
    }
}
