use std::fs;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct TestWorkspace {
    root: std::path::PathBuf,
}

impl TestWorkspace {
    pub fn new(namespace: &str) -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let namespace: String = namespace
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "actioneer-{namespace}-test-{}-{n}-{nonce}",
            std::process::id(),
        ));
        fs::create_dir(&root).unwrap();
        Self { root }
    }

    pub fn write(&self, path: &str, contents: &str) -> std::path::PathBuf {
        let file = self.root.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file, normalize_fixture(contents)).unwrap();
        file
    }

    #[allow(dead_code)]
    pub fn write_raw(&self, path: &str, contents: &str) -> std::path::PathBuf {
        let file = self.root.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file, contents).unwrap();
        file
    }

    #[allow(dead_code)]
    pub fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).unwrap()
    }

    #[allow(dead_code)]
    pub fn path(&self, path: &str) -> String {
        self.root.join(path).display().to_string()
    }

    pub fn root(&self) -> String {
        self.root.display().to_string()
    }
}

fn normalize_fixture(contents: &str) -> String {
    let contents = contents.strip_prefix('\n').unwrap_or(contents);
    let contents = contents.strip_suffix('\n').unwrap_or(contents);
    let indent = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.find(|c| c != ' ' && c != '\t'))
        .min()
        .unwrap_or(0);

    let mut normalized = String::new();
    for line in contents.lines() {
        if line.len() >= indent {
            normalized.push_str(&line[indent..]);
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
    }
    normalized
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

macro_rules! test_workspace {
    ($($path:literal => $contents:expr),+ $(,)?) => {{
        let workspace = $crate::support::TestWorkspace::new(module_path!());
        $(workspace.write($path, $contents);)+
        workspace
    }};
}
