use ropey::Rope;

use crate::editor::selection::Position;

#[derive(Clone)]
struct Snapshot {
    rope: Rope,
    cursor: Position,
}

pub struct History {
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
    max_entries: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_entries: 1000,
        }
    }

    /// Save a snapshot before making changes. Clears the redo stack.
    pub fn save(&mut self, rope: &Rope, cursor: Position) {
        self.redo_stack.clear();
        if self.undo_stack.len() >= self.max_entries {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(Snapshot {
            rope: rope.clone(),
            cursor,
        });
    }

    /// Undo: restore previous snapshot, push current state to redo stack.
    pub fn undo(&mut self, rope: &Rope, cursor: Position) -> Option<(Rope, Position)> {
        let snapshot = self.undo_stack.pop()?;
        self.redo_stack.push(Snapshot {
            rope: rope.clone(),
            cursor,
        });
        Some((snapshot.rope, snapshot.cursor))
    }

    /// Redo: restore next snapshot, push current state to undo stack.
    pub fn redo(&mut self, rope: &Rope, cursor: Position) -> Option<(Rope, Position)> {
        let snapshot = self.redo_stack.pop()?;
        self.undo_stack.push(Snapshot {
            rope: rope.clone(),
            cursor,
        });
        Some((snapshot.rope, snapshot.cursor))
    }
}
