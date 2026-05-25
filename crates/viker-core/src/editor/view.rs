use ropey::Rope;

use crate::editor::selection::Position;
use crate::editor::wrap;

#[derive(Debug, Default, Clone, Copy)]
pub struct View {
    pub offset_row: usize,
    pub offset_col: usize,
    pub width: u16,
    pub height: u16,
    /// Which wrap segment of offset_row is at the top of the screen.
    pub offset_wrap: usize,
    pub wrap: bool,
}

impl View {
    pub fn ensure_cursor_visible(&mut self, cursor: &Position, scroll_off: usize) {
        if self.width == 0 || self.height == 0 {
            return;
        }
        let scrolloff = scroll_off.min(self.height as usize / 2);

        // Vertical scrolling
        if cursor.row < self.offset_row + scrolloff {
            self.offset_row = cursor.row.saturating_sub(scrolloff);
        }
        if cursor.row >= self.offset_row + self.height as usize - scrolloff {
            self.offset_row = cursor
                .row
                .saturating_sub(self.height as usize - 1 - scrolloff);
        }

        // Horizontal scrolling
        if cursor.col < self.offset_col {
            self.offset_col = cursor.col;
        }
        if cursor.col >= self.offset_col + self.width as usize {
            self.offset_col = cursor.col.saturating_sub(self.width as usize - 1);
        }
    }

    /// Ensure cursor is visible when wrap mode is active.
    /// `text_width` is the available width for text (area.width - gutter_width).
    pub fn ensure_cursor_visible_wrapped(
        &mut self,
        cursor: &Position,
        scroll_off: usize,
        rope: &Rope,
        text_width: u16,
        tab_width: usize,
    ) {
        // In wrap mode, horizontal offset is always 0
        self.offset_col = 0;

        if text_width == 0 || self.height == 0 {
            return;
        }

        let scrolloff = scroll_off.min(self.height as usize / 2);

        // Calculate cursor's screen row relative to viewport top
        let cursor_screen_row = self.cursor_screen_row(cursor, rope, text_width, tab_width);

        if let Some(screen_row) = cursor_screen_row {
            // Cursor is below visible area (or below scrolloff zone at bottom)
            if screen_row >= self.height as usize - scrolloff {
                let overshoot = screen_row - (self.height as usize - 1 - scrolloff);
                self.scroll_down_by(overshoot, rope, text_width, tab_width);
            }
            // Cursor is above scrolloff zone at top
            else if screen_row < scrolloff {
                let undershoot = scrolloff - screen_row;
                self.scroll_up_by(undershoot, rope, text_width, tab_width);
            }
        } else {
            // Cursor is completely off screen — need to reposition
            // Check if cursor is above viewport
            if cursor.row < self.offset_row
                || (cursor.row == self.offset_row && self.offset_wrap > 0 && {
                    let line = rope.line(cursor.row);
                    let (seg, _) = wrap::char_to_wrap_pos_with_tab_width(
                        line, cursor.col, text_width, tab_width,
                    );
                    seg < self.offset_wrap
                })
            {
                // Cursor is above — put cursor at scrolloff from top
                let line = rope.line(cursor.row);
                let (cursor_seg, _) =
                    wrap::char_to_wrap_pos_with_tab_width(line, cursor.col, text_width, tab_width);
                self.offset_row = cursor.row;
                self.offset_wrap = cursor_seg.saturating_sub(scrolloff);
                // If we can't go back enough segments in this line, go to previous lines
                if cursor_seg < scrolloff {
                    let mut remaining = scrolloff - cursor_seg;
                    self.offset_wrap = 0;
                    while self.offset_row > 0 && remaining > 0 {
                        self.offset_row -= 1;
                        let prev_line = rope.line(self.offset_row);
                        let wc = wrap::wrap_count_with_tab_width(prev_line, text_width, tab_width);
                        if wc <= remaining {
                            remaining -= wc;
                        } else {
                            self.offset_wrap = wc - remaining;
                            remaining = 0;
                        }
                    }
                }
            } else {
                // Cursor is below — put cursor at scrolloff from bottom
                self.position_cursor_near_bottom(cursor, scrolloff, rope, text_width, tab_width);
            }
        }
    }

    /// Returns the cursor's screen row (0-based from viewport top), or None if off-screen.
    fn cursor_screen_row(
        &self,
        cursor: &Position,
        rope: &Rope,
        text_width: u16,
        tab_width: usize,
    ) -> Option<usize> {
        if cursor.row < self.offset_row {
            return None;
        }

        let mut screen_row = 0usize;
        let line_count = rope.len_lines();

        // Count screen rows from viewport start to cursor
        for doc_row in self.offset_row..=cursor.row.min(line_count.saturating_sub(1)) {
            let line = rope.line(doc_row);
            let wc = wrap::wrap_count_with_tab_width(line, text_width, tab_width);
            let start_seg = if doc_row == self.offset_row {
                self.offset_wrap
            } else {
                0
            };

            if doc_row == cursor.row {
                let (cursor_seg, _) =
                    wrap::char_to_wrap_pos_with_tab_width(line, cursor.col, text_width, tab_width);
                if cursor_seg < start_seg {
                    return None;
                }
                let row = screen_row + cursor_seg - start_seg;
                if row >= self.height as usize {
                    return None;
                }
                return Some(row);
            }

            screen_row += wc - start_seg;
            if screen_row >= self.height as usize {
                return None;
            }
        }
        None
    }

    /// Scroll the viewport down by `n` screen lines.
    pub fn scroll_down_by(&mut self, n: usize, rope: &Rope, text_width: u16, tab_width: usize) {
        let line_count = rope.len_lines();
        let mut remaining = n;

        while remaining > 0 && self.offset_row < line_count {
            let line = rope.line(self.offset_row);
            let wc = wrap::wrap_count_with_tab_width(line, text_width, tab_width);
            let segs_left = wc - self.offset_wrap;

            if segs_left <= remaining {
                remaining -= segs_left;
                self.offset_row += 1;
                self.offset_wrap = 0;
            } else {
                self.offset_wrap += remaining;
                remaining = 0;
            }
        }
    }

    /// Scroll the viewport up by `n` screen lines.
    pub fn scroll_up_by(&mut self, n: usize, rope: &Rope, text_width: u16, tab_width: usize) {
        let mut remaining = n;

        while remaining > 0 {
            if self.offset_wrap > 0 {
                let go_back = remaining.min(self.offset_wrap);
                self.offset_wrap -= go_back;
                remaining -= go_back;
            } else if self.offset_row > 0 {
                self.offset_row -= 1;
                let line = rope.line(self.offset_row);
                let wc = wrap::wrap_count_with_tab_width(line, text_width, tab_width);
                if wc <= remaining {
                    remaining -= wc;
                    // offset_wrap stays 0, continue to previous line
                } else {
                    self.offset_wrap = wc - remaining;
                    remaining = 0;
                }
            } else {
                break;
            }
        }
    }

    /// Position viewport so cursor is near the bottom with scrolloff.
    fn position_cursor_near_bottom(
        &mut self,
        cursor: &Position,
        scrolloff: usize,
        rope: &Rope,
        text_width: u16,
        tab_width: usize,
    ) {
        // We want cursor to be at screen row (height - 1 - scrolloff)
        let target_from_top = (self.height as usize)
            .saturating_sub(1)
            .saturating_sub(scrolloff);

        // Start from cursor position and go backwards target_from_top screen lines
        let line = rope.line(cursor.row);
        let (cursor_seg, _) =
            wrap::char_to_wrap_pos_with_tab_width(line, cursor.col, text_width, tab_width);

        let mut row = cursor.row;
        let mut seg = cursor_seg;
        let mut to_go = target_from_top;

        while to_go > 0 {
            if seg > 0 {
                let go_back = to_go.min(seg);
                seg -= go_back;
                to_go -= go_back;
            } else if row > 0 {
                row -= 1;
                let prev_line = rope.line(row);
                let wc = wrap::wrap_count_with_tab_width(prev_line, text_width, tab_width);
                if wc <= to_go {
                    to_go -= wc;
                    seg = 0;
                } else {
                    seg = wc - to_go;
                    to_go = 0;
                }
            } else {
                break;
            }
        }

        self.offset_row = row;
        self.offset_wrap = seg;
    }
}
