use ropey::RopeSlice;
use unicode_width::UnicodeWidthChar;

use crate::buffer;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayCell {
    pub char_start: usize,
    pub char_end: usize,
    pub cell_start: usize,
    pub cell_width: usize,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewCell {
    pub row: usize,
    pub col: usize,
}

pub fn char_display_width(ch: char, cell_col: usize, tab_width: usize) -> usize {
    if ch == '\t' {
        let tab_width = tab_width.max(1);
        let remainder = cell_col % tab_width;
        if remainder == 0 {
            tab_width
        } else {
            tab_width - remainder
        }
    } else {
        UnicodeWidthChar::width(ch).unwrap_or(1)
    }
}

pub fn line_display_cells(line: RopeSlice<'_>, tab_width: usize) -> Vec<DisplayCell> {
    display_cells_for_char_range(line, 0, buffer::line_display_len(line), tab_width)
}

pub fn display_cells_for_char_range(
    line: RopeSlice<'_>,
    char_start: usize,
    char_end: usize,
    tab_width: usize,
) -> Vec<DisplayCell> {
    let line_len = buffer::line_display_len(line);
    let start = char_start.min(line_len);
    let end = char_end.min(line_len);
    let mut cells: Vec<DisplayCell> = Vec::new();
    let mut cell_col = 0usize;

    for char_idx in start..end {
        let ch = line.char(char_idx);
        let width = char_display_width(ch, cell_col, tab_width);

        if width == 0 {
            if let Some(cell) = cells.last_mut() {
                cell.char_end = char_idx + 1;
                cell.text.push(ch);
            } else {
                cells.push(DisplayCell {
                    char_start: char_idx,
                    char_end: char_idx + 1,
                    cell_start: cell_col,
                    cell_width: 0,
                    text: ch.to_string(),
                });
            }
            continue;
        }

        let text = if ch == '\t' {
            " ".repeat(width)
        } else {
            ch.to_string()
        };
        cells.push(DisplayCell {
            char_start: char_idx,
            char_end: char_idx + 1,
            cell_start: cell_col,
            cell_width: width,
            text,
        });
        cell_col = cell_col.saturating_add(width);
    }

    cells
}

pub fn line_display_width(line: RopeSlice<'_>, tab_width: usize) -> usize {
    display_column_for_char(line, buffer::line_display_len(line), tab_width)
}

pub fn display_column_for_char(line: RopeSlice<'_>, char_idx: usize, tab_width: usize) -> usize {
    display_column_for_char_range(line, 0, char_idx, tab_width)
}

pub fn display_column_for_char_range(
    line: RopeSlice<'_>,
    char_start: usize,
    char_idx: usize,
    tab_width: usize,
) -> usize {
    let line_len = buffer::line_display_len(line);
    let start = char_start.min(line_len);
    let end = char_idx.min(line_len);
    if end <= start {
        return 0;
    }

    let mut cell_col = 0usize;
    for idx in start..end {
        let width = char_display_width(line.char(idx), cell_col, tab_width);
        cell_col = cell_col.saturating_add(width);
    }
    cell_col
}

pub fn char_for_display_column(line: RopeSlice<'_>, target_col: usize, tab_width: usize) -> usize {
    char_for_display_column_in_range(
        line,
        0,
        buffer::line_display_len(line),
        target_col,
        tab_width,
    )
}

pub fn char_for_display_column_in_range(
    line: RopeSlice<'_>,
    char_start: usize,
    char_end: usize,
    target_col: usize,
    tab_width: usize,
) -> usize {
    let line_len = buffer::line_display_len(line);
    let start = char_start.min(line_len);
    let end = char_end.min(line_len);
    let mut cell_col = 0usize;

    for char_idx in start..end {
        let width = char_display_width(line.char(char_idx), cell_col, tab_width);
        if target_col <= cell_col || (width > 0 && target_col < cell_col.saturating_add(width)) {
            return char_idx;
        }
        cell_col = cell_col.saturating_add(width);
    }

    end
}
