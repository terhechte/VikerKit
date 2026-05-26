use viker_core::input::mode::Mode;
use viker_core::key::{KeyCode, KeyInput};
use viker_vim::{Effect, VimCore};

fn key(code: KeyCode) -> KeyInput {
    KeyInput {
        code,
        ctrl: false,
        alt: false,
    }
}

fn ctrl(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode::Char(ch),
        ctrl: true,
        alt: false,
    }
}

fn alt(ch: char) -> KeyInput {
    KeyInput {
        code: KeyCode::Char(ch),
        ctrl: false,
        alt: true,
    }
}

fn ch(ch: char) -> KeyInput {
    key(KeyCode::Char(ch))
}

fn type_keys(vim: &mut VimCore, input: &str) -> Vec<Effect> {
    let mut effects = Vec::new();
    for ch in input.chars() {
        effects.extend(vim.process_key(KeyInput {
            code: KeyCode::Char(ch),
            ctrl: false,
            alt: false,
        }));
    }
    effects
}

#[test]
fn vim_core_processes_insert_keys_without_editor() {
    let mut vim = VimCore::from_text("hello\n");

    vim.process_key(ch('i'));
    vim.process_key(ch('X'));
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "Xhello\n");
    assert_eq!(vim.mode, Mode::Normal);
}

#[test]
fn vim_core_keeps_registers_deterministic() {
    let mut vim = VimCore::from_text("one\ntwo\n");

    vim.process_key(ch('"'));
    vim.process_key(ch('a'));
    vim.process_key(ch('y'));
    vim.process_key(ch('y'));

    assert_eq!(vim.registers.get(&'a').unwrap().content, "one\n");
    assert!(vim.registers.get(&'a').unwrap().linewise);

    vim.process_key(ch('j'));
    vim.process_key(ch('"'));
    vim.process_key(ch('a'));
    vim.process_key(ch('p'));

    assert_eq!(vim.text(), "one\ntwo\none\n");
}

#[test]
fn vim_core_emits_effects_for_external_work() {
    let mut vim = VimCore::from_text("hello\n");

    assert_eq!(vim.process_key(ctrl('p')), vec![Effect::OpenFileFinder]);

    vim.process_key(ch(':'));
    let effects = type_keys(&mut vim, "!echo hi");
    assert!(effects.is_empty());
    assert_eq!(
        vim.process_key(key(KeyCode::Enter)),
        vec![Effect::ShellCommand("echo hi".to_string())]
    );

    vim.process_key(ch(':'));
    type_keys(&mut vim, "w");
    assert_eq!(vim.process_key(key(KeyCode::Enter)), vec![Effect::Save]);
}

#[test]
fn vim_core_substitute_char_with_s() {
    let mut vim = VimCore::from_text("abc\n");

    vim.process_key(ch('s'));
    assert_eq!(vim.mode, Mode::Insert);
    vim.process_key(ch('X'));
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "Xbc\n");
    assert_eq!(vim.registers.get(&'"').unwrap().content, "a");
}

#[test]
fn vim_core_substitute_line_with_s_uppercase() {
    let mut vim = VimCore::from_text("  hello\nnext\n");

    vim.process_key(ch('S'));
    type_keys(&mut vim, "changed");
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "  changed\nnext\n");
}

#[test]
fn vim_core_deletes_previous_char_with_x_uppercase() {
    let mut vim = VimCore::from_text("abc\n");

    vim.process_key(ch('l'));
    vim.process_key(ch('l'));
    vim.process_key(ch('X'));

    assert_eq!(vim.text(), "ac\n");
    assert_eq!(vim.cursor.col, 1);
    assert_eq!(vim.registers.get(&'"').unwrap().content, "b");
}

#[test]
fn vim_core_yanks_line_with_y_uppercase() {
    let mut vim = VimCore::from_text("one\ntwo\n");

    vim.process_key(ch('Y'));
    assert_eq!(vim.registers.get(&'"').unwrap().content, "one\n");

    vim.process_key(ch('p'));
    assert_eq!(vim.text(), "one\none\ntwo\n");
}

#[test]
fn vim_core_insert_ctrl_w_deletes_previous_word() {
    let mut vim = VimCore::from_text("\n");

    vim.process_key(ch('i'));
    type_keys(&mut vim, "hello world");
    vim.process_key(ctrl('w'));
    vim.process_key(ch('!'));
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "hello !\n");
}

#[test]
fn vim_core_insert_ctrl_u_deletes_to_line_start() {
    let mut vim = VimCore::from_text("\n");

    vim.process_key(ch('i'));
    type_keys(&mut vim, "discard");
    vim.process_key(ctrl('u'));
    vim.process_key(ch('z'));
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "z\n");
}

#[test]
fn vim_core_enters_visual_block_with_ctrl_v() {
    let mut vim = VimCore::from_text("abcd\n1234\n");

    vim.process_key(ctrl('v'));

    assert_eq!(vim.mode, Mode::VisualBlock);
    assert_eq!(vim.visual_anchor, Some(vim.cursor));
}

#[test]
fn vim_core_visual_block_delete_removes_rectangle() {
    let mut vim = VimCore::from_text("abcd\n1234\nWXYZ\n");

    vim.process_key(ctrl('v'));
    vim.process_key(ch('l'));
    vim.process_key(ch('j'));
    vim.process_key(ch('d'));

    assert_eq!(vim.text(), "cd\n34\nWXYZ\n");
    assert_eq!(vim.mode, Mode::Normal);
    assert_eq!(vim.registers.get(&'"').unwrap().content, "ab\n12");
    assert!(!vim.registers.get(&'"').unwrap().linewise);
}

#[test]
fn vim_core_visual_block_yank_copies_rectangle() {
    let mut vim = VimCore::from_text("abcd\n1234\nWXYZ\n");

    vim.process_key(ch('l'));
    vim.process_key(ctrl('v'));
    vim.process_key(ch('l'));
    vim.process_key(ch('j'));
    vim.process_key(ch('j'));
    vim.process_key(ch('y'));

    assert_eq!(vim.text(), "abcd\n1234\nWXYZ\n");
    assert_eq!(vim.mode, Mode::Normal);
    assert_eq!(vim.registers.get(&'"').unwrap().content, "bc\n23\nXY");
}

#[test]
fn vim_core_visual_block_case_change_applies_to_rectangle() {
    let mut vim = VimCore::from_text("abcd\nwxyz\n");

    vim.process_key(ctrl('v'));
    vim.process_key(ch('l'));
    vim.process_key(ch('j'));
    vim.process_key(ch('U'));

    assert_eq!(vim.text(), "ABcd\nWXyz\n");
    assert_eq!(vim.mode, Mode::Normal);
}

#[test]
fn vim_core_insert_ctrl_a_moves_to_line_start() {
    let mut vim = VimCore::from_text("abc\n");

    vim.process_key(ch('A'));
    vim.process_key(ctrl('a'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.col, 0);
}

#[test]
fn vim_core_insert_ctrl_e_moves_to_line_end() {
    let mut vim = VimCore::from_text("abc\n");

    vim.process_key(ch('i'));
    vim.process_key(ctrl('e'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.col, 3);
}

#[test]
fn vim_core_insert_ctrl_f_moves_next_char() {
    let mut vim = VimCore::from_text("abc\n");

    vim.process_key(ch('i'));
    vim.process_key(ctrl('f'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.col, 1);
}

#[test]
fn vim_core_insert_ctrl_b_moves_previous_char() {
    let mut vim = VimCore::from_text("abc\n");

    vim.process_key(ch('A'));
    vim.process_key(ctrl('b'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.col, 2);
}

#[test]
fn vim_core_insert_ctrl_n_moves_next_line() {
    let mut vim = VimCore::from_text("abc\ndef\n");

    vim.process_key(ch('i'));
    vim.process_key(ctrl('n'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.row, 1);
    assert_eq!(vim.cursor.col, 0);
}

#[test]
fn vim_core_insert_ctrl_p_moves_previous_line() {
    let mut vim = VimCore::from_text("abc\ndef\n");

    vim.process_key(ch('j'));
    vim.process_key(ch('A'));
    vim.process_key(ctrl('p'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.row, 0);
    assert_eq!(vim.cursor.col, 3);
}

#[test]
fn vim_core_insert_alt_f_moves_next_word() {
    let mut vim = VimCore::from_text("one two three\n");

    vim.process_key(ch('i'));
    vim.process_key(alt('f'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.col, 4);
}

#[test]
fn vim_core_insert_alt_b_moves_previous_word() {
    let mut vim = VimCore::from_text("one two three\n");

    vim.process_key(ch('A'));
    vim.process_key(alt('b'));

    assert_eq!(vim.mode, Mode::Insert);
    assert_eq!(vim.cursor.col, 8);
}

#[test]
fn vim_core_command_toggles_relative_number() {
    let mut vim = VimCore::from_text("one\ntwo\n");

    vim.process_key(ch(':'));
    type_keys(&mut vim, "relativenumber");
    vim.process_key(key(KeyCode::Enter));

    assert!(vim.config.relative_number);
    assert_eq!(vim.status_message.as_deref(), Some("relativenumber on"));

    vim.process_key(ch(':'));
    type_keys(&mut vim, "norelativenumber");
    vim.process_key(key(KeyCode::Enter));

    assert!(!vim.config.relative_number);
    assert_eq!(vim.status_message.as_deref(), Some("relativenumber off"));
}

#[test]
fn vim_core_counts_repeat_basic_motions_and_line_ops() {
    let mut vim = VimCore::from_text("abcdef\n");
    type_keys(&mut vim, "3l");
    assert_eq!(vim.cursor.col, 3);

    let mut vim = VimCore::from_text("one\ntwo\nthree\nfour\n");
    type_keys(&mut vim, "3G");
    assert_eq!(vim.cursor.row, 2);

    let mut vim = VimCore::from_text("one\ntwo\nthree\n");
    type_keys(&mut vim, "2dd");
    assert_eq!(vim.text(), "three\n");
    assert_eq!(vim.registers.get(&'"').unwrap().content, "one\ntwo\n");
}

#[test]
fn vim_core_counts_multiply_operator_and_motion_counts() {
    let mut vim = VimCore::from_text("one two three four five six seven\n");

    type_keys(&mut vim, "2d3w");

    assert_eq!(vim.text(), "seven\n");
    assert_eq!(
        vim.registers.get(&'"').unwrap().content,
        "one two three four five six "
    );
}

#[test]
fn vim_core_replaces_single_and_counted_characters() {
    let mut vim = VimCore::from_text("abcdef\n");

    type_keys(&mut vim, "lrZ");
    assert_eq!(vim.text(), "aZcdef\n");

    type_keys(&mut vim, "l3r!");
    assert_eq!(vim.text(), "aZ!!!f\n");
}

#[test]
fn vim_core_zz_centers_the_viewport() {
    let text = (1..=30)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let mut vim = VimCore::from_text(&text);
    vim.view.width = 80;
    vim.view.height = 5;

    type_keys(&mut vim, "20Gzz");

    assert_eq!(vim.cursor.row, 19);
    assert_eq!(vim.view.offset_row, 17);
}

#[test]
fn vim_core_counts_repeat_paste_and_macro_effects() {
    let mut vim = VimCore::from_text("one\ntwo\n");

    type_keys(&mut vim, "yy3p");
    assert_eq!(vim.text(), "one\none\none\none\ntwo\n");

    let mut vim = VimCore::from_text("one\n");
    let effects = type_keys(&mut vim, "2@a");
    assert_eq!(
        effects,
        vec![Effect::PlayMacro('a'), Effect::PlayMacro('a')]
    );
}

#[test]
fn vim_core_ge_and_g_e_move_to_previous_word_ends() {
    let mut vim = VimCore::from_text("one two three\n");
    type_keys(&mut vim, "wwge");
    assert_eq!(vim.cursor.col, 6);

    let mut vim = VimCore::from_text("one two-three four\n");
    type_keys(&mut vim, "WWgE");
    assert_eq!(vim.cursor.col, 12);
}

#[test]
fn vim_core_repeats_find_forward_and_backward() {
    let mut vim = VimCore::from_text("abc abc abc\n");

    type_keys(&mut vim, "fa");
    assert_eq!(vim.cursor.col, 4);
    type_keys(&mut vim, ";");
    assert_eq!(vim.cursor.col, 8);
    type_keys(&mut vim, ",");
    assert_eq!(vim.cursor.col, 4);
}

#[test]
fn vim_core_marks_jump_exact_linewise_and_previous_position() {
    let mut vim = VimCore::from_text("  one\n  two\n");

    type_keys(&mut vim, "llmaj`a");
    assert_eq!((vim.cursor.row, vim.cursor.col), (0, 2));

    type_keys(&mut vim, "``");
    assert_eq!((vim.cursor.row, vim.cursor.col), (1, 2));

    type_keys(&mut vim, "'a");
    assert_eq!((vim.cursor.row, vim.cursor.col), (0, 2));
}

#[test]
fn vim_core_marks_clamp_after_line_deletion() {
    let mut vim = VimCore::from_text("one\ntwo\n");

    type_keys(&mut vim, "jmakdd'a");

    assert_eq!(vim.text(), "two\n");
    assert_eq!(vim.cursor.row, 0);
}

#[test]
fn vim_core_backward_search_sets_n_and_shift_n_direction() {
    let mut vim = VimCore::from_text("one two one\n");

    type_keys(&mut vim, "$?one");
    vim.process_key(key(KeyCode::Enter));
    assert_eq!(vim.cursor.col, 8);

    type_keys(&mut vim, "n");
    assert_eq!(vim.cursor.col, 0);

    type_keys(&mut vim, "N");
    assert_eq!(vim.cursor.col, 8);
}

#[test]
fn vim_core_nohlsearch_and_diagnostics_commands_work() {
    let mut vim = VimCore::from_text("one two one\n");

    type_keys(&mut vim, "/one");
    vim.process_key(key(KeyCode::Enter));
    assert!(!vim.search_matches.is_empty());

    vim.process_key(ch(':'));
    type_keys(&mut vim, "noh");
    vim.process_key(key(KeyCode::Enter));
    assert!(vim.search_matches.is_empty());

    vim.process_key(ch(':'));
    type_keys(&mut vim, "diags");
    assert_eq!(
        vim.process_key(key(KeyCode::Enter)),
        vec![Effect::DiagnosticList]
    );
}

#[test]
fn vim_core_replace_mode_replaces_successive_chars() {
    let mut vim = VimCore::from_text("abcdef\n");

    type_keys(&mut vim, "RXYZ");
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "XYZdef\n");
    assert_eq!(vim.mode, Mode::Normal);
}

#[test]
fn vim_core_window_prefix_exposes_new_pane_commands() {
    let mut vim = VimCore::from_text("one\n");

    assert_eq!(vim.process_key(ctrl('w')), Vec::<Effect>::new());
    assert_eq!(
        vim.process_key(ch('o')),
        vec![Effect::ShowMessage(
            "pane-only is handled by the host editor".to_string()
        )]
    );

    assert_eq!(vim.process_key(ctrl('w')), Vec::<Effect>::new());
    assert_eq!(vim.process_key(ch('H')), vec![Effect::PaneLeft]);
}

#[test]
fn vim_core_registers_track_yank_zero_small_delete_and_delete_rotation() {
    let mut vim = VimCore::from_text("one\ntwo\nthree\n");

    type_keys(&mut vim, "yy");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "one\n");
    assert_eq!(vim.registers.get(&'"').unwrap().content, "one\n");

    type_keys(&mut vim, "dd");
    assert_eq!(vim.registers.get(&'1').unwrap().content, "one\n");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "one\n");

    type_keys(&mut vim, "dd");
    assert_eq!(vim.registers.get(&'1').unwrap().content, "two\n");
    assert_eq!(vim.registers.get(&'2').unwrap().content, "one\n");

    let mut vim = VimCore::from_text("abc\n");
    type_keys(&mut vim, "x");
    assert_eq!(vim.registers.get(&'-').unwrap().content, "a");
    assert_eq!(vim.registers.get(&'"').unwrap().content, "a");
    assert!(!vim.registers.contains_key(&'1'));
}

#[test]
fn vim_core_registers_support_black_hole_and_uppercase_append() {
    let mut vim = VimCore::from_text("one\ntwo\nthree\n");

    type_keys(&mut vim, "\"ayyj\"Ayy");
    assert_eq!(vim.registers.get(&'a').unwrap().content, "one\ntwo\n");
    assert_eq!(vim.registers.get(&'"').unwrap().content, "two\n");

    type_keys(&mut vim, "\"_dd");
    assert_eq!(vim.registers.get(&'a').unwrap().content, "one\ntwo\n");
    assert_eq!(vim.registers.get(&'"').unwrap().content, "two\n");
    assert_eq!(vim.registers.get(&'1').map(|r| r.content.as_str()), None);
}

#[test]
fn vim_core_register_prompt_lists_text_register_contents() {
    let mut vim = VimCore::from_text("one\ntwo\n");

    type_keys(&mut vim, "\"ayyjyy");
    vim.process_key(ch('"'));
    let display = vim.register_display().unwrap();

    assert!(display.starts_with("select register: "));
    assert!(display.contains("\"0:two\\n"));
    assert!(display.contains("\"a:one\\n"));
    assert!(display.contains("\"\":two\\n"));
}

#[test]
fn vim_core_macro_register_prompt_lists_recorded_macros() {
    let mut vim = VimCore::from_text("one\n");
    vim.macros.insert('a', vec![ch('l'), ctrl('w')]);

    vim.process_key(ch('q'));
    let display = vim.register_display().unwrap();

    assert!(display.starts_with("record macro in: "));
    assert!(display.contains("@a:l<C-w>"));
}

#[test]
fn vim_core_visual_text_objects_select_words_quotes_and_paragraphs() {
    let mut vim = VimCore::from_text("alpha beta\n");
    type_keys(&mut vim, "viwy");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "alpha");

    let mut vim = VimCore::from_text("say \"hello world\" now\n");
    type_keys(&mut vim, "f\"lvi\"y");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "hello world");

    let mut vim = VimCore::from_text("say \"hello\" now\n");
    type_keys(&mut vim, "f\"lva\"y");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "\"hello\"");

    let mut vim = VimCore::from_text("one\ntwo\n\nthree\n");
    type_keys(&mut vim, "vipy");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "one\ntwo");
}

#[test]
fn vim_core_visual_text_objects_select_tags_arguments_and_big_words() {
    let mut vim = VimCore::from_text("<p>Hello <b>world</b></p>\n");
    type_keys(&mut vim, "lllvity");
    assert_eq!(
        vim.registers.get(&'0').unwrap().content,
        "Hello <b>world</b>"
    );

    let mut vim = VimCore::from_text("call(one, two, three)\n");
    type_keys(&mut vim, "ftvi,y");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "two");

    let mut vim = VimCore::from_text("one two-three four\n");
    type_keys(&mut vim, "wviWy");
    assert_eq!(vim.registers.get(&'0').unwrap().content, "two-three");
}

#[test]
fn vim_core_visual_yank_respects_selected_register() {
    let mut vim = VimCore::from_text("alpha beta\n");

    type_keys(&mut vim, "viw\"ay");

    assert_eq!(vim.registers.get(&'a').unwrap().content, "alpha");
    assert_eq!(vim.registers.get(&'0'), None);
    assert_eq!(vim.registers.get(&'"').unwrap().content, "alpha");
}

#[test]
fn vim_core_visual_block_insert_applies_to_each_selected_row() {
    let mut vim = VimCore::from_text("one\ntwo\nthree\n");

    vim.process_key(ctrl('v'));
    type_keys(&mut vim, "jjI//");
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "//one\n//two\n//three\n");
    assert_eq!(vim.mode, Mode::Normal);
}

#[test]
fn vim_core_visual_block_append_applies_to_each_selected_row() {
    let mut vim = VimCore::from_text("ab\ncd\nef\n");

    vim.process_key(ctrl('v'));
    type_keys(&mut vim, "jlAX");
    vim.process_key(key(KeyCode::Esc));

    assert_eq!(vim.text(), "abX\ncdX\nef\n");
    assert_eq!(vim.mode, Mode::Normal);
}
