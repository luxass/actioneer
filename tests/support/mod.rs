use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_WORKSPACE_ID: AtomicUsize = AtomicUsize::new(0);

pub struct WorkflowWorkspace {
    root: PathBuf,
}

impl WorkflowWorkspace {
    pub fn new(file: &str, line: u32) -> Self {
        let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "actioneer-workflow-{}-{}-{id}",
            std::process::id(),
            sanitize(&format!("{file}-{line}"))
        ));
        fs::create_dir_all(&root).expect("create workflow workspace");
        Self { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn write(&self, relative_path: &str, contents: impl AsRef<str>) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create workflow fixture parent directory");
        }
        fs::write(path, contents.as_ref()).expect("write workflow fixture file");
    }
}

impl Drop for WorkflowWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn dedent(contents: &str) -> String {
    let normalized = contents.replace("\r\n", "\n");
    let mut lines = normalized.split('\n').collect::<Vec<_>>();

    if lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    if lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|character| character.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    let mut output = lines
        .iter()
        .map(|line| strip_indent(line, indent))
        .collect::<Vec<_>>()
        .join("\n");
    output.push('\n');
    output
}

fn strip_indent(line: &str, indent: usize) -> &str {
    if line.trim().is_empty() {
        return "";
    }

    let byte_index = line
        .char_indices()
        .map(|(index, _)| index)
        .nth(indent)
        .unwrap_or(line.len());
    &line[byte_index..]
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[macro_export]
macro_rules! workflow_workspace {
    ($($path:literal => $contents:expr),+ $(,)?) => {{
        let workspace = $crate::support::WorkflowWorkspace::new(file!(), line!());
        $(
            workspace.write($path, $crate::support::dedent($contents));
        )+
        workspace
    }};
}
