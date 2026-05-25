use ropey::{Rope, RopeSlice};

use crate::buffer;
use crate::editor::display;

/// A single screen line segment produced by wrapping a document line.
#[derive(Debug, Clone)]
pub struct WrapSegment {
    pub doc_row: usize,
    /// 0 = first segment of the line
    pub segment_index: usize,
    /// Start character index in the document line (inclusive)
    pub char_start: usize,
    /// End character index in the document line (exclusive)
    pub char_end: usize,
}

fn char_width(ch: char, col: u16, tab_width: usize) -> u16 {
    display::char_display_width(ch, col as usize, tab_width).min(u16::MAX as usize) as u16
}

/// How many screen lines a document line occupies when wrapped.
/// Empty lines and lines fitting within text_width occupy 1 screen line.
pub fn wrap_count(line: RopeSlice, text_width: u16) -> usize {
    wrap_count_with_tab_width(line, text_width, 1)
}

pub fn wrap_count_with_tab_width(line: RopeSlice, text_width: u16, tab_width: usize) -> usize {
    if text_width == 0 {
        return 1;
    }
    let line_len = buffer::line_display_len(line);
    if line_len == 0 {
        return 1;
    }
    let mut segments = 1usize;
    let mut col: u16 = 0;
    for i in 0..line_len {
        let ch = line.char(i);
        let w = char_width(ch, col, tab_width);
        // CJK character that doesn't fit on this segment → start new segment
        if w > 1 && col.saturating_add(w) > text_width && col > 0 {
            segments += 1;
            col = w;
            continue;
        }
        if w > 0 && col.saturating_add(w) > text_width {
            segments += 1;
            col = w;
        } else {
            col += w;
        }
    }
    segments
}

/// Convert a character index within a line to (segment_index, display_column_within_segment).
pub fn char_to_wrap_pos(line: RopeSlice, char_idx: usize, text_width: u16) -> (usize, u16) {
    char_to_wrap_pos_with_tab_width(line, char_idx, text_width, 1)
}

pub fn char_to_wrap_pos_with_tab_width(
    line: RopeSlice,
    char_idx: usize,
    text_width: u16,
    tab_width: usize,
) -> (usize, u16) {
    if text_width == 0 {
        return (0, 0);
    }
    let line_len = buffer::line_display_len(line);
    let target = char_idx.min(line_len);
    for seg in build_line_segments_with_tab_width(line, 0, text_width, tab_width) {
        let in_segment = if target == line_len {
            target >= seg.char_start && target <= seg.char_end
        } else {
            target >= seg.char_start && target < seg.char_end
        };
        if in_segment {
            let col =
                display::display_column_for_char_range(line, seg.char_start, target, tab_width)
                    .min(u16::MAX as usize) as u16;
            return (seg.segment_index, col);
        }
    }
    (0, 0)
}

/// Convert (segment_index, target_display_column) back to a character index.
/// Used for vertical cursor movement within wrapped lines.
pub fn wrap_pos_to_char(
    line: RopeSlice,
    segment: usize,
    target_col: u16,
    text_width: u16,
) -> usize {
    wrap_pos_to_char_with_tab_width(line, segment, target_col, text_width, 1)
}

pub fn wrap_pos_to_char_with_tab_width(
    line: RopeSlice,
    segment: usize,
    target_col: u16,
    text_width: u16,
    tab_width: usize,
) -> usize {
    if text_width == 0 {
        return 0;
    }
    let line_len = buffer::line_display_len(line);
    if line_len == 0 {
        return 0;
    }
    let segments = build_line_segments_with_tab_width(line, 0, text_width, tab_width);
    if let Some(seg) = segments.get(segment).or_else(|| segments.last()) {
        let char_idx = display::char_for_display_column_in_range(
            line,
            seg.char_start,
            seg.char_end,
            target_col as usize,
            tab_width,
        );
        if char_idx >= seg.char_end {
            return seg.char_end.saturating_sub(1).max(seg.char_start);
        }
        return char_idx;
    }
    line_len.saturating_sub(1)
}

/// Build a screen map of WrapSegments starting from (start_doc_row, start_wrap_segment)
/// for up to screen_height screen lines.
pub fn build_screen_map(
    rope: &Rope,
    start_doc_row: usize,
    start_wrap_segment: usize,
    text_width: u16,
    screen_height: u16,
) -> Vec<WrapSegment> {
    build_screen_map_with_tab_width(
        rope,
        start_doc_row,
        start_wrap_segment,
        text_width,
        screen_height,
        1,
    )
}

pub fn build_screen_map_with_tab_width(
    rope: &Rope,
    start_doc_row: usize,
    start_wrap_segment: usize,
    text_width: u16,
    screen_height: u16,
    tab_width: usize,
) -> Vec<WrapSegment> {
    let mut result = Vec::with_capacity(screen_height as usize);
    let line_count = rope.len_lines();
    let mut doc_row = start_doc_row;

    if doc_row >= line_count {
        return result;
    }

    // For the first line, we may skip some segments
    let first_line = rope.line(doc_row);
    let segments = build_line_segments_with_tab_width(first_line, doc_row, text_width, tab_width);
    for seg in segments.into_iter().skip(start_wrap_segment) {
        result.push(seg);
        if result.len() >= screen_height as usize {
            return result;
        }
    }
    doc_row += 1;

    while doc_row < line_count && result.len() < screen_height as usize {
        let line = rope.line(doc_row);
        let segments = build_line_segments_with_tab_width(line, doc_row, text_width, tab_width);
        for seg in segments {
            result.push(seg);
            if result.len() >= screen_height as usize {
                return result;
            }
        }
        doc_row += 1;
    }

    result
}

fn build_line_segments_with_tab_width(
    line: RopeSlice,
    doc_row: usize,
    text_width: u16,
    tab_width: usize,
) -> Vec<WrapSegment> {
    let line_len = buffer::line_display_len(line);
    if line_len == 0 || text_width == 0 {
        return vec![WrapSegment {
            doc_row,
            segment_index: 0,
            char_start: 0,
            char_end: 0,
        }];
    }

    let mut segments = Vec::new();
    let mut seg_start = 0usize;
    let mut seg_idx = 0usize;
    let mut col: u16 = 0;

    for i in 0..line_len {
        let ch = line.char(i);
        let w = char_width(ch, col, tab_width);

        let need_wrap = if w > 1 && col.saturating_add(w) > text_width && col > 0 {
            true
        } else {
            w > 0 && col.saturating_add(w) > text_width
        };

        if need_wrap {
            segments.push(WrapSegment {
                doc_row,
                segment_index: seg_idx,
                char_start: seg_start,
                char_end: i,
            });
            seg_idx += 1;
            seg_start = i;
            col = w;
        } else {
            col += w;
        }
    }

    // Final segment
    segments.push(WrapSegment {
        doc_row,
        segment_index: seg_idx,
        char_start: seg_start,
        char_end: line_len,
    });

    segments
}
