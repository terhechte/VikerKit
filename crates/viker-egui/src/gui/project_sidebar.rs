use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::{Color32, FontId, RichText, Stroke, TextEdit};
use viker_core::{git, search};

const SEARCH_ID: &str = "viker_project_sidebar_search";
const SIDEBAR_BG: Color32 = Color32::from_rgb(34, 37, 43);
const SIDEBAR_HEADER: Color32 = Color32::from_rgb(42, 46, 54);
const SIDEBAR_BORDER: Color32 = Color32::from_rgb(58, 63, 73);
const TEXT: Color32 = Color32::from_rgb(210, 214, 222);
const MUTED: Color32 = Color32::from_rgb(137, 143, 153);
const ACCENT: Color32 = Color32::from_rgb(91, 143, 249);
const MODIFIED: Color32 = Color32::from_rgb(235, 196, 93);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectFile {
    pub rel_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectTreeNode {
    pub name: String,
    pub rel_path: String,
    pub is_file: bool,
    pub children: Vec<ProjectTreeNode>,
}

#[derive(Debug, Default)]
struct BuildNode {
    dirs: BTreeMap<String, BuildNode>,
    files: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(crate) struct ProjectSidebar {
    pub visible: bool,
    search: String,
    modified_only: bool,
    recent_only: bool,
    files: Vec<ProjectFile>,
    modified_files: HashSet<String>,
    recent_files: Vec<String>,
    root: Option<PathBuf>,
    last_git_refresh: Option<Instant>,
}

impl Default for ProjectSidebar {
    fn default() -> Self {
        Self {
            visible: true,
            search: String::new(),
            modified_only: false,
            recent_only: false,
            files: Vec::new(),
            modified_files: HashSet::new(),
            recent_files: Vec::new(),
            root: None,
            last_git_refresh: None,
        }
    }
}

impl ProjectSidebar {
    pub fn refresh(&mut self, root: &Path) {
        self.root = Some(root.to_path_buf());
        self.files = scan_project_files(root)
            .into_iter()
            .map(|rel_path| ProjectFile { rel_path })
            .collect();
        self.refresh_git_status(root);
    }

    pub fn refresh_git_status(&mut self, root: &Path) {
        self.modified_files = git_modified_files(root);
        self.last_git_refresh = Some(Instant::now());
    }

    pub fn refresh_git_status_if_stale(&mut self, root: &Path, max_age: Duration) {
        let stale = self
            .last_git_refresh
            .is_none_or(|last_refresh| last_refresh.elapsed() >= max_age);
        if stale {
            self.refresh_git_status(root);
        }
    }

    pub fn note_opened(&mut self, root: &Path, path: &Path) {
        let rel = match path.strip_prefix(root) {
            Ok(rel) => normalize_rel_path(rel),
            Err(_) => return,
        };
        if rel.is_empty() {
            return;
        }
        self.recent_files.retain(|existing| existing != &rel);
        self.recent_files.insert(0, rel);
        self.recent_files.truncate(40);
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn filtered_tree(&self) -> ProjectTreeNode {
        build_tree(self.filtered_files())
    }

    fn filtered_files(&self) -> Vec<&ProjectFile> {
        let recent: HashSet<&str> = self.recent_files.iter().map(String::as_str).collect();
        let candidates: Vec<&ProjectFile> = self
            .files
            .iter()
            .filter(|file| {
                (!self.modified_only || self.modified_files.contains(&file.rel_path))
                    && (!self.recent_only || recent.contains(file.rel_path.as_str()))
            })
            .collect();

        let query = self.search.trim();
        if query.is_empty() {
            return candidates;
        }

        let paths: Vec<String> = candidates
            .iter()
            .map(|file| file.rel_path.clone())
            .collect();
        let matches: HashSet<String> = search::filter_file_paths(&paths, query, 0)
            .into_iter()
            .map(|result| result.path)
            .collect();
        candidates
            .into_iter()
            .filter(|file| matches.contains(&file.rel_path))
            .collect()
    }
}

pub(crate) fn project_relative_path(root: &Path, path: &Path) -> Option<String> {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    path.strip_prefix(root).ok().map(normalize_rel_path)
}

pub(crate) fn draw_toolbar(
    ui: &mut egui::Ui,
    sidebar: &mut ProjectSidebar,
    project_root: Option<&Path>,
) {
    let project_name = project_root
        .and_then(|root| root.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("No Folder");

    egui::Frame::new()
        .fill(SIDEBAR_HEADER)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let toggle_text = if sidebar.visible { "Files" } else { "Show" };
                if ui
                    .small_button(toggle_text)
                    .on_hover_text("Toggle project files sidebar")
                    .clicked()
                {
                    sidebar.visible = !sidebar.visible;
                }
                ui.separator();
                ui.label(RichText::new(project_name).color(MUTED).size(12.0));
            });
        });
}

pub(crate) fn draw_sidebar(
    ui: &mut egui::Ui,
    sidebar: &mut ProjectSidebar,
    project_root: &Path,
    active_rel_path: Option<&str>,
) -> Option<String> {
    let mut open_file = None;
    egui::Frame::new()
        .fill(SIDEBAR_BG)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            let title = project_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Project");
            ui.horizontal(|ui| {
                ui.label(RichText::new(title).strong().color(TEXT).size(13.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{}", sidebar.files.len()))
                            .monospace()
                            .color(MUTED)
                            .size(11.0),
                    );
                });
            });

            ui.add_space(5.0);

            let bottom_height = 58.0;
            let tree_height = (ui.available_height() - bottom_height).max(0.0);
            egui::ScrollArea::vertical()
                .id_salt("project_sidebar_tree")
                .max_height(tree_height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let tree = sidebar.filtered_tree();
                    if tree.children.is_empty() {
                        ui.add_space(12.0);
                        ui.centered_and_justified(|ui| {
                            ui.label(RichText::new("No files").color(MUTED).size(12.0));
                        });
                    } else {
                        for child in &tree.children {
                            if let Some(path) = draw_node(ui, child, sidebar, active_rel_path, 0) {
                                open_file = Some(path);
                            }
                        }
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                filter_button(
                    ui,
                    &mut sidebar.modified_only,
                    "M",
                    "Show git modified files only",
                );
                filter_button(
                    ui,
                    &mut sidebar.recent_only,
                    "R",
                    "Show recently opened files only",
                );
                if ui
                    .small_button("Refresh")
                    .on_hover_text("Refresh project files and git status")
                    .clicked()
                {
                    sidebar.refresh(project_root);
                }
            });
            ui.add(
                TextEdit::singleline(&mut sidebar.search)
                    .id(egui::Id::new(SEARCH_ID))
                    .hint_text("Search")
                    .desired_width(f32::INFINITY),
            );
        });

    if open_file.is_some() {
        ui.memory_mut(|mem| {
            mem.surrender_focus(egui::Id::new(SEARCH_ID));
            mem.stop_text_input();
        });
    }

    open_file
}

fn draw_node(
    ui: &mut egui::Ui,
    node: &ProjectTreeNode,
    sidebar: &ProjectSidebar,
    active_rel_path: Option<&str>,
    depth: usize,
) -> Option<String> {
    if node.is_file {
        let selected = active_rel_path == Some(node.rel_path.as_str());
        let modified = sidebar.modified_files.contains(&node.rel_path);
        let recent = sidebar
            .recent_files
            .iter()
            .any(|path| path == &node.rel_path);
        let prefix = if modified {
            "M "
        } else if recent {
            "R "
        } else {
            "  "
        };
        let color = if modified { MODIFIED } else { TEXT };
        let label = RichText::new(format!("{prefix}{}", node.name))
            .font(FontId::proportional(12.0))
            .color(color);
        let response = ui
            .selectable_label(selected, label)
            .on_hover_text(node.rel_path.as_str());
        return response.clicked().then(|| node.rel_path.clone());
    }

    let label = RichText::new(&node.name)
        .font(FontId::proportional(12.0))
        .color(if depth == 0 { TEXT } else { MUTED });
    let mut open_file = None;
    egui::CollapsingHeader::new(label)
        .id_salt(format!("project_dir:{}", node.rel_path))
        .default_open(depth < 1)
        .show(ui, |ui| {
            for child in &node.children {
                if let Some(path) = draw_node(ui, child, sidebar, active_rel_path, depth + 1) {
                    open_file = Some(path);
                }
            }
        });
    open_file
}

fn filter_button(ui: &mut egui::Ui, value: &mut bool, label: &str, tooltip: &str) {
    let fill = if *value {
        ACCENT
    } else {
        Color32::from_rgb(43, 47, 55)
    };
    let button = egui::Button::new(RichText::new(label).size(11.0))
        .min_size(egui::vec2(24.0, 20.0))
        .fill(fill)
        .stroke(Stroke::new(1.0, SIDEBAR_BORDER));
    if ui.add(button).on_hover_text(tooltip).clicked() {
        *value = !*value;
    }
}

pub(crate) fn scan_project_files(root: &Path) -> Vec<String> {
    search::scan_project_files(root)
}

fn git_modified_files(root: &Path) -> HashSet<String> {
    git::repository_status(root)
        .map(|status| status.files.into_iter().map(|file| file.path).collect())
        .unwrap_or_default()
}

fn build_tree(files: Vec<&ProjectFile>) -> ProjectTreeNode {
    let mut root = BuildNode::default();
    for file in files {
        insert_file(&mut root, &file.rel_path);
    }
    ProjectTreeNode {
        name: String::new(),
        rel_path: String::new(),
        is_file: false,
        children: build_children(root, ""),
    }
}

fn insert_file(root: &mut BuildNode, rel_path: &str) {
    let parts: Vec<&str> = rel_path
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return;
    }
    let mut node = root;
    for part in &parts[..parts.len().saturating_sub(1)] {
        node = node.dirs.entry((*part).to_string()).or_default();
    }
    let file_name = parts[parts.len() - 1].to_string();
    node.files.insert(file_name, rel_path.to_string());
}

fn build_children(node: BuildNode, parent: &str) -> Vec<ProjectTreeNode> {
    let mut children = Vec::new();
    for (name, child) in node.dirs {
        let rel_path = join_rel(parent, &name);
        children.push(ProjectTreeNode {
            name,
            rel_path: rel_path.clone(),
            is_file: false,
            children: build_children(child, &rel_path),
        });
    }
    for (name, rel_path) in node.files {
        children.push(ProjectTreeNode {
            name,
            rel_path,
            is_file: true,
            children: Vec::new(),
        });
    }
    children
}

fn join_rel(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}/{child}")
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

#[cfg(test)]
mod tests {
    use super::*;

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
            "viker-sidebar-{name}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        TempProject { root }
    }

    #[test]
    fn scan_project_files_respects_gitignore() {
        let project = temp_project("gitignore");
        std::fs::create_dir_all(project.root.join("src")).unwrap();
        std::fs::write(project.root.join(".gitignore"), "*.log\nignored/\n").unwrap();
        std::fs::write(project.root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(project.root.join("debug.log"), "ignored\n").unwrap();
        std::fs::create_dir_all(project.root.join("ignored")).unwrap();
        std::fs::write(project.root.join("ignored/file.txt"), "ignored\n").unwrap();

        assert_eq!(
            scan_project_files(&project.root),
            vec![".gitignore", "src/main.rs"]
        );
    }

    #[test]
    fn filtered_tree_preserves_parent_directories() {
        let mut sidebar = ProjectSidebar::default();
        sidebar.files = vec![
            ProjectFile {
                rel_path: "src/main.rs".to_string(),
            },
            ProjectFile {
                rel_path: "README.md".to_string(),
            },
        ];
        sidebar.search = "main".to_string();

        let tree = sidebar.filtered_tree();

        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].name, "src");
        assert_eq!(tree.children[0].children[0].rel_path, "src/main.rs");
    }

    #[test]
    fn filtered_tree_uses_skim_fuzzy_matching() {
        let mut sidebar = ProjectSidebar::default();
        sidebar.files = vec![
            ProjectFile {
                rel_path: "src/main.rs".to_string(),
            },
            ProjectFile {
                rel_path: "README.md".to_string(),
            },
        ];
        sidebar.search = "smr".to_string();

        let tree = sidebar.filtered_tree();

        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].children[0].rel_path, "src/main.rs");
    }

    #[test]
    fn recent_filter_uses_session_opened_files() {
        let project = temp_project("recent");
        let file = project.root.join("src/main.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "fn main() {}\n").unwrap();
        let mut sidebar = ProjectSidebar::default();
        sidebar.files = vec![
            ProjectFile {
                rel_path: "src/main.rs".to_string(),
            },
            ProjectFile {
                rel_path: "README.md".to_string(),
            },
        ];

        sidebar.note_opened(&project.root, &file);
        sidebar.recent_only = true;

        let tree = sidebar.filtered_tree();
        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].children[0].rel_path, "src/main.rs");
    }
}
