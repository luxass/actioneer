use std::collections::{HashMap, HashSet};

use crate::actions::{ActionReference, Tag};
use crate::github::{Error as GitHubError, GitHubClient};

pub mod audit;
pub mod update;
pub mod version;

pub(crate) struct FetchTagsError {
    pub owner: String,
    pub name: String,
    pub error: GitHubError,
}

pub(crate) fn fetch_tags_for_actions(
    actions: &[ActionReference],
    gh: &GitHubClient,
) -> Result<HashMap<(String, String), Vec<Tag>>, FetchTagsError> {
    let repos: HashSet<(String, String)> = actions
        .iter()
        .map(|a| (a.owner.clone(), a.name.clone()))
        .collect();
    let mut tags = HashMap::new();
    for (owner, name) in repos {
        match gh.fetch_tags(&owner, &name) {
            Ok(repo_tags) => {
                tags.insert((owner, name), repo_tags);
            }
            Err(error) => {
                return Err(FetchTagsError { owner, name, error });
            }
        }
    }
    Ok(tags)
}

fn default_inputs(inputs: Vec<String>, recursive: bool) -> Vec<String> {
    if inputs.is_empty() {
        vec![if recursive { "." } else { ".github" }.to_string()]
    } else {
        inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_recursive_defaults_to_dot() {
        assert_eq!(vec!["."], default_inputs(vec![], true));
    }

    #[test]
    fn empty_non_recursive_defaults_to_github() {
        assert_eq!(vec![".github"], default_inputs(vec![], false));
    }

    #[test]
    fn explicit_inputs_returned_verbatim() {
        assert_eq!(vec!["ci.yml"], default_inputs(vec!["ci.yml".into()], true));
    }
}
