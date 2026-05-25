use std::path::{Path, PathBuf};

use viker_core::search;

struct TempProject {
    root: PathBuf,
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn temp_project(name: &str) -> TempProject {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "viker-search-{name}-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    TempProject { root }
}

fn write_file(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

#[test]
fn search_scans_project_files_with_gitignore_rules() {
    let project = temp_project("scan");
    write_file(&project.root, ".gitignore", "target\n");
    write_file(&project.root, "src/main.rs", "fn main() {}\n");
    write_file(&project.root, "target/generated.rs", "ignored\n");

    let files = search::scan_project_files(&project.root);

    assert_eq!(files, vec![".gitignore", "src/main.rs"]);
}

#[test]
fn search_filters_file_paths_with_skim_matcher() {
    let entries = vec![
        "README.md".to_string(),
        "src/editor/mod.rs".to_string(),
        "src/search.rs".to_string(),
    ];

    let results = search::filter_file_paths(&entries, "srs", 10);

    assert!(results.iter().any(|result| result.path == "src/search.rs"));
    assert!(!results.iter().any(|result| result.path == "README.md"));
    assert!(
        results
            .iter()
            .all(|result| !result.matched_indices.is_empty())
    );
}

#[test]
fn search_file_contents_finds_matching_lines() {
    let project = temp_project("content");
    write_file(
        &project.root,
        "src/lib.rs",
        "pub fn answer() -> i32 {\n    42\n}\n",
    );
    write_file(&project.root, "notes.md", "nothing relevant\n");

    let results = search::search_file_contents(&project.root, "answer", 20);

    let result = results
        .iter()
        .find(|result| result.path == "src/lib.rs")
        .unwrap();
    assert_eq!(result.row, 0);
    assert_eq!(result.column, 7);
    assert!(result.text.contains("answer"));
    assert!(!result.matched_indices.is_empty());
    assert!(search::search_file_contents(&project.root, "", 20).is_empty());
}
