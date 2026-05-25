use ropey::RopeSlice;

/// Returns the display length of a line, excluding trailing newline/CR.
pub fn line_display_len(line: RopeSlice) -> usize {
    let len = line.len_chars();
    if len == 0 {
        return 0;
    }
    let last = line.char(len - 1);
    if last == '\n' {
        if len >= 2 && line.char(len - 2) == '\r' {
            len - 2
        } else {
            len - 1
        }
    } else {
        len
    }
}

/// Returns true if the character is a word character (alphanumeric or _).
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}
