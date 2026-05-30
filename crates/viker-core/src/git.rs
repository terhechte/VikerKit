use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, bail};
use git2::build::CheckoutBuilder;
use git2::{
    ApplyLocation, ApplyOptions, BranchType, Delta, DiffFindOptions, DiffOptions, ObjectType, Oid,
    Repository, Signature, Sort, Status, StatusOptions,
};
use ropey::Rope;
use serde::{Deserialize, Serialize};

use crate::highlight::style::{SyntaxStyle, SyntaxToken};
use crate::highlight::{Highlighter, SyntaxLanguage};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitDiffMode {
    Worktree,
    Staged,
    Head,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Typechange,
    Untracked,
    Conflicted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitLineKind {
    Context,
    Addition,
    Deletion,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitPatchHighlight {
    pub start_column: usize,
    pub end_column: usize,
    pub token: SyntaxToken,
    pub style: SyntaxStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiffLine {
    pub id: String,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub kind: GitLineKind,
    pub prefix: String,
    pub content: String,
    pub highlights: Vec<GitPatchHighlight>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiffHunk {
    pub id: String,
    pub header: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<GitDiffLine>,
    pub raw_patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub change: GitChangeKind,
    pub binary: bool,
    pub hunks: Vec<GitDiffHunk>,
    #[serde(skip)]
    old_blob_id: Option<Oid>,
    #[serde(skip)]
    new_blob_id: Option<Oid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiff {
    pub repository_root: String,
    pub mode: GitDiffMode,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub files: Vec<GitFileDiff>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiffOptions {
    pub mode: GitDiffMode,
    pub context_lines: u32,
    pub include_untracked: bool,
    pub pathspecs: Vec<String>,
    pub max_highlight_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitApplyPatchMode {
    StageToIndex,
    UnstageFromIndex,
    DiscardFromWorktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCommitSummary {
    pub oid: String,
    pub short_oid: String,
    pub summary: String,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub time_seconds: i64,
    pub decorations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitChangeSnapshot {
    pub repository_root: String,
    pub head: Option<String>,
    pub index_tree: Option<String>,
    pub staged_signature: String,
    pub unstaged_signature: String,
    pub untracked_signature: String,
    pub status_signature: String,
}

impl Default for GitDiffOptions {
    fn default() -> Self {
        Self {
            mode: GitDiffMode::Worktree,
            context_lines: 3,
            include_untracked: true,
            pathspecs: Vec::new(),
            max_highlight_bytes: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    pub old_path: Option<String>,
    pub index: Option<GitChangeKind>,
    pub worktree: Option<GitChangeKind>,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflicted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranch {
    pub name: String,
    pub is_current: bool,
    pub is_remote: bool,
    pub upstream: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitStash {
    pub index: usize,
    pub message: String,
    pub oid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRepositoryStatus {
    pub repository_root: String,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub detached: bool,
    pub files: Vec<GitFileStatus>,
    pub branches: Vec<GitBranch>,
    pub stashes: Vec<GitStash>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitOperationReport {
    pub message: String,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitEditorCommand {
    Status,
    Branches,
    Diff {
        mode: GitDiffMode,
        paths: Vec<String>,
    },
    StageFiles(Vec<String>),
    UnstageFiles(Vec<String>),
    StageHunk {
        path: String,
        hunk_id: String,
    },
    UnstageHunk {
        path: String,
        hunk_id: String,
    },
    DeleteFiles(Vec<String>),
    CreateBranch(String),
    CheckoutBranch(String),
    Amend {
        message: Option<String>,
    },
    StashPush {
        message: Option<String>,
    },
    StashApply {
        index: usize,
    },
    StashPop {
        index: usize,
    },
    Merge {
        branch: String,
    },
    Rebase {
        upstream: String,
    },
}

pub fn discover_repository_root(path: impl AsRef<Path>) -> Result<PathBuf> {
    let repo = Repository::discover(path.as_ref()).with_context(|| {
        format!(
            "could not find a git repository from {}",
            path.as_ref().display()
        )
    })?;
    repository_root(&repo)
}

pub fn repository_status(path: impl AsRef<Path>) -> Result<GitRepositoryStatus> {
    let mut repo = Repository::discover(path.as_ref()).with_context(|| {
        format!(
            "could not find a git repository from {}",
            path.as_ref().display()
        )
    })?;
    let root = repository_root(&repo)?;
    let (branch, detached, head_oid) = {
        let head = repo.head().ok();
        (
            head.as_ref()
                .and_then(|head| head.shorthand().map(ToOwned::to_owned)),
            head.as_ref().map(|head| !head.is_branch()).unwrap_or(false),
            head.as_ref()
                .and_then(|head| head.target())
                .map(|oid| oid.to_string()),
        )
    };

    let mut files = {
        let mut status_options = StatusOptions::new();
        status_options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .renames_head_to_index(true)
            .renames_index_to_workdir(true);

        let statuses = repo.statuses(Some(&mut status_options))?;
        let mut files = Vec::new();
        for entry in statuses.iter() {
            let status = entry.status();
            let index = index_change(status);
            let worktree = worktree_change(status);
            let conflicted = status.is_conflicted();
            let untracked = status.contains(Status::WT_NEW) && index.is_none();
            let old_path = status_old_path(&entry);
            let path = status_path(&entry).unwrap_or_else(|| old_path.clone().unwrap_or_default());
            if path.is_empty() {
                continue;
            }
            files.push(GitFileStatus {
                path,
                old_path,
                index,
                worktree,
                staged: index.is_some(),
                unstaged: worktree.is_some() || conflicted,
                untracked,
                conflicted,
            });
        }
        files
    };
    files.sort_by_key(|file| file.path.to_ascii_lowercase());

    let branches = repository_branches_from_repo(&repo)?;
    let stashes = repository_stashes_from_repo(&mut repo)?;

    Ok(GitRepositoryStatus {
        repository_root: root.display().to_string(),
        branch,
        head: head_oid,
        detached,
        files,
        branches,
        stashes,
    })
}

pub fn repository_branches(path: impl AsRef<Path>) -> Result<Vec<GitBranch>> {
    let repo = Repository::discover(path.as_ref())?;
    repository_branches_from_repo(&repo)
}

pub fn repository_diff(path: impl AsRef<Path>, options: GitDiffOptions) -> Result<GitDiff> {
    let repo = Repository::discover(path.as_ref()).with_context(|| {
        format!(
            "could not find a git repository from {}",
            path.as_ref().display()
        )
    })?;
    let root = repository_root(&repo)?;
    let mut diff_options = diff_options_from(&options);

    let head_tree = head_tree(&repo).ok();
    let diff = match options.mode {
        GitDiffMode::Worktree => repo.diff_index_to_workdir(None, Some(&mut diff_options))?,
        GitDiffMode::Staged => {
            repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut diff_options))?
        }
        GitDiffMode::Head => {
            repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_options))?
        }
        GitDiffMode::Reference => bail!("use repository_diff_reference for reference diffs"),
    };

    build_repository_diff(&repo, root, options.mode, diff, options.max_highlight_bytes)
}

pub fn repository_diff_reference(
    path: impl AsRef<Path>,
    reference: &str,
    context_lines: u32,
    pathspecs: &[String],
    find_renames: bool,
    max_highlight_bytes: usize,
) -> Result<GitDiff> {
    let repo = Repository::discover(path.as_ref()).with_context(|| {
        format!(
            "could not find a git repository from {}",
            path.as_ref().display()
        )
    })?;
    let root = repository_root(&repo)?;
    let reference = reference.trim();
    if reference.is_empty() {
        bail!("reference is required");
    }

    let options = GitDiffOptions {
        mode: GitDiffMode::Reference,
        context_lines,
        include_untracked: false,
        pathspecs: pathspecs.to_vec(),
        max_highlight_bytes,
    };
    let mut diff_options = diff_options_from(&options);

    let mut diff = if let Some((left, right)) = reference.split_once("...") {
        let left_commit = repo.revparse_single(left.trim())?.peel_to_commit()?;
        let right_commit = repo.revparse_single(right.trim())?.peel_to_commit()?;
        let base_oid = repo.merge_base(left_commit.id(), right_commit.id())?;
        let base_tree = repo.find_commit(base_oid)?.tree()?;
        let right_tree = right_commit.tree()?;
        repo.diff_tree_to_tree(Some(&base_tree), Some(&right_tree), Some(&mut diff_options))?
    } else {
        let commit = repo.revparse_single(reference)?.peel_to_commit()?;
        let old_tree = if commit.parent_count() == 0 {
            empty_tree(&repo)?
        } else {
            commit.parent(0)?.tree()?
        };
        let new_tree = commit.tree()?;
        repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut diff_options))?
    };

    if find_renames {
        let mut find_options = DiffFindOptions::new();
        find_options.renames(true);
        diff.find_similar(Some(&mut find_options))?;
    }

    build_repository_diff(
        &repo,
        root,
        GitDiffMode::Reference,
        diff,
        max_highlight_bytes,
    )
}

pub fn repository_diff_reference_json(
    path: impl AsRef<Path>,
    reference: &str,
    context_lines: u32,
    pathspecs: &[String],
    find_renames: bool,
) -> Result<String> {
    Ok(serde_json::to_string(&repository_diff_reference(
        path,
        reference,
        context_lines,
        pathspecs,
        find_renames,
        GitDiffOptions::default().max_highlight_bytes,
    )?)?)
}

fn build_repository_diff(
    repo: &Repository,
    root: PathBuf,
    mode: GitDiffMode,
    diff: git2::Diff<'_>,
    max_highlight_bytes: usize,
) -> Result<GitDiff> {
    let state = RefCell::new(DiffBuildState::default());
    {
        let mut file_cb = |delta: git2::DiffDelta<'_>, _progress: f32| {
            state.borrow_mut().push_file(delta);
            true
        };
        let mut binary_cb = |_delta: git2::DiffDelta<'_>, _binary: git2::DiffBinary<'_>| {
            state.borrow_mut().mark_current_binary();
            true
        };
        let mut hunk_cb = |_delta: git2::DiffDelta<'_>, hunk: git2::DiffHunk<'_>| {
            state.borrow_mut().push_hunk(hunk);
            true
        };
        let mut line_cb = |_delta: git2::DiffDelta<'_>,
                           _hunk: Option<git2::DiffHunk<'_>>,
                           line: git2::DiffLine<'_>| {
            state.borrow_mut().push_line(line);
            true
        };
        diff.foreach(
            &mut file_cb,
            Some(&mut binary_cb),
            Some(&mut hunk_cb),
            Some(&mut line_cb),
        )?;
    }

    let mut files = state.into_inner().files;
    for file in &mut files {
        add_code_highlights(repo, &root, mode, file, max_highlight_bytes);
    }

    let head = repo.head().ok();
    Ok(GitDiff {
        repository_root: root.display().to_string(),
        mode,
        branch: head
            .as_ref()
            .and_then(|head| head.shorthand().map(ToOwned::to_owned)),
        head: head
            .as_ref()
            .and_then(|head| head.target())
            .map(|oid| oid.to_string()),
        files,
    })
}

pub fn repository_diff_json(path: impl AsRef<Path>, options: GitDiffOptions) -> Result<String> {
    Ok(serde_json::to_string(&repository_diff(path, options)?)?)
}

fn diff_options_from(options: &GitDiffOptions) -> DiffOptions {
    let mut diff_options = DiffOptions::new();
    diff_options
        .context_lines(options.context_lines)
        .include_untracked(options.include_untracked)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .include_typechange(true);
    for pathspec in &options.pathspecs {
        diff_options.pathspec(pathspec);
    }
    diff_options
}

pub fn stage_files(path: impl AsRef<Path>, paths: &[String]) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    ensure_paths(paths, "stage")?;
    let mut index = repo.index()?;
    for path in repo_relative_paths(&repo, paths)? {
        if repository_root(&repo)?.join(&path).exists() {
            index.add_path(&path)?;
        } else {
            let _ = index.remove_path(&path);
        }
    }
    index.write()?;
    Ok(report(format!("staged {} path(s)", paths.len())))
}

pub fn unstage_files(path: impl AsRef<Path>, paths: &[String]) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    ensure_paths(paths, "unstage")?;
    let head = repo
        .head()
        .ok()
        .and_then(|head| head.peel(ObjectType::Commit).ok());
    let pathspecs = repo_relative_strings(&repo, paths)?;
    if let Some(head) = head.as_ref() {
        repo.reset_default(Some(head), pathspecs.iter().map(String::as_str))?;
    } else {
        let mut index = repo.index()?;
        for path in repo_relative_paths(&repo, paths)? {
            let _ = index.remove_path(&path);
        }
        index.write()?;
    }
    Ok(report(format!("unstaged {} path(s)", paths.len())))
}

pub fn stage_hunk(
    path: impl AsRef<Path>,
    file_path: &str,
    hunk_id: &str,
) -> Result<GitOperationReport> {
    apply_hunk(path, GitDiffMode::Worktree, file_path, hunk_id, false)
}

pub fn unstage_hunk(
    path: impl AsRef<Path>,
    file_path: &str,
    hunk_id: &str,
) -> Result<GitOperationReport> {
    apply_hunk(path, GitDiffMode::Staged, file_path, hunk_id, true)
}

pub fn discard_file(path: impl AsRef<Path>, file_path: &str) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let rel_path = repo_relative_paths(&repo, &[file_path.to_string()])?
        .into_iter()
        .next()
        .context("file path is required")?;
    let root = repository_root(&repo)?;
    let status = repo.status_file(&rel_path).ok();
    if status
        .is_some_and(|status| status.contains(Status::WT_NEW) && index_change(status).is_none())
    {
        let absolute = root.join(&rel_path);
        if absolute.is_dir() {
            std::fs::remove_dir_all(&absolute)
                .with_context(|| format!("failed to delete {}", absolute.display()))?;
        } else if absolute.exists() {
            std::fs::remove_file(&absolute)
                .with_context(|| format!("failed to delete {}", absolute.display()))?;
        }
    } else {
        let mut checkout = CheckoutBuilder::new();
        checkout.force().path(&rel_path);
        repo.checkout_index(None, Some(&mut checkout))?;
    }
    Ok(report(format!("discarded worktree changes in {file_path}")))
}

pub fn discard_hunk(
    path: impl AsRef<Path>,
    file_path: &str,
    hunk_id: &str,
) -> Result<GitOperationReport> {
    let anchor = path.as_ref().to_path_buf();
    let repo = Repository::discover(&anchor)?;
    let patch = find_hunk_patch(&anchor, GitDiffMode::Worktree, file_path, hunk_id, 3)
        .or_else(|_| find_hunk_patch(&anchor, GitDiffMode::Worktree, file_path, hunk_id, 0))
        .with_context(|| format!("hunk {hunk_id} not found for {file_path}"))?;
    apply_patch_with_repo(&repo, &patch, GitApplyPatchMode::DiscardFromWorktree)?;
    Ok(report(format!("discarded hunk {hunk_id}")))
}

pub fn stage_line(
    path: impl AsRef<Path>,
    file_path: &str,
    line_id: &str,
) -> Result<GitOperationReport> {
    apply_line(
        path,
        GitDiffMode::Worktree,
        file_path,
        line_id,
        GitApplyPatchMode::StageToIndex,
        "staged",
    )
}

pub fn unstage_line(
    path: impl AsRef<Path>,
    file_path: &str,
    line_id: &str,
) -> Result<GitOperationReport> {
    apply_line(
        path,
        GitDiffMode::Staged,
        file_path,
        line_id,
        GitApplyPatchMode::UnstageFromIndex,
        "unstaged",
    )
}

pub fn discard_line(
    path: impl AsRef<Path>,
    file_path: &str,
    line_id: &str,
) -> Result<GitOperationReport> {
    apply_line(
        path,
        GitDiffMode::Worktree,
        file_path,
        line_id,
        GitApplyPatchMode::DiscardFromWorktree,
        "discarded",
    )
}

pub fn apply_patch(
    path: impl AsRef<Path>,
    patch: &str,
    mode: GitApplyPatchMode,
) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    apply_patch_with_repo(&repo, patch, mode)?;
    Ok(report(match mode {
        GitApplyPatchMode::StageToIndex => "applied patch to index".to_string(),
        GitApplyPatchMode::UnstageFromIndex => "unstaged patch from index".to_string(),
        GitApplyPatchMode::DiscardFromWorktree => "discarded patch from worktree".to_string(),
    }))
}

pub fn delete_files(path: impl AsRef<Path>, paths: &[String]) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    ensure_paths(paths, "delete")?;
    let root = repository_root(&repo)?;
    let mut index = repo.index()?;
    for rel_path in repo_relative_paths(&repo, paths)? {
        let absolute = root.join(&rel_path);
        if absolute.is_dir() {
            std::fs::remove_dir_all(&absolute)
                .with_context(|| format!("failed to delete {}", absolute.display()))?;
        } else if absolute.exists() {
            std::fs::remove_file(&absolute)
                .with_context(|| format!("failed to delete {}", absolute.display()))?;
        }
        let _ = index.remove_path(&rel_path);
    }
    index.write()?;
    Ok(report(format!("deleted {} path(s)", paths.len())))
}

pub fn create_branch(path: impl AsRef<Path>, name: &str) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let name = name.trim();
    if name.is_empty() {
        bail!("branch name is required");
    }
    let head = repo.head()?.peel_to_commit()?;
    repo.branch(name, &head, false)?;
    Ok(report(format!("created branch {name}")))
}

pub fn checkout_branch(path: impl AsRef<Path>, name: &str) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let name = name.trim();
    if name.is_empty() {
        bail!("branch name is required");
    }
    let (object, reference) = repo.revparse_ext(name)?;
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&object, Some(&mut checkout))?;
    if let Some(reference) = reference {
        let refname = reference
            .name()
            .with_context(|| format!("branch {name} has no reference name"))?;
        repo.set_head(refname)?;
    } else {
        repo.set_head_detached(object.id())?;
    }
    Ok(report(format!("checked out {name}")))
}

pub fn stash_push(path: impl AsRef<Path>, message: Option<&str>) -> Result<GitOperationReport> {
    let mut repo = Repository::discover(path.as_ref())?;
    let signature = repo_signature(&repo)?;
    let message = message
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .unwrap_or("Viker stash");
    let oid = repo.stash_save(
        &signature,
        message,
        Some(git2::StashFlags::INCLUDE_UNTRACKED),
    )?;
    Ok(report(format!("stashed {oid}")))
}

pub fn stash_file(
    path: impl AsRef<Path>,
    file_path: &str,
    message: Option<&str>,
) -> Result<GitOperationReport> {
    let anchor = path.as_ref().to_path_buf();
    let diff = repository_diff(
        &anchor,
        GitDiffOptions {
            mode: GitDiffMode::Worktree,
            context_lines: 3,
            pathspecs: vec![file_path.to_string()],
            ..GitDiffOptions::default()
        },
    )?;
    let patch = diff
        .files
        .iter()
        .flat_map(|file| file.hunks.iter())
        .map(|hunk| hunk.raw_patch.as_str())
        .collect::<Vec<_>>()
        .join("");
    if patch.is_empty() {
        bail!("no worktree changes found for {file_path}");
    }
    stash_patch(anchor, &patch, message)
}

pub fn stash_patch(
    path: impl AsRef<Path>,
    patch: &str,
    message: Option<&str>,
) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let head = repo.head()?.peel_to_commit()?;
    let signature = repo_signature(&repo)?;
    let patch_diff = git2::Diff::from_buffer(patch.as_bytes())?;

    let mut index = repo.index()?;
    let index_tree_id = index.write_tree()?;
    let index_tree = repo.find_tree(index_tree_id)?;
    let mut apply_options = ApplyOptions::new();
    let mut selected_index =
        repo.apply_to_tree(&index_tree, &patch_diff, Some(&mut apply_options))?;
    let selected_tree_id = selected_index.write_tree_to(&repo)?;
    let selected_tree = repo.find_tree(selected_tree_id)?;

    let index_commit_id = repo.commit(
        None,
        &signature,
        &signature,
        "index on Viker stash patch",
        &index_tree,
        &[&head],
    )?;
    let index_commit = repo.find_commit(index_commit_id)?;
    let message = normalized_stash_message(message).unwrap_or("Viker patch stash");
    let oid = repo.commit(
        None,
        &signature,
        &signature,
        message,
        &selected_tree,
        &[&head, &index_commit],
    )?;
    update_stash_ref(&repo, oid, message)?;
    apply_patch_with_repo(&repo, patch, GitApplyPatchMode::DiscardFromWorktree)?;
    Ok(report(format!("stashed patch as {oid}")))
}

pub fn stash_apply(path: impl AsRef<Path>, index: usize) -> Result<GitOperationReport> {
    let mut repo = Repository::discover(path.as_ref())?;
    repo.stash_apply(index, None)?;
    Ok(report(format!("applied stash@{{{index}}}")))
}

pub fn stash_pop(path: impl AsRef<Path>, index: usize) -> Result<GitOperationReport> {
    let mut repo = Repository::discover(path.as_ref())?;
    repo.stash_pop(index, None)?;
    Ok(report(format!("popped stash@{{{index}}}")))
}

pub fn amend(path: impl AsRef<Path>, message: Option<&str>) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let head = repo.head()?.peel_to_commit()?;
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo_signature(&repo)?;
    let message = message
        .map(str::trim)
        .filter(|message| !message.is_empty())
        .or_else(|| head.message())
        .unwrap_or("Amend");
    let oid = head.amend(
        Some("HEAD"),
        None,
        Some(&signature),
        None,
        Some(message),
        Some(&tree),
    )?;
    Ok(report(format!("amended HEAD {oid}")))
}

pub fn merge_branch(path: impl AsRef<Path>, branch: &str) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let branch = branch.trim();
    if branch.is_empty() {
        bail!("merge branch is required");
    }
    let annotated = annotated_commit(&repo, branch)?;
    let (analysis, _) = repo.merge_analysis(&[&annotated])?;
    if analysis.is_up_to_date() {
        return Ok(report(format!("already up to date with {branch}")));
    }
    if analysis.is_fast_forward() {
        fast_forward(&repo, branch, annotated.id())?;
        return Ok(report(format!("fast-forwarded to {branch}")));
    }

    repo.merge(&[&annotated], None, None)?;
    let index = repo.index()?;
    if index.has_conflicts() {
        return Ok(GitOperationReport {
            message: format!("merge started with conflicts from {branch}"),
            conflicts: index_conflicts(&index),
        });
    }
    Ok(report(format!(
        "merged {branch}; commit the merge to finish"
    )))
}

pub fn rebase_onto(path: impl AsRef<Path>, upstream: &str) -> Result<GitOperationReport> {
    let repo = Repository::discover(path.as_ref())?;
    let upstream = upstream.trim();
    if upstream.is_empty() {
        bail!("rebase upstream is required");
    }
    let upstream_commit = annotated_commit(&repo, upstream)?;
    let signature = repo_signature(&repo)?;
    let mut rebase = repo.rebase(None, Some(&upstream_commit), None, None)?;
    let mut count = 0usize;
    while let Some(operation) = rebase.next() {
        operation?;
        let index = repo.index()?;
        if index.has_conflicts() {
            return Ok(GitOperationReport {
                message: format!("rebase stopped with conflicts onto {upstream}"),
                conflicts: index_conflicts(&index),
            });
        }
        rebase.commit(None, &signature, None)?;
        count += 1;
    }
    rebase.finish(Some(&signature))?;
    Ok(report(format!("rebased {count} commit(s) onto {upstream}")))
}

pub fn list_commits(
    path: impl AsRef<Path>,
    limit: usize,
    reference: Option<&str>,
) -> Result<Vec<GitCommitSummary>> {
    let repo = Repository::discover(path.as_ref())?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    let decorations = commit_decorations(&repo)?;
    let mut revwalk = repo.revwalk()?;
    if let Some(reference) = reference
        .map(str::trim)
        .filter(|reference| !reference.is_empty())
    {
        revwalk.push(repo.revparse_single(reference)?.id())?;
    } else {
        revwalk.push_head()?;
    }
    revwalk.set_sorting(Sort::TIME)?;

    let mut commits = Vec::new();
    for oid in revwalk.take(limit) {
        let oid = oid?;
        let commit = repo.find_commit(oid)?;
        let summary = commit
            .summary()
            .or_else(|| commit.message())
            .unwrap_or("")
            .to_string();
        commits.push(GitCommitSummary {
            oid: oid.to_string(),
            short_oid: oid.to_string().chars().take(7).collect(),
            summary,
            author_name: commit.author().name().map(ToOwned::to_owned),
            author_email: commit.author().email().map(ToOwned::to_owned),
            time_seconds: commit.time().seconds(),
            decorations: decorations.get(&oid).cloned().unwrap_or_default(),
        });
    }
    Ok(commits)
}

pub fn change_snapshot(path: impl AsRef<Path>) -> Result<GitChangeSnapshot> {
    let repo = Repository::discover(path.as_ref())?;
    let root = repository_root(&repo)?;
    let head = repo
        .head()
        .ok()
        .and_then(|head| head.target())
        .map(|oid| oid.to_string());
    let index_tree = repo
        .index()
        .ok()
        .and_then(|mut index| index.write_tree().ok())
        .map(|oid| oid.to_string());

    let mut status_options = StatusOptions::new();
    status_options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    let statuses = repo.statuses(Some(&mut status_options))?;
    let mut staged_parts = Vec::new();
    let mut unstaged_parts = Vec::new();
    let mut untracked_parts = Vec::new();

    for entry in statuses.iter() {
        let status = entry.status();
        let old_path = status_old_path(&entry);
        let path = status_path(&entry).unwrap_or_else(|| old_path.clone().unwrap_or_default());
        if path.is_empty() {
            continue;
        }
        if let Some(index) = index_change(status) {
            staged_parts.push(format!("{path}:{old_path:?}:{index:?}"));
        }
        if let Some(worktree) = worktree_change(status) {
            let metadata = file_metadata_signature(&root, &path);
            unstaged_parts.push(format!("{path}:{worktree:?}:{metadata}"));
            if worktree == GitChangeKind::Untracked {
                untracked_parts.push(format!("{path}:{metadata}"));
            }
        }
        if status.is_conflicted() {
            unstaged_parts.push(format!("{path}:conflicted"));
        }
    }
    staged_parts.sort();
    unstaged_parts.sort();
    untracked_parts.sort();
    let staged_signature = stable_hash_hex(&staged_parts.join("\n"));
    let unstaged_signature = stable_hash_hex(&unstaged_parts.join("\n"));
    let untracked_signature = stable_hash_hex(&untracked_parts.join("\n"));
    let status_signature = stable_hash_hex(&format!(
        "head={head:?}\nindex={index_tree:?}\nstaged={staged_signature}\nunstaged={unstaged_signature}\nuntracked={untracked_signature}"
    ));

    Ok(GitChangeSnapshot {
        repository_root: root.display().to_string(),
        head,
        index_tree,
        staged_signature,
        unstaged_signature,
        untracked_signature,
        status_signature,
    })
}

fn apply_hunk(
    path: impl AsRef<Path>,
    mode: GitDiffMode,
    file_path: &str,
    hunk_id: &str,
    reverse: bool,
) -> Result<GitOperationReport> {
    let anchor = path.as_ref().to_path_buf();
    let repo = Repository::discover(&anchor)?;
    let patch = find_hunk_patch(&anchor, mode, file_path, hunk_id, 3)
        .or_else(|_| find_hunk_patch(&anchor, mode, file_path, hunk_id, 0))
        .with_context(|| format!("hunk {hunk_id} not found for {file_path}"))?;
    let mut apply_options = ApplyOptions::new();
    let patch = if reverse {
        reverse_patch(&patch)
    } else {
        patch
    };
    let patch_diff = git2::Diff::from_buffer(patch.as_bytes())?;
    repo.apply(&patch_diff, ApplyLocation::Index, Some(&mut apply_options))?;
    Ok(report(if reverse {
        format!("unstaged hunk {hunk_id}")
    } else {
        format!("staged hunk {hunk_id}")
    }))
}

fn apply_line(
    path: impl AsRef<Path>,
    mode: GitDiffMode,
    file_path: &str,
    line_id: &str,
    patch_mode: GitApplyPatchMode,
    verb: &str,
) -> Result<GitOperationReport> {
    let anchor = path.as_ref().to_path_buf();
    let repo = Repository::discover(&anchor)?;
    let patch = find_line_patch(&anchor, mode, file_path, line_id, 3)
        .or_else(|_| find_line_patch(&anchor, mode, file_path, line_id, 0))
        .with_context(|| format!("line {line_id} not found for {file_path}"))?;
    apply_patch_with_repo(&repo, &patch, patch_mode)?;
    Ok(report(format!("{verb} line {line_id}")))
}

fn apply_patch_with_repo(repo: &Repository, patch: &str, mode: GitApplyPatchMode) -> Result<()> {
    let (patch, location) = match mode {
        GitApplyPatchMode::StageToIndex => (patch.to_string(), ApplyLocation::Index),
        GitApplyPatchMode::UnstageFromIndex => (reverse_patch(patch), ApplyLocation::Index),
        GitApplyPatchMode::DiscardFromWorktree => (reverse_patch(patch), ApplyLocation::WorkDir),
    };
    let patch_diff = git2::Diff::from_buffer(patch.as_bytes())?;
    let mut apply_options = ApplyOptions::new();
    repo.apply(&patch_diff, location, Some(&mut apply_options))?;
    Ok(())
}

fn find_hunk_patch(
    path: impl AsRef<Path>,
    mode: GitDiffMode,
    file_path: &str,
    hunk_id: &str,
    context_lines: u32,
) -> Result<String> {
    let diff = repository_diff(
        path,
        GitDiffOptions {
            mode,
            context_lines,
            pathspecs: vec![file_path.to_string()],
            ..GitDiffOptions::default()
        },
    )?;
    diff.files
        .iter()
        .find(|file| {
            file.new_path.as_deref() == Some(file_path)
                || file.old_path.as_deref() == Some(file_path)
        })
        .and_then(|file| file.hunks.iter().find(|hunk| hunk.id == hunk_id))
        .map(|hunk| hunk.raw_patch.clone())
        .with_context(|| format!("hunk {hunk_id} not found"))
}

fn find_line_patch(
    path: impl AsRef<Path>,
    mode: GitDiffMode,
    file_path: &str,
    line_id: &str,
    context_lines: u32,
) -> Result<String> {
    let diff = repository_diff(
        path,
        GitDiffOptions {
            mode,
            context_lines,
            pathspecs: vec![file_path.to_string()],
            ..GitDiffOptions::default()
        },
    )?;
    diff.files
        .iter()
        .find(|file| {
            file.new_path.as_deref() == Some(file_path)
                || file.old_path.as_deref() == Some(file_path)
        })
        .and_then(|file| {
            file.hunks.iter().find_map(|hunk| {
                hunk.lines
                    .iter()
                    .position(|line| line.id == line_id)
                    .and_then(|line_idx| single_line_patch(file, hunk, line_idx).ok())
            })
        })
        .with_context(|| format!("line {line_id} not found"))
}

fn single_line_patch(file: &GitFileDiff, hunk: &GitDiffHunk, line_idx: usize) -> Result<String> {
    let line = hunk
        .lines
        .get(line_idx)
        .context("line index is outside the hunk")?;
    if !matches!(line.kind, GitLineKind::Addition | GitLineKind::Deletion) {
        bail!("line {} is not an addition or deletion", line.id);
    }

    let mut old_anchor = hunk.old_start.saturating_sub(1);
    let mut new_anchor = hunk.new_start.saturating_sub(1);
    for prior in &hunk.lines[..line_idx] {
        if let Some(old_line) = prior.old_line {
            old_anchor = old_line;
        }
        if let Some(new_line) = prior.new_line {
            new_anchor = new_line;
        }
    }

    let included = hunk
        .lines
        .iter()
        .enumerate()
        .filter(|(index, candidate)| {
            *index == line_idx || matches!(candidate.kind, GitLineKind::Context)
        })
        .map(|(_, line)| line)
        .collect::<Vec<_>>();

    let old_start = included
        .iter()
        .find_map(|line| line.old_line)
        .unwrap_or(old_anchor);
    let new_start = included
        .iter()
        .find_map(|line| line.new_line)
        .unwrap_or(new_anchor);
    let old_count = included
        .iter()
        .filter(|line| line.old_line.is_some())
        .count() as u32;
    let new_count = included
        .iter()
        .filter(|line| line.new_line.is_some())
        .count() as u32;

    let mut patch = file_patch_header(file);
    patch.push_str(&format!(
        "@@ {} {} @@{}\n",
        unified_range('-', old_start, old_count),
        unified_range('+', new_start, new_count),
        hunk_header_suffix(&hunk.header)
    ));
    for line in included {
        match line.kind {
            GitLineKind::Context => patch.push(' '),
            GitLineKind::Addition => patch.push('+'),
            GitLineKind::Deletion => patch.push('-'),
            GitLineKind::Other => continue,
        }
        patch.push_str(&line.content);
        patch.push('\n');
    }
    Ok(patch)
}

fn unified_range(sign: char, start: u32, count: u32) -> String {
    if count == 1 {
        format!("{sign}{start}")
    } else {
        format!("{sign}{start},{count}")
    }
}

fn hunk_header_suffix(header: &str) -> &str {
    header
        .match_indices("@@")
        .nth(1)
        .map(|(index, _)| &header[index + 2..])
        .unwrap_or("")
}

#[derive(Default)]
struct DiffBuildState {
    files: Vec<GitFileDiff>,
    current_file: Option<usize>,
    current_hunk: Option<usize>,
}

impl DiffBuildState {
    fn push_file(&mut self, delta: git2::DiffDelta<'_>) {
        let old_path = diff_path(delta.old_file().path());
        let new_path = diff_path(delta.new_file().path());
        self.files.push(GitFileDiff {
            old_path,
            new_path,
            change: change_from_delta(delta.status()),
            binary: false,
            hunks: Vec::new(),
            old_blob_id: blob_id(delta.old_file().id()),
            new_blob_id: blob_id(delta.new_file().id()),
        });
        self.current_file = Some(self.files.len() - 1);
        self.current_hunk = None;
    }

    fn mark_current_binary(&mut self) {
        if let Some(file_idx) = self.current_file {
            self.files[file_idx].binary = true;
        }
    }

    fn push_hunk(&mut self, hunk: git2::DiffHunk<'_>) {
        let Some(file_idx) = self.current_file else {
            return;
        };
        let header = String::from_utf8_lossy(hunk.header())
            .trim_end()
            .to_string();
        let hunk_idx = self.files[file_idx].hunks.len();
        let id = hunk_id(
            &self.files[file_idx],
            hunk_idx,
            hunk.old_start(),
            hunk.old_lines(),
            hunk.new_start(),
            hunk.new_lines(),
        );
        let mut patch = file_patch_header(&self.files[file_idx]);
        patch.push_str(&header);
        patch.push('\n');
        self.files[file_idx].hunks.push(GitDiffHunk {
            id,
            header,
            old_start: hunk.old_start(),
            old_lines: hunk.old_lines(),
            new_start: hunk.new_start(),
            new_lines: hunk.new_lines(),
            lines: Vec::new(),
            raw_patch: patch,
        });
        self.current_hunk = Some(hunk_idx);
    }

    fn push_line(&mut self, line: git2::DiffLine<'_>) {
        let (Some(file_idx), Some(hunk_idx)) = (self.current_file, self.current_hunk) else {
            return;
        };
        let origin = line.origin();
        let raw = String::from_utf8_lossy(line.content()).to_string();
        let content = raw.trim_end_matches(['\n', '\r']).to_string();
        let kind = match origin {
            '+' => GitLineKind::Addition,
            '-' => GitLineKind::Deletion,
            ' ' => GitLineKind::Context,
            _ => GitLineKind::Other,
        };
        let path = self.files[file_idx]
            .new_path
            .as_deref()
            .or(self.files[file_idx].old_path.as_deref())
            .unwrap_or("<unknown>")
            .to_string();
        let id = line_id(&path, line.old_lineno(), line.new_lineno(), &kind, &content);
        let hunk = &mut self.files[file_idx].hunks[hunk_idx];
        hunk.raw_patch.push(origin);
        hunk.raw_patch.push_str(&raw);
        if !raw.ends_with('\n') {
            hunk.raw_patch.push('\n');
        }
        hunk.lines.push(GitDiffLine {
            id,
            old_line: line.old_lineno(),
            new_line: line.new_lineno(),
            kind,
            prefix: origin.to_string(),
            content,
            highlights: Vec::new(),
        });
    }
}

fn add_code_highlights(
    repo: &Repository,
    root: &Path,
    mode: GitDiffMode,
    file: &mut GitFileDiff,
    max_highlight_bytes: usize,
) {
    if file.binary {
        return;
    }
    let path = file
        .new_path
        .as_deref()
        .or(file.old_path.as_deref())
        .map(Path::new);
    let Some(language) = SyntaxLanguage::from_path(path) else {
        return;
    };
    let old_text = side_text(
        repo,
        root,
        file.old_blob_id,
        file.old_path.as_deref(),
        max_highlight_bytes,
    );
    let new_text = match mode {
        GitDiffMode::Staged | GitDiffMode::Reference => side_text(
            repo,
            root,
            file.new_blob_id,
            file.new_path.as_deref(),
            max_highlight_bytes,
        ),
        GitDiffMode::Worktree | GitDiffMode::Head => side_text(
            repo,
            root,
            None,
            file.new_path.as_deref(),
            max_highlight_bytes,
        )
        .or_else(|| {
            side_text(
                repo,
                root,
                file.new_blob_id,
                file.new_path.as_deref(),
                max_highlight_bytes,
            )
        }),
    };
    let old_styles = old_text
        .as_deref()
        .and_then(|text| highlighted_lines(language, text));
    let new_styles = new_text
        .as_deref()
        .and_then(|text| highlighted_lines(language, text));

    for hunk in &mut file.hunks {
        for line in &mut hunk.lines {
            let styles = match line.kind {
                GitLineKind::Deletion => line.old_line.and_then(|line_no| {
                    old_styles
                        .as_ref()
                        .and_then(|styles| styles.get((line_no - 1) as usize))
                }),
                GitLineKind::Addition | GitLineKind::Context => line.new_line.and_then(|line_no| {
                    new_styles
                        .as_ref()
                        .and_then(|styles| styles.get((line_no - 1) as usize))
                }),
                GitLineKind::Other => None,
            };
            if let Some(styles) = styles {
                line.highlights = styles.clone();
            }
        }
    }
}

fn highlighted_lines(language: SyntaxLanguage, text: &str) -> Option<Vec<Vec<GitPatchHighlight>>> {
    let mut highlighter = Highlighter::new(language)?;
    let rope = Rope::from_str(text);
    let tree = highlighter.parse(&rope, None)?;
    let line_styles = highlighter.highlight_lines(&tree, &rope, 0, rope.len_lines());
    Some(
        line_styles
            .into_iter()
            .map(|line| {
                line.into_iter()
                    .map(|(start_column, end_column, highlight)| GitPatchHighlight {
                        start_column,
                        end_column,
                        token: highlight.token,
                        style: highlight.style,
                    })
                    .collect()
            })
            .collect(),
    )
}

fn side_text(
    repo: &Repository,
    root: &Path,
    blob_id: Option<Oid>,
    path: Option<&str>,
    max_highlight_bytes: usize,
) -> Option<String> {
    if let Some(blob_id) = blob_id {
        let blob = repo.find_blob(blob_id).ok()?;
        if blob.size() > max_highlight_bytes {
            return None;
        }
        return std::str::from_utf8(blob.content())
            .ok()
            .map(ToOwned::to_owned);
    }
    let path = path?;
    let absolute = root.join(path);
    let metadata = std::fs::metadata(&absolute).ok()?;
    if metadata.len() as usize > max_highlight_bytes {
        return None;
    }
    std::fs::read_to_string(absolute).ok()
}

fn repository_branches_from_repo(repo: &Repository) -> Result<Vec<GitBranch>> {
    let mut branches = Vec::new();
    for branch in repo.branches(None)? {
        let (branch, branch_type) = branch?;
        let Some(name) = branch.name()?.map(ToOwned::to_owned) else {
            continue;
        };
        let upstream = branch
            .upstream()
            .ok()
            .and_then(|upstream| upstream.name().ok().flatten().map(ToOwned::to_owned));
        branches.push(GitBranch {
            name,
            is_current: branch.is_head(),
            is_remote: branch_type == BranchType::Remote,
            upstream,
            target: branch.get().target().map(|oid| oid.to_string()),
        });
    }
    branches.sort_by(|left, right| {
        right.is_current.cmp(&left.is_current).then_with(|| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        })
    });
    Ok(branches)
}

fn repository_stashes_from_repo(repo: &mut Repository) -> Result<Vec<GitStash>> {
    let mut stashes = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        stashes.push(GitStash {
            index,
            message: message.to_string(),
            oid: oid.to_string(),
        });
        true
    })?;
    Ok(stashes)
}

fn head_tree(repo: &Repository) -> Result<git2::Tree<'_>> {
    Ok(repo.head()?.peel_to_tree()?)
}

fn empty_tree(repo: &Repository) -> Result<git2::Tree<'_>> {
    let builder = repo.treebuilder(None)?;
    let oid = builder.write()?;
    Ok(repo.find_tree(oid)?)
}

fn repository_root(repo: &Repository) -> Result<PathBuf> {
    repo.workdir()
        .map(Path::to_path_buf)
        .or_else(|| repo.path().parent().map(Path::to_path_buf))
        .context("repository has no workdir")
}

fn status_path(entry: &git2::StatusEntry<'_>) -> Option<String> {
    entry
        .head_to_index()
        .and_then(|delta| diff_path(delta.new_file().path()))
        .or_else(|| {
            entry
                .index_to_workdir()
                .and_then(|delta| diff_path(delta.new_file().path()))
        })
        .or_else(|| entry.path().map(normalize_status_path))
}

fn status_old_path(entry: &git2::StatusEntry<'_>) -> Option<String> {
    entry
        .head_to_index()
        .and_then(|delta| diff_path(delta.old_file().path()))
        .or_else(|| {
            entry
                .index_to_workdir()
                .and_then(|delta| diff_path(delta.old_file().path()))
        })
}

fn normalize_status_path(path: &str) -> String {
    path.trim_matches('"').replace('\\', "/")
}

fn index_change(status: Status) -> Option<GitChangeKind> {
    if status.contains(Status::INDEX_NEW) {
        Some(GitChangeKind::Added)
    } else if status.contains(Status::INDEX_MODIFIED) {
        Some(GitChangeKind::Modified)
    } else if status.contains(Status::INDEX_DELETED) {
        Some(GitChangeKind::Deleted)
    } else if status.contains(Status::INDEX_RENAMED) {
        Some(GitChangeKind::Renamed)
    } else if status.contains(Status::INDEX_TYPECHANGE) {
        Some(GitChangeKind::Typechange)
    } else if status.is_conflicted() {
        Some(GitChangeKind::Conflicted)
    } else {
        None
    }
}

fn worktree_change(status: Status) -> Option<GitChangeKind> {
    if status.contains(Status::WT_NEW) {
        Some(GitChangeKind::Untracked)
    } else if status.contains(Status::WT_MODIFIED) {
        Some(GitChangeKind::Modified)
    } else if status.contains(Status::WT_DELETED) {
        Some(GitChangeKind::Deleted)
    } else if status.contains(Status::WT_RENAMED) {
        Some(GitChangeKind::Renamed)
    } else if status.contains(Status::WT_TYPECHANGE) {
        Some(GitChangeKind::Typechange)
    } else if status.is_conflicted() {
        Some(GitChangeKind::Conflicted)
    } else {
        None
    }
}

fn change_from_delta(delta: Delta) -> GitChangeKind {
    match delta {
        Delta::Added => GitChangeKind::Added,
        Delta::Modified => GitChangeKind::Modified,
        Delta::Deleted => GitChangeKind::Deleted,
        Delta::Renamed => GitChangeKind::Renamed,
        Delta::Copied => GitChangeKind::Copied,
        Delta::Typechange => GitChangeKind::Typechange,
        Delta::Untracked => GitChangeKind::Untracked,
        Delta::Conflicted => GitChangeKind::Conflicted,
        _ => GitChangeKind::Unknown,
    }
}

fn diff_path(path: Option<&Path>) -> Option<String> {
    path.map(|path| {
        path.components()
            .filter_map(|component| match component {
                std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/")
    })
    .filter(|path| !path.is_empty())
}

fn blob_id(oid: Oid) -> Option<Oid> {
    (oid != Oid::zero()).then_some(oid)
}

fn hunk_id(
    file: &GitFileDiff,
    hunk_idx: usize,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
) -> String {
    let path = file
        .new_path
        .as_deref()
        .or(file.old_path.as_deref())
        .unwrap_or("<unknown>");
    format!("{path}:{hunk_idx}:{old_start},{old_lines}:{new_start},{new_lines}")
}

fn line_id(
    path: &str,
    old_line: Option<u32>,
    new_line: Option<u32>,
    kind: &GitLineKind,
    content: &str,
) -> String {
    format!(
        "{path}:line:{kind:?}:{}:{}:{}",
        old_line.unwrap_or(0),
        new_line.unwrap_or(0),
        stable_hash_hex(content)
    )
}

fn file_patch_header(file: &GitFileDiff) -> String {
    let old_path = file.old_path.as_deref().unwrap_or("/dev/null");
    let new_path = file.new_path.as_deref().unwrap_or("/dev/null");
    let old_display = if matches!(file.change, GitChangeKind::Added | GitChangeKind::Untracked) {
        "/dev/null".to_string()
    } else {
        format!("a/{old_path}")
    };
    let new_display = if matches!(file.change, GitChangeKind::Deleted) {
        "/dev/null".to_string()
    } else {
        format!("b/{new_path}")
    };
    format!("diff --git a/{old_path} b/{new_path}\n--- {old_display}\n+++ {new_display}\n")
}

fn commit_decorations(repo: &Repository) -> Result<HashMap<Oid, Vec<String>>> {
    let mut decorations: HashMap<Oid, Vec<String>> = HashMap::new();
    for reference in repo.references()? {
        let reference = reference?;
        let Some(name) = reference.shorthand().map(ToOwned::to_owned) else {
            continue;
        };
        let target = reference.target().or_else(|| {
            reference
                .peel(ObjectType::Commit)
                .ok()
                .map(|object| object.id())
        });
        if let Some(target) = target {
            decorations.entry(target).or_default().push(name);
        }
    }
    for values in decorations.values_mut() {
        values.sort();
        values.dedup();
    }
    Ok(decorations)
}

fn file_metadata_signature(root: &Path, path: &str) -> String {
    let Ok(metadata) = std::fs::metadata(root.join(path)) else {
        return "missing".to_string();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok());
    match modified {
        Some(modified) => format!(
            "len={}:mtime={}.{}",
            metadata.len(),
            modified.as_secs(),
            modified.subsec_nanos()
        ),
        None => format!("len={}:mtime=unknown", metadata.len()),
    }
}

fn normalized_stash_message(message: Option<&str>) -> Option<&str> {
    message.map(str::trim).filter(|message| !message.is_empty())
}

fn update_stash_ref(repo: &Repository, oid: Oid, message: &str) -> Result<()> {
    repo.reference_ensure_log("refs/stash")?;
    repo.reference("refs/stash", oid, true, message)?;
    Ok(())
}

fn stable_hash_hex(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn reverse_patch(patch: &str) -> String {
    let lines = patch.lines().collect::<Vec<_>>();
    let mut reversed = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            if parts.len() == 2 {
                reversed.push(format!("diff --git {} {}", parts[1], parts[0]));
                index += 1;
                continue;
            }
        }
        if let (Some(old_path), Some(next)) = (line.strip_prefix("--- "), lines.get(index + 1))
            && let Some(new_path) = next.strip_prefix("+++ ")
        {
            reversed.push(format!("--- {new_path}"));
            reversed.push(format!("+++ {old_path}"));
            index += 2;
            continue;
        }
        if let Some(header) = reverse_hunk_header(line) {
            reversed.push(header);
        } else if let Some(rest) = line.strip_prefix('+') {
            reversed.push(format!("-{rest}"));
        } else if let Some(rest) = line.strip_prefix('-') {
            reversed.push(format!("+{rest}"));
        } else {
            reversed.push(line.to_string());
        }
        index += 1;
    }
    reversed.join("\n") + "\n"
}

fn reverse_hunk_header(line: &str) -> Option<String> {
    let rest = line.strip_prefix("@@ -")?;
    let (old_range, rest) = rest.split_once(' ')?;
    let rest = rest.strip_prefix('+')?;
    let (new_range, suffix) = rest.split_once(" @@")?;
    Some(format!("@@ -{new_range} +{old_range} @@{suffix}"))
}

fn ensure_paths(paths: &[String], verb: &str) -> Result<()> {
    if paths.is_empty() {
        bail!("no paths supplied to {verb}");
    }
    Ok(())
}

fn repo_relative_strings(repo: &Repository, paths: &[String]) -> Result<Vec<String>> {
    repo_relative_paths(repo, paths).map(|paths| {
        paths
            .into_iter()
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .collect()
    })
}

fn repo_relative_paths(repo: &Repository, paths: &[String]) -> Result<Vec<PathBuf>> {
    let root = repository_root(repo)?;
    paths
        .iter()
        .map(|path| {
            let path = Path::new(path);
            let rel = if path.is_absolute() {
                path.strip_prefix(&root)
                    .with_context(|| format!("{} is outside {}", path.display(), root.display()))?
                    .to_path_buf()
            } else {
                path.to_path_buf()
            };
            if rel
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                bail!("path {} escapes the repository", path.display());
            }
            Ok(rel)
        })
        .collect()
}

fn repo_signature(_repo: &Repository) -> Result<Signature<'static>> {
    Signature::now("Viker", "viker@example.invalid").map_err(Into::into)
}

fn annotated_commit<'repo>(
    repo: &'repo Repository,
    rev: &str,
) -> Result<git2::AnnotatedCommit<'repo>> {
    if let Ok(reference) = repo.find_reference(&format!("refs/heads/{rev}"))
        && let Some(oid) = reference.target()
    {
        return Ok(repo.find_annotated_commit(oid)?);
    }
    let object = repo.revparse_single(rev)?;
    Ok(repo.find_annotated_commit(object.id())?)
}

fn fast_forward(repo: &Repository, branch: &str, target: Oid) -> Result<()> {
    let head = repo.head()?;
    if head.is_branch() {
        let refname = head.name().context("current HEAD has no reference name")?;
        let mut reference = repo.find_reference(refname)?;
        reference.set_target(target, &format!("fast-forward to {branch}"))?;
        repo.set_head(refname)?;
    } else {
        repo.set_head_detached(target)?;
    }
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))?;
    Ok(())
}

fn index_conflicts(index: &git2::Index) -> Vec<String> {
    let Ok(conflicts) = index.conflicts() else {
        return Vec::new();
    };
    conflicts
        .filter_map(|conflict| conflict.ok())
        .filter_map(|conflict| {
            conflict
                .our
                .or(conflict.their)
                .or(conflict.ancestor)
                .map(|entry| String::from_utf8_lossy(&entry.path).replace('\\', "/"))
        })
        .collect()
}

fn report(message: String) -> GitOperationReport {
    GitOperationReport {
        message,
        conflicts: Vec::new(),
    }
}
