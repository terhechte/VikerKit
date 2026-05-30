## Cursor movement

- `h` — Move cursor left.
- `j` — Move cursor down.
- `k` — Move cursor up.
- `l` — Move cursor right.
- `gj` — Move cursor down through wrapped display lines.
- `gk` — Move cursor up through wrapped display lines.
- `H` — Move to the top of the screen.
- `M` — Move to the middle of the screen.
- `L` — Move to the bottom of the screen.
- `w` — Jump forward to the start of the next word.
- `W` — Jump forward to the start of the next WORD, including punctuation.
- `e` — Jump forward to the end of the current or next word.
- `E` — Jump forward to the end of the current or next WORD, including punctuation.
- `b` — Jump backward to the start of a word.
- `B` — Jump backward to the start of a WORD, including punctuation.
- `ge` — Jump backward to the end of a word.
- `gE` — Jump backward to the end of a WORD, including punctuation.
- `%` — Move to the matching character, such as `()`, `{}`, or `[]`.
- `0` — Jump to the start of the line.
- `^` — Jump to the first non-blank character of the line.
- `$` — Jump to the end of the line.
- `g_` — Jump to the last non-blank character of the line.
- `gg` — Go to the first line of the document.
- `G` — Go to the last line of the document.
- `5gg` — Go to line 5.
- `5G` — Go to line 5.
- `gd` — Move to the local declaration.
- `gD` — Move to the global declaration.
- `fx` — Jump to the next occurrence of character `x`.
- `tx` — Jump to just before the next occurrence of character `x`.
- `Fx` — Jump to the previous occurrence of character `x`.
- `Tx` — Jump to just after the previous occurrence of character `x`.
- `;` — Repeat the previous `f`, `t`, `F`, or `T` movement.
- `,` — Repeat the previous `f`, `t`, `F`, or `T` movement in the opposite direction.
- `}` — Jump to the next paragraph, function, or block.
- `{` — Jump to the previous paragraph, function, or block.
- `zz` — Center the cursor on the screen.
- `zt` — Put the cursor line at the top of the screen.
- `zb` — Put the cursor line at the bottom of the screen.
- `Ctrl` + `e` — Scroll the screen down one line without moving the cursor.
- `Ctrl` + `y` — Scroll the screen up one line without moving the cursor.
- `Ctrl` + `b` — Move one page up.
- `Ctrl` + `f` — Move one page down.
- `Ctrl` + `d` — Move down half a page.
- `Ctrl` + `u` — Move up half a page.

## Insert mode

- `i` — Insert text before the cursor.
- `I` — Insert text at the beginning of the line.
- `a` — Append text after the cursor.
- `A` — Append text at the end of the line.
- `o` — Open a new line below the current line.
- `O` — Open a new line above the current line.
- `ea` — Append text at the end of the current word.
- `Ctrl` + `rx` — Insert the contents of register `x`.
- `Ctrl` + `ox` — Temporarily enter normal mode to run one command `x`.

## Editing

- `r` — Replace a single character.
- `R` — Replace characters until `Esc` is pressed.
- `J` — Join the line below with the current line, inserting a space.
- `gJ` — Join the line below with the current line without inserting a space.
- `gwip` — Reflow the current paragraph.
- `g~` — Toggle case up to a motion.
- `gu` — Change text to lowercase up to a motion.
- `gU` — Change text to uppercase up to a motion.
- `cc` — Change the entire current line.
- `c$` — Change from the cursor to the end of the line.
- `C` — Change from the cursor to the end of the line.
- `ciw` — Change the entire word under the cursor.
- `cw` — Change from the cursor to the end of the word.
- `ce` — Change from the cursor to the end of the word.
- `s` — Delete one character and enter insert mode.
- `S` — Delete the current line and enter insert mode.
- `xp` — Swap two adjacent letters by deleting and pasting.
- `u` — Undo the last change.
- `U` — Undo changes on the last modified line.
- `Ctrl` + `r` — Redo.
- `.` — Repeat the last command.

## Visual mode

- `v` — Start characterwise visual mode.
- `V` — Start linewise visual mode.
- `o` — Move to the other end of the selected area.
- `Ctrl` + `v` — Start visual block mode.
- `O` — Move to the other corner of the selected block.
- `aw` — Select a word.
- `ab` — Select a block surrounded by `()`.
- `aB` — Select a block surrounded by `{}`.
- `at` — Select a block surrounded by `<>` tags.
- `ib` — Select the inner block inside `()`.
- `iB` — Select the inner block inside `{}`.
- `it` — Select the inner block inside `<>` tags.
- `Esc` — Exit visual mode.
- `Ctrl` + `c` — Exit visual mode.

## Visual commands

- `>` — Shift selected text right.
- `<` — Shift selected text left.
- `y` — Yank, or copy, selected text.
- `d` — Delete selected text.
- `~` — Toggle case of selected text.
- `u` — Change selected text to lowercase.
- `U` — Change selected text to uppercase.

## Registers

- `:reg[isters]` — Show register contents.
- `"xy` — Yank into register `x`.
- `"xp` — Paste from register `x`.
- `"+y` — Yank into the system clipboard register.
- `"+p` — Paste from the system clipboard register.
- `0` — Register containing the last yank.
- `"` — Unnamed register, containing the last delete or yank.
- `%` — Register containing the current file name.
- `#` — Register containing the alternate file name.
- `*` — Register containing X11 primary clipboard contents.
- `+` — Register containing X11 clipboard contents.
- `/` — Register containing the last search pattern.
- `:` — Register containing the last command-line command.
- `.` — Register containing the last inserted text.
- `-` — Register containing the last small delete.
- `=` — Expression register.
- `_` — Black hole register.

## Marks and positions

- `:marks` — List all marks.
- `ma` — Set mark `a` at the current position.
- `` `a `` — Jump to the position of mark `a`.
- ``y`a`` — Yank text from the cursor to mark `a`.
- `` `0 `` — Go to the position where Vim was previously exited.
- `` `" `` — Go to the position where this file was last edited.
- `` `. `` — Go to the position of the last change in this file.
- ` ` `` — Go to the position before the last jump.
- `:ju[mps]` — Show the jump list.
- `Ctrl` + `i` — Go to a newer position in the jump list.
- `Ctrl` + `o` — Go to an older position in the jump list.
- `:changes` — Show the change list.
- `g,` — Go to a newer position in the change list.
- `g;` — Go to an older position in the change list.
- `Ctrl` + `]` — Jump to the tag under the cursor.

## Macros

- `qa` — Start recording macro `a`.
- `q` — Stop recording a macro.
- `@a` — Run macro `a`.
- `@@` — Run the last executed macro again.

## Cut and paste

- `yy` — Yank, or copy, the current line.
- `2yy` — Yank two lines.
- `yw` — Yank from the cursor to the start of the next word.
- `yiw` — Yank the word under the cursor.
- `yaw` — Yank the word under the cursor plus surrounding space.
- `y$` — Yank from the cursor to the end of the line.
- `Y` — Yank from the cursor to the end of the line.
- `p` — Paste after the cursor.
- `P` — Paste before the cursor.
- `gp` — Paste after the cursor and leave the cursor after the pasted text.
- `gP` — Paste before the cursor and leave the cursor after the pasted text.
- `dd` — Delete, or cut, the current line.
- `2dd` — Delete two lines.
- `dw` — Delete from the cursor to the start of the next word.
- `diw` — Delete the word under the cursor.
- `daw` — Delete the word under the cursor plus surrounding space.
- `:3,5d` — Delete lines 3 through 5.
- `:.,$d` — Delete from the current line to the end of the file.
- `:.,1d` — Delete from the current line to the beginning of the file.
- `:10,1d` — Delete from line 10 to the beginning of the file.
- `:g/{pattern}/d` — Delete all lines matching `{pattern}`.
- `:g!/{pattern}/d` — Delete all lines not matching `{pattern}`.
- `d$` — Delete from the cursor to the end of the line.
- `D` — Delete from the cursor to the end of the line.
- `x` — Delete the character under the cursor.

## Indent text

- `>>` — Indent the current line by one shiftwidth.
- `<<` — De-indent the current line by one shiftwidth.
- `>%` — Indent a `()` or `{}` block while the cursor is on a brace.
- `<%` — De-indent a `()` or `{}` block while the cursor is on a brace.
- `>ib` — Indent the inner `()` block.
- `>at` — Indent a block surrounded by `<>` tags.
- `3==` — Re-indent three lines.
- `=%` — Re-indent a `()` or `{}` block while the cursor is on a brace.
- `=iB` — Re-indent the inner `{}` block.
- `gg=G` — Re-indent the entire buffer.
- `]p` — Paste and adjust indentation to the current line.

## Search and replace

- `/pattern` — Search forward for `pattern`.
- `?pattern` — Search backward for `pattern`.
- `\vpattern` — Search using “very magic” mode, where many regex symbols need less escaping.
- `n` — Repeat the search in the same direction.
- `N` — Repeat the search in the opposite direction.
- `:%s/old/new/g` — Replace all occurrences of `old` with `new` in the file.
- `:%s/old/new/gc` — Replace all occurrences of `old` with `new` in the file, asking for confirmation.
- `:noh[lsearch]` — Clear highlighted search matches.
