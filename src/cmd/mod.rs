pub mod audit;
pub mod update;
pub mod version;

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
