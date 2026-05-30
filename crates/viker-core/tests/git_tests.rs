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
fn git_line_operations_stage_unstage_and_discard_single_diff_lines() {
    let (project, repo) = temp_repo("line-ops");
    write_file(&project.root, "notes.txt", "alpha\ncharlie\n");
    commit_all(&repo, "initial");
    write_file(&project.root, "notes.txt", "alpha\nbravo\ncharlie\n");

    let worktree = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Worktree,
            context_lines: 0,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    let line_id = worktree.files[0].hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == GitLineKind::Addition && line.content == "bravo")
        .unwrap()
        .id
        .clone();

    git::stage_line(&project.root, "notes.txt", &line_id).unwrap();
    let staged = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Staged,
            context_lines: 0,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert!(
        staged.files[0].hunks[0]
            .lines
            .iter()
            .any(|line| line.kind == GitLineKind::Addition && line.content == "bravo")
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
    assert!(worktree.files.is_empty());

    let staged_line_id = staged.files[0].hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == GitLineKind::Addition && line.content == "bravo")
        .unwrap()
        .id
        .clone();
    git::unstage_line(&project.root, "notes.txt", &staged_line_id).unwrap();
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

    let worktree = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Worktree,
            context_lines: 0,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    let line_id = worktree.files[0].hunks[0]
        .lines
        .iter()
        .find(|line| line.kind == GitLineKind::Addition && line.content == "bravo")
        .unwrap()
        .id
        .clone();
    git::discard_line(&project.root, "notes.txt", &line_id).unwrap();
    assert_eq!(
        std::fs::read_to_string(project.root.join("notes.txt")).unwrap(),
        "alpha\ncharlie\n"
    );
}

#[test]
fn git_apply_patch_and_discard_file_cover_fallback_review_actions() {
    let (project, repo) = temp_repo("patch-discard");
    write_file(
        &project.root,
        "src/lib.rs",
        "pub fn value() -> i32 {\n    1\n}\n",
    );
    commit_all(&repo, "initial");
    write_file(
        &project.root,
        "src/lib.rs",
        "pub fn value() -> i32 {\n    2\n}\n",
    );

    let worktree = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Worktree,
            context_lines: 3,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    let patch = worktree.files[0].hunks[0].raw_patch.clone();
    git::apply_patch(&project.root, &patch, git::GitApplyPatchMode::StageToIndex).unwrap();
    let staged = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Staged,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert_eq!(staged.files.len(), 1);

    git::apply_patch(
        &project.root,
        &patch,
        git::GitApplyPatchMode::UnstageFromIndex,
    )
    .unwrap();
    let staged = git::repository_diff(
        &project.root,
        GitDiffOptions {
            mode: GitDiffMode::Staged,
            ..GitDiffOptions::default()
        },
    )
    .unwrap();
    assert!(staged.files.is_empty());

    git::discard_file(&project.root, "src/lib.rs").unwrap();
    assert_eq!(
        std::fs::read_to_string(project.root.join("src/lib.rs")).unwrap(),
        "pub fn value() -> i32 {\n    1\n}\n"
    );

    write_file(&project.root, "scratch.txt", "temporary\n");
    git::discard_file(&project.root, "scratch.txt").unwrap();
    assert!(!project.root.join("scratch.txt").exists());
}

#[test]
fn git_reference_diffs_cover_branches_commits_and_stashes() {
    let (project, repo) = temp_repo("reference-diff");
    write_file(&project.root, "src/lib.rs", "pub fn value() -> i32 { 1 }\n");
    commit_all(&repo, "initial");
    git::create_branch(&project.root, "feature").unwrap();
    git::checkout_branch(&project.root, "feature").unwrap();
    write_file(&project.root, "src/lib.rs", "pub fn value() -> i32 { 2 }\n");
    commit_all(&repo, "feature change");
    let feature_oid = repo.head().unwrap().target().unwrap().to_string();
    git::checkout_branch(&project.root, "master").unwrap();

    let branch_diff =
        git::repository_diff_reference(&project.root, "HEAD...feature", 3, &[], true, 1_000_000)
            .unwrap();
    assert_eq!(branch_diff.mode, GitDiffMode::Reference);
    assert_eq!(branch_diff.files[0].new_path.as_deref(), Some("src/lib.rs"));

    let commit_diff =
        git::repository_diff_reference(&project.root, &feature_oid, 3, &[], false, 1_000_000)
            .unwrap();
    assert_eq!(commit_diff.files[0].new_path.as_deref(), Some("src/lib.rs"));

    write_file(&project.root, "src/lib.rs", "pub fn value() -> i32 { 3 }\n");
    git::stash_push(&project.root, Some("stash change")).unwrap();
    let stash_diff =
        git::repository_diff_reference(&project.root, "stash@{0}", 3, &[], false, 1_000_000)
            .unwrap();
    assert_eq!(stash_diff.files[0].new_path.as_deref(), Some("src/lib.rs"));
}

#[test]
fn git_commits_snapshot_and_selective_stash_are_exposed() {
    let (project, repo) = temp_repo("commits-snapshot-stash");
    write_file(&project.root, "a.txt", "one\n");
    write_file(&project.root, "b.txt", "one\n");
    commit_all(&repo, "initial");
    write_file(&project.root, "a.txt", "two\n");
    commit_all(&repo, "second");

    let commits = git::list_commits(&project.root, 10, None).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].summary, "second");
    assert!(commits[0].decorations.iter().any(|name| name == "master"));

    let before = git::change_snapshot(&project.root).unwrap();
    write_file(&project.root, "a.txt", "three\n");
    let after = git::change_snapshot(&project.root).unwrap();
    assert_ne!(before.status_signature, after.status_signature);

    write_file(&project.root, "b.txt", "two\n");
    git::stash_file(&project.root, "a.txt", Some("stash only a")).unwrap();
    let status = git::repository_status(&project.root).unwrap();
    assert!(status.files.iter().all(|file| file.path != "a.txt"));
    assert!(status.files.iter().any(|file| file.path == "b.txt"));
    assert_eq!(status.stashes.len(), 1);
    assert!(status.stashes[0].message.contains("stash only a"));
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
