use std::path::{Path, PathBuf};

use git2::{IndexAddOption, Repository, Signature};
use viker_core::editor::document::Document;
use viker_core::editor::{DeferredAction, Editor};
use viker_core::git::{self, GitDiffMode, GitDiffOptions, GitLineKind};

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn temp_repo(name: &str) -> (TempRepo, Repository) {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("viker-git-{name}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let repo = Repository::init(&root).unwrap();
    (TempRepo { root }, repo)
}

fn write_file(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, text).unwrap();
}

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = Signature::now("Viker", "viker@example.invalid").unwrap();
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents = parent.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .unwrap();
}

#[test]
fn git_diff_includes_parseable_syntax_highlighted_patches() {
    let (project, repo) = temp_repo("diff-highlight");
    write_file(
        &project.root,
        "src/lib.rs",
        "pub fn answer() -> i32 {\n    41\n}\n",
    );
    commit_all(&repo, "initial");

    write_file(
        &project.root,
        "src/lib.rs",
        "pub fn answer() -> i32 {\n    let value = 42;\n    value\n}\n",
    );

    let diff = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Worktree,
            context_lines: 3,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].new_path.as_deref(), Some("src/lib.rs"));
    let added = diff.files[0].hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == GitLineKind::Addition && line.content.contains("let value"))
        .unwrap();
    assert!(!added.highlights.is_empty());

    let json = git::repository_diff_json(&project.root, GitDiffOptions::default()).unwrap();
    assert!(json.contains("src/lib.rs"));
    assert!(json.contains("highlights"));
}

#[test]
fn git_stage_and_unstage_files_update_status_and_staged_diff() {
    let (project, repo) = temp_repo("stage-file");
    write_file(&project.root, "src/lib.rs", "fn answer() -> i32 { 1 }\n");
    commit_all(&repo, "initial");
    write_file(&project.root, "src/lib.rs", "fn answer() -> i32 { 2 }\n");

    git::stage_files(&project.root, &["src/lib.rs".to_string()]).unwrap();
    let status = git::repository_status(&project.root).unwrap();
    assert!(
        status
            .files
            .iter()
            .any(|file| file.path == "src/lib.rs" && file.staged)
    );

    let staged = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Staged,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert_eq!(staged.files.len(), 1);

    git::unstage_files(&project.root, &["src/lib.rs".to_string()]).unwrap();
    let status = git::repository_status(&project.root).unwrap();
    assert!(
        status
            .files
            .iter()
            .any(|file| file.path == "src/lib.rs" && !file.staged && file.unstaged)
    );
}

#[test]
fn git_stage_and_unstage_hunks_operate_on_individual_patches() {
    let (project, repo) = temp_repo("stage-hunk");
    write_file(
        &project.root,
        "src/lib.rs",
        "fn a() {\n    let n = 1;\n}\n\nfn b() {\n    let n = 10;\n}\n",
    );
    commit_all(&repo, "initial");
    write_file(
        &project.root,
        "src/lib.rs",
        "fn a() {\n    let n = 2;\n}\n\nfn b() {\n    let n = 20;\n}\n",
    );

    let worktree = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Worktree,
            context_lines: 0,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert_eq!(worktree.files[0].hunks.len(), 2);
    let first_hunk = worktree.files[0].hunks[0].id.clone();

    git::stage_hunk(&project.root, "src/lib.rs", &first_hunk).unwrap();
    let staged = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Staged,
            context_lines: 0,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert_eq!(staged.files[0].hunks.len(), 1);

    let staged_hunk = staged.files[0].hunks[0].id.clone();
    git::unstage_hunk(&project.root, "src/lib.rs", &staged_hunk).unwrap();
    let staged = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Staged,
            context_lines: 0,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert!(staged.files.is_empty());
}

#[test]
fn git_branch_and_stash_operations_are_exposed() {
    let (project, repo) = temp_repo("branches-stash");
    write_file(&project.root, "src/lib.rs", "fn answer() -> i32 { 1 }\n");
    commit_all(&repo, "initial");

    git::create_branch(&project.root, "feature").unwrap();
    git::checkout_branch(&project.root, "feature").unwrap();
    let status = git::repository_status(&project.root).unwrap();
    assert_eq!(status.branch.as_deref(), Some("feature"));
    assert!(
        status
            .branches
            .iter()
            .any(|branch| branch.name == "feature")
    );

    write_file(&project.root, "src/lib.rs", "fn answer() -> i32 { 3 }\n");
    git::stash_push(&project.root, Some("save work")).unwrap();
    let status = git::repository_status(&project.root).unwrap();
    assert_eq!(status.stashes.len(), 1);
    assert!(status.files.is_empty());
    git::stash_pop(&project.root, 0).unwrap();
    let status = git::repository_status(&project.root).unwrap();
    assert!(status.files.iter().any(|file| file.path == "src/lib.rs"));
}

#[test]
fn git_delete_and_amend_operations_are_exposed() {
    let (project, repo) = temp_repo("delete-amend");
    write_file(&project.root, "src/lib.rs", "fn answer() -> i32 { 1 }\n");
    write_file(&project.root, "README.md", "docs\n");
    commit_all(&repo, "initial");

    git::delete_files(&project.root, &["README.md".to_string()]).unwrap();
    assert!(!project.root.join("README.md").exists());
    let status = git::repository_status(&project.root).unwrap();
    assert!(
        status
            .files
            .iter()
            .any(|file| file.path == "README.md" && file.staged)
    );

    write_file(&project.root, "src/lib.rs", "fn answer() -> i32 { 2 }\n");
    git::stage_files(&project.root, &["src/lib.rs".to_string()]).unwrap();
    git::amend(&project.root, Some("amended")).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message(), Some("amended"));
}

#[test]
fn git_merge_and_rebase_operations_are_exposed() {
    let (project, repo) = temp_repo("merge-rebase");
    write_file(&project.root, "base.txt", "base\n");
    commit_all(&repo, "initial");

    git::create_branch(&project.root, "feature").unwrap();
    git::checkout_branch(&project.root, "feature").unwrap();
    write_file(&project.root, "feature.txt", "feature\n");
    commit_all(&repo, "feature");

    git::checkout_branch(&project.root, "master").unwrap();
    git::merge_branch(&project.root, "feature").unwrap();
    assert!(project.root.join("feature.txt").exists());

    git::create_branch(&project.root, "topic").unwrap();
    git::checkout_branch(&project.root, "topic").unwrap();
    write_file(&project.root, "topic.txt", "topic\n");
    commit_all(&repo, "topic");

    git::checkout_branch(&project.root, "master").unwrap();
    write_file(&project.root, "main.txt", "main\n");
    commit_all(&repo, "main");

    git::checkout_branch(&project.root, "topic").unwrap();
    let report = git::rebase_onto(&project.root, "master").unwrap();
    assert!(report.message.contains("rebased"));
    assert!(project.root.join("main.txt").exists());
    assert!(project.root.join("topic.txt").exists());
}

#[test]
fn git_commands_are_available_through_shared_editor_command_mode() {
    let mut editor = Editor::new(Document::new_empty());

    editor.command_buffer = "git diff --staged src/lib.rs".to_string();
    let action = editor.command_execute();
    assert!(matches!(
        action,
        Some(DeferredAction::Git(git::GitEditorCommand::Diff {
            mode: GitDiffMode::Staged,
            paths
        })) if paths == vec!["src/lib.rs".to_string()]
    ));

    editor.command_buffer = "git stage-hunk src/lib.rs src/lib.rs:0:1,1:1,1".to_string();
    let action = editor.command_execute();
    assert!(matches!(
        action,
        Some(DeferredAction::Git(git::GitEditorCommand::StageHunk { path, hunk_id }))
            if path == "src/lib.rs" && hunk_id == "src/lib.rs:0:1,1:1,1"
    ));
}
