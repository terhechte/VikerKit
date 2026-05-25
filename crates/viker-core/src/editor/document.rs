use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use ropey::Rope;

use crate::buffer;
use crate::editor::selection::Position;

pub struct Document {
    pub rope: Rope,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub version: i64,
}

impl Document {
    pub fn open(path: &str) -> Result<Self> {
        let p = PathBuf::from(path);
        if p.exists() {
            let content = fs::read_to_string(&p)?;
            let rope = Rope::from_str(&content);
            Ok(Self {
                rope,
                path: Some(p),
                modified: false,
                version: 0,
            })
        } else {
            Ok(Self {
                rope: Rope::from_str("\n"),
                path: Some(p),
                modified: false,
                version: 0,
            })
        }
    }

    pub fn new_empty() -> Self {
        Self {
            rope: Rope::from_str("\n"),
            path: None,
            modified: false,
            version: 0,
        }
    }

    pub fn save(&mut self) -> Result<()> {
        if let Some(path) = &self.path {
            let content = self.rope.to_string();
            fs::write(path, &content)?;
            self.modified = false;
            Ok(())
        } else {
            anyhow::bail!("No file path set")
        }
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line_len(&self, row: usize) -> usize {
        if row >= self.rope.len_lines() {
            return 0;
        }
        buffer::line_display_len(self.rope.line(row))
    }

    pub fn bump_version(&mut self) {
        self.version += 1;
    }

    pub fn insert_char(&mut self, pos: Position, ch: char) {
        let idx = self.pos_to_char_idx(pos);
        self.rope.insert_char(idx, ch);
        self.modified = true;
        self.bump_version();
    }

    pub fn insert_newline(&mut self, pos: Position) {
        let idx = self.pos_to_char_idx(pos);
        self.rope.insert_char(idx, '\n');
        self.modified = true;
        self.bump_version();
    }

    pub fn delete_char_backward(&mut self, pos: Position) -> Option<Position> {
        if pos.col == 0 && pos.row == 0 {
            return None;
        }
        let idx = self.pos_to_char_idx(pos);
        if idx == 0 {
            return None;
        }
        self.rope.remove(idx - 1..idx);
        self.modified = true;
        self.bump_version();

        if pos.col > 0 {
            Some(Position {
                row: pos.row,
                col: pos.col - 1,
            })
        } else {
            // Merged with previous line
            let new_row = pos.row - 1;
            let new_col = self.line_len(new_row);
            Some(Position {
                row: new_row,
                col: new_col,
            })
        }
    }

    pub fn delete_char_forward(&mut self, pos: Position) {
        let line_len = self.line_len(pos.row);
        if pos.col >= line_len && pos.row >= self.line_count().saturating_sub(1) {
            return;
        }
        let idx = self.pos_to_char_idx(pos);
        if idx >= self.rope.len_chars() {
            return;
        }
        self.rope.remove(idx..idx + 1);
        self.modified = true;
        self.bump_version();
    }

    pub fn delete_line(&mut self, row: usize) {
        if self.line_count() <= 1 {
            // Clear the only line but keep the newline
            let len = self.rope.len_chars();
            if len > 0 {
                self.rope.remove(0..len);
                self.rope.insert(0, "\n");
                self.modified = true;
                self.bump_version();
            }
            return;
        }
        let start = self.rope.line_to_char(row);
        let end = if row + 1 < self.rope.len_lines() {
            self.rope.line_to_char(row + 1)
        } else {
            self.rope.len_chars()
        };
        if start < end {
            self.rope.remove(start..end);
            self.modified = true;
            self.bump_version();
        }
    }

    pub fn file_name(&self) -> &str {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("[No Name]")
    }

    fn pos_to_char_idx(&self, pos: Position) -> usize {
        let line_start = self.rope.line_to_char(pos.row);
        line_start + pos.col
    }
}
