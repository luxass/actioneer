use std::collections::BTreeSet;

use thiserror::Error;

use crate::engine::rewrite::{apply_selected_updates, RewriteError};
use crate::engine::scanner::{scan, ScanError};
use crate::github::Client as GitHubClient;
use crate::model::{Reference, ResolveOptions, ResolvedUpdate};

pub mod git;
mod resolver;
pub mod rewrite;
pub mod scanner;

pub use resolver::ResolveError;

#[derive(Debug)]
pub struct CheckOptions {
    pub paths: Vec<String>,
    pub recursive: bool,
    pub no_cache: bool,
    pub resolve_options: ResolveOptions,
}

#[derive(Debug)]
pub struct CheckResult {
    pub reference_count: usize,
    pub reference_file_count: usize,
    pub updates: Vec<ResolvedUpdate>,
    pub skipped_branches: usize,
}

#[derive(Debug)]
pub struct ApplyResult {
    pub applied: usize,
    pub selected_files: usize,
}

#[derive(Debug, Error)]
pub enum CheckError {
    #[error(transparent)]
    Scan(ScanError),
    #[error(transparent)]
    Resolve(#[from] ResolveError),
}

pub fn check(options: CheckOptions) -> Result<CheckResult, CheckError> {
    let references = scan(&options.paths, options.recursive).map_err(CheckError::Scan)?;
    let github = GitHubClient::new(options.no_cache);
    let (updates, skipped_branches) = resolver::resolve_updates(
        &|repository| github.fetch_tags(repository),
        &references,
        &options.resolve_options,
    )?;

    Ok(CheckResult {
        reference_count: references.len(),
        reference_file_count: count_reference_files(&references),
        updates,
        skipped_branches,
    })
}

pub fn apply(updates: &[ResolvedUpdate], selected: &[usize]) -> Result<ApplyResult, RewriteError> {
    let applied = apply_selected_updates(updates, selected)?;
    Ok(ApplyResult {
        applied,
        selected_files: count_selected_files(updates, selected),
    })
}

fn count_reference_files(references: &[Reference]) -> usize {
    references
        .iter()
        .map(|reference| reference.source.file.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn count_selected_files(updates: &[ResolvedUpdate], selected: &[usize]) -> usize {
    selected
        .iter()
        .filter_map(|index| updates.get(*index))
        .map(|update| update.file())
        .collect::<BTreeSet<_>>()
        .len()
}
