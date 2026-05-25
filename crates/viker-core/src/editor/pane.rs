use crate::editor::document::Document;
use crate::editor::history::History;
use crate::editor::selection::Position;
use crate::editor::view::View;
use crate::highlight::style::SyntaxStyle;
use crate::highlight::{self, LineStyles};
use crate::input::mode::Mode;
use crate::lsp::LspDiagnostic;

use crate::buffer;

/// Frontend-independent rectangle for pane layout.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AreaRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl AreaRect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Per-pane state. Each pane has its own cursor, view, history, and buffer reference.
pub struct Pane {
    pub id: usize,
    pub buffer_idx: usize,
    pub cursor: Position,
    pub view: View,
    pub history: History,
    pub syntax_tree: Option<tree_sitter::Tree>,
    pub line_styles: LineStyles,
    pub styles_offset: usize,
    pub search_query: String,
    pub search_matches: Vec<(usize, usize, usize)>,
    pub search_index: Option<usize>,
    pub search_regex: Option<regex::Regex>,
    pub search_start_cursor: Option<Position>,
    pub jump_list: Vec<Position>,
    pub jump_index: usize,
}

impl Pane {
    pub fn new(id: usize, buffer_idx: usize) -> Self {
        Self {
            id,
            buffer_idx,
            cursor: Position::default(),
            view: View::default(),
            history: History::new(),
            syntax_tree: None,
            line_styles: Vec::new(),
            styles_offset: 0,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_index: None,
            search_regex: None,
            search_start_cursor: None,
            jump_list: Vec::new(),
            jump_index: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

pub enum PaneNode {
    Leaf(usize), // pane id
    Split {
        direction: SplitDirection,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    /// Calculate the layout rect for each leaf pane.
    pub fn layout(&self, area: AreaRect) -> Vec<(usize, AreaRect)> {
        let mut result = Vec::new();
        self.layout_inner(area, &mut result);
        result
    }

    fn layout_inner(&self, area: AreaRect, result: &mut Vec<(usize, AreaRect)>) {
        match self {
            PaneNode::Leaf(id) => {
                result.push((*id, area));
            }
            PaneNode::Split {
                direction,
                first,
                second,
            } => match direction {
                SplitDirection::Vertical => {
                    // Split left/right with 1-column separator
                    let half = area.width / 2;
                    if half < 2 || area.width < 5 {
                        // Too narrow, just show first
                        first.layout_inner(area, result);
                        return;
                    }
                    let left = AreaRect::new(area.x, area.y, half, area.height);
                    // 1-column separator
                    let right = AreaRect::new(
                        area.x + half + 1,
                        area.y,
                        area.width - half - 1,
                        area.height,
                    );
                    first.layout_inner(left, result);
                    second.layout_inner(right, result);
                }
                SplitDirection::Horizontal => {
                    // Split top/bottom. Status line of top pane serves as separator.
                    let half = area.height / 2;
                    if half < 2 || area.height < 4 {
                        first.layout_inner(area, result);
                        return;
                    }
                    let top = AreaRect::new(area.x, area.y, area.width, half);
                    let bottom =
                        AreaRect::new(area.x, area.y + half, area.width, area.height - half);
                    first.layout_inner(top, result);
                    second.layout_inner(bottom, result);
                }
            },
        }
    }

    /// Split the target leaf into a Split node with the target and a new pane.
    pub fn split(&mut self, target_id: usize, new_id: usize, direction: SplitDirection) -> bool {
        match self {
            PaneNode::Leaf(id) if *id == target_id => {
                let old = Box::new(PaneNode::Leaf(target_id));
                let new = Box::new(PaneNode::Leaf(new_id));
                *self = PaneNode::Split {
                    direction,
                    first: old,
                    second: new,
                };
                true
            }
            PaneNode::Split { first, second, .. } => {
                first.split(target_id, new_id, direction)
                    || second.split(target_id, new_id, direction)
            }
            _ => false,
        }
    }

    /// Remove a pane. The sibling replaces the parent Split.
    /// Returns true if the pane was found and removed.
    pub fn remove(&mut self, pane_id: usize) -> bool {
        match self {
            PaneNode::Leaf(_) => false,
            PaneNode::Split { first, second, .. } => {
                // Check if first child is the target leaf
                if matches!(first.as_ref(), PaneNode::Leaf(id) if *id == pane_id) {
                    // Replace self with second
                    let second = std::mem::replace(second, Box::new(PaneNode::Leaf(0)));
                    *self = *second;
                    return true;
                }
                // Check if second child is the target leaf
                if matches!(second.as_ref(), PaneNode::Leaf(id) if *id == pane_id) {
                    let first = std::mem::replace(first, Box::new(PaneNode::Leaf(0)));
                    *self = *first;
                    return true;
                }
                // Recurse
                first.remove(pane_id) || second.remove(pane_id)
            }
        }
    }

    /// Return all leaf pane IDs in order (left-to-right, top-to-bottom).
    pub fn leaves(&self) -> Vec<usize> {
        let mut result = Vec::new();
        self.collect_leaves(&mut result);
        result
    }

    fn collect_leaves(&self, result: &mut Vec<usize>) {
        match self {
            PaneNode::Leaf(id) => result.push(*id),
            PaneNode::Split { first, second, .. } => {
                first.collect_leaves(result);
                second.collect_leaves(result);
            }
        }
    }

    /// Find an adjacent pane in the given direction using rect center coordinates.
    pub fn find_adjacent(
        &self,
        from_id: usize,
        dir: NavigateDir,
        rects: &[(usize, AreaRect)],
    ) -> Option<usize> {
        let from_rect = rects.iter().find(|(id, _)| *id == from_id)?.1;
        let center_x = from_rect.x as i32 + from_rect.width as i32 / 2;
        let center_y = from_rect.y as i32 + from_rect.height as i32 / 2;

        let mut best: Option<(usize, i32)> = None;

        for &(id, rect) in rects {
            if id == from_id {
                continue;
            }
            let cx = rect.x as i32 + rect.width as i32 / 2;
            let cy = rect.y as i32 + rect.height as i32 / 2;

            let valid = match dir {
                NavigateDir::Left => cx < center_x,
                NavigateDir::Right => cx > center_x,
                NavigateDir::Up => cy < center_y,
                NavigateDir::Down => cy > center_y,
            };

            if !valid {
                continue;
            }

            let dist = (cx - center_x).abs() + (cy - center_y).abs();
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((id, dist));
            }
        }

        best.map(|(id, _)| id)
    }

    /// Returns true if this is a single leaf (no splits).
    pub fn is_single(&self) -> bool {
        matches!(self, PaneNode::Leaf(_))
    }

    /// Collect vertical separator positions for rendering.
    #[allow(dead_code)]
    pub fn separators(&self, area: AreaRect) -> Vec<(u16, u16, u16)> {
        let mut result = Vec::new();
        self.collect_separators(area, &mut result);
        result
    }

    fn collect_separators(&self, area: AreaRect, result: &mut Vec<(u16, u16, u16)>) {
        if let PaneNode::Split {
            direction,
            first,
            second,
        } = self
        {
            match direction {
                SplitDirection::Vertical => {
                    let half = area.width / 2;
                    if half >= 2 && area.width >= 5 {
                        // Separator at x = area.x + half
                        result.push((area.x + half, area.y, area.height));
                        let left = AreaRect::new(area.x, area.y, half, area.height);
                        let right = AreaRect::new(
                            area.x + half + 1,
                            area.y,
                            area.width - half - 1,
                            area.height,
                        );
                        first.collect_separators(left, result);
                        second.collect_separators(right, result);
                    }
                }
                SplitDirection::Horizontal => {
                    let half = area.height / 2;
                    if half >= 2 && area.height >= 4 {
                        let top = AreaRect::new(area.x, area.y, area.width, half);
                        let bottom =
                            AreaRect::new(area.x, area.y + half, area.width, area.height - half);
                        first.collect_separators(top, result);
                        second.collect_separators(bottom, result);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigateDir {
    Left,
    Down,
    Up,
    Right,
}

/// Data needed to render a single pane. Borrows from Editor state.
pub struct PaneRenderData<'a> {
    pub document: &'a Document,
    pub cursor: Position,
    pub view: View,
    pub mode: Mode,
    pub diagnostics: &'a [LspDiagnostic],
    pub line_styles: &'a LineStyles,
    pub styles_offset: usize,
    pub search_matches: &'a [(usize, usize, usize)],
    pub search_query: &'a str,
    pub visual_anchor: Option<Position>,
    pub is_active: bool,
    pub matching_bracket: Option<Position>,
    pub relative_number: bool,
    pub tab_width: usize,
}

impl<'a> PaneRenderData<'a> {
    /// Get syntax highlight style at a position.
    pub fn highlight_style_at(&self, doc_row: usize, col: usize) -> SyntaxStyle {
        if let Some(rel) = doc_row.checked_sub(self.styles_offset) {
            highlight::style_at(self.line_styles, rel, col)
        } else {
            highlight::theme::default_style()
        }
    }

    /// Check if position is a search match.
    pub fn is_search_match(&self, row: usize, col: usize) -> bool {
        if self.search_query.is_empty() {
            return false;
        }
        self.search_matches
            .iter()
            .any(|&(r, c, len)| r == row && col >= c && col < c + len)
    }

    /// Get selection range if in visual mode.
    pub fn selection_range(&self) -> Option<(Position, Position)> {
        let anchor = self.visual_anchor?;
        if !self.mode.is_visual() {
            return None;
        }
        let (start, end) = if anchor <= self.cursor {
            (anchor, self.cursor)
        } else {
            (self.cursor, anchor)
        };
        if self.mode == Mode::VisualLine {
            Some((
                Position {
                    row: start.row,
                    col: 0,
                },
                Position {
                    row: end.row,
                    col: usize::MAX,
                },
            ))
        } else {
            Some((start, end))
        }
    }

    pub fn is_selected(&self, row: usize, col: usize) -> bool {
        let anchor = match self.visual_anchor {
            Some(anchor) if self.mode.is_visual() => anchor,
            _ => return false,
        };
        if self.mode == Mode::VisualBlock {
            let start_row = anchor.row.min(self.cursor.row);
            let end_row = anchor.row.max(self.cursor.row);
            let start_col = anchor.col.min(self.cursor.col);
            let end_col = anchor.col.max(self.cursor.col);
            return row >= start_row && row <= end_row && col >= start_col && col <= end_col;
        }
        let Some((start, end)) = self.selection_range() else {
            return false;
        };
        let pos = Position { row, col };
        pos >= start && pos <= end
    }

    pub fn line_number_label(&self, row: usize) -> usize {
        if self.relative_number && self.is_active && row != self.cursor.row {
            row.abs_diff(self.cursor.row)
        } else {
            row + 1
        }
    }

    /// Compute matching bracket position.
    pub fn compute_matching_bracket(document: &Document, cursor: Position) -> Option<Position> {
        let line = document.rope.line(cursor.row);
        let line_len = buffer::line_display_len(line);
        if cursor.col >= line_len {
            return None;
        }
        let ch = line.char(cursor.col);
        let (target, forward) = match ch {
            '(' => (')', true),
            '{' => ('}', true),
            '[' => (']', true),
            ')' => ('(', false),
            '}' => ('{', false),
            ']' => ('[', false),
            _ => return None,
        };

        if forward {
            Self::find_matching_forward(document, cursor, ch, target)
        } else {
            Self::find_matching_backward(document, cursor, ch, target)
        }
    }

    fn find_matching_forward(
        document: &Document,
        cursor: Position,
        open: char,
        close: char,
    ) -> Option<Position> {
        let mut depth = 0i32;
        let line_count = document.line_count();
        for row in cursor.row..line_count {
            let line = document.rope.line(row);
            let start_col = if row == cursor.row { cursor.col } else { 0 };
            let line_len = buffer::line_display_len(line);
            for col in start_col..line_len {
                let c = line.char(col);
                if c == open {
                    depth += 1;
                } else if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position { row, col });
                    }
                }
            }
        }
        None
    }

    fn find_matching_backward(
        document: &Document,
        cursor: Position,
        close: char,
        open: char,
    ) -> Option<Position> {
        let mut depth = 0i32;
        for row in (0..=cursor.row).rev() {
            let line = document.rope.line(row);
            let line_len = buffer::line_display_len(line);
            let end_col = if row == cursor.row {
                cursor.col
            } else {
                line_len.saturating_sub(1)
            };
            for col in (0..=end_col).rev() {
                if col >= line_len {
                    continue;
                }
                let c = line.char(col);
                if c == close {
                    depth += 1;
                } else if c == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(Position { row, col });
                    }
                }
            }
        }
        None
    }
}
