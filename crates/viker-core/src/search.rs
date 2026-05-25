use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use skim::fuzzy_matcher::FuzzyMatcher;
use skim::prelude::SkimMatcherV2;

const DEFAULT_CONTENT_MAX_BYTES: u64 = 2_000_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchResult {
    pub path: String,
    pub score: i64,
    pub matched_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContentSearchResult {
    pub path: String,
    pub row: usize,
    pub column: usize,
    pub text: String,
    pub score: i64,
    pub matched_indices: Vec<usize>,
}

pub fn scan_project_files(root: impl AsRef<Path>) -> Vec<String> {
    let root = root.as_ref();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .ignore(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .filter_entry(|entry| entry.file_name().to_string_lossy() != ".git");

    let mut files = Vec::new();
    for result in builder.build() {
        let Ok(entry) = result else {
            continue;
        };
        let path = entry.path();
        if path == root || !path.is_file() {
            continue;
        }
        if let Ok(rel) = path.strip_prefix(root) {
            let rel = normalize_rel_path(rel);
            if !rel.is_empty() {
                files.push(rel);
            }
        }
    }
    files.sort_by_key(|path| path.to_ascii_lowercase());
    files
}

pub fn search_project_files(
    root: impl AsRef<Path>,
    query: &str,
    limit: usize,
) -> Vec<FileSearchResult> {
    let files = scan_project_files(root);
    filter_file_paths(&files, query, limit)
}

pub fn filter_file_paths(entries: &[String], query: &str, limit: usize) -> Vec<FileSearchResult> {
    let query = query.trim();
    let mut results: Vec<FileSearchResult> = if query.is_empty() {
        entries
            .iter()
            .map(|path| FileSearchResult {
                path: path.clone(),
                score: 0,
                matched_indices: Vec::new(),
            })
            .collect()
    } else {
        let matcher = SkimMatcherV2::default();
        entries
            .iter()
            .filter_map(|path| {
                matcher
                    .fuzzy_indices(path, query)
                    .map(|(score, matched_indices)| FileSearchResult {
                        path: path.clone(),
                        score,
                        matched_indices,
                    })
            })
            .collect()
    };
    sort_file_results(&mut results);
    truncate_limit(&mut results, limit);
    results
}

pub fn search_file_contents(
    root: impl AsRef<Path>,
    query: &str,
    limit: usize,
) -> Vec<FileContentSearchResult> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let root = root.as_ref();
    let matcher = SkimMatcherV2::default();
    let mut results = Vec::new();
    for rel_path in scan_project_files(root) {
        let full_path = root.join(&rel_path);
        if should_skip_content_search(&full_path) {
            continue;
        }
        let Ok(file) = File::open(&full_path) else {
            continue;
        };
        let reader = BufReader::new(file);
        for (row, line) in reader.lines().enumerate() {
            let Ok(text) = line else {
                break;
            };
            let Some((score, matched_indices)) = matcher.fuzzy_indices(&text, query) else {
                continue;
            };
            let column = matched_indices.first().copied().unwrap_or(0);
            results.push(FileContentSearchResult {
                path: rel_path.clone(),
                row,
                column,
                text,
                score,
                matched_indices,
            });
        }
    }
    sort_content_results(&mut results);
    truncate_limit(&mut results, limit);
    results
}

fn should_skip_content_search(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata.len() > DEFAULT_CONTENT_MAX_BYTES)
        .unwrap_or(true)
}

fn sort_file_results(results: &mut [FileSearchResult]) {
    results.sort_by(|a, b| {
        b.score.cmp(&a.score).then_with(|| {
            a.path
                .to_ascii_lowercase()
                .cmp(&b.path.to_ascii_lowercase())
        })
    });
}

fn sort_content_results(results: &mut [FileContentSearchResult]) {
    results.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| {
                a.path
                    .to_ascii_lowercase()
                    .cmp(&b.path.to_ascii_lowercase())
            })
            .then_with(|| a.row.cmp(&b.row))
            .then_with(|| a.column.cmp(&b.column))
    });
}

fn truncate_limit<T>(results: &mut Vec<T>, limit: usize) {
    if limit > 0 && results.len() > limit {
        results.truncate(limit);
    }
}

fn normalize_rel_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
