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
