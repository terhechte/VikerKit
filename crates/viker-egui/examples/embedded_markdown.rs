use std::path::PathBuf;

use anyhow::Result;
use eframe::egui;
use viker_core::config::Config;
use viker_core::editor::Editor;
use viker_core::editor::document::Document;
use viker_egui::VikerEditor;

const SAMPLE_MARKDOWN: &str = r#"# Embedded Viker

This example hosts the Vim-style editor in the main pane and keeps the rest of
the egui window available for ordinary application UI.

## Draft

- Edit this text with Vim bindings.
- The preview and stats on the right are regular egui widgets.
- Try splitting this layout further for a terminal, outline, or inspector.

```rust
fn main() {
    println!("Viker can live inside a larger egui app");
}
```
"#;

fn main() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viker embedded markdown example")
            .with_inner_size([1200.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Viker embedded markdown example",
        options,
        Box::new(|_cc| Ok(Box::new(EmbeddedMarkdownApp::new()))),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))
}

struct EmbeddedMarkdownApp {
    editor: VikerEditor,
}

impl EmbeddedMarkdownApp {
    fn new() -> Self {
        let document = Document {
            rope: ropey::Rope::from_str(SAMPLE_MARKDOWN),
            path: Some(PathBuf::from("embedded-demo.md")),
            modified: false,
            version: 0,
        };

        let config = Config {
            wrap: true,
            relative_number: true,
            font_size: 15.0,
            ..Config::default()
        };

        let editor = Editor::with_config(document, config);

        Self {
            editor: VikerEditor::from_editor(editor),
        }
    }

    fn markdown(&self) -> String {
        self.editor.editor().document.rope.to_string()
    }
}

impl eframe::App for EmbeddedMarkdownApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::right("preview")
            .resizable(true)
            .default_width(380.0)
            .width_range(260.0..=560.0)
            .show(ctx, |ui| {
                let markdown = self.markdown();
                draw_stats(ui, &markdown);

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(12.0);

                egui::ScrollArea::vertical()
                    .id_salt("markdown_preview_scroll")
                    .show(ui, |ui| {
                        draw_markdown_preview(ui, &markdown);
                    });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(40, 44, 52)))
            .show(ctx, |ui| {
                self.editor.show_inside(ui);
            });
    }
}

fn draw_stats(ui: &mut egui::Ui, markdown: &str) {
    let words = markdown.split_whitespace().count();
    let chars = markdown.chars().count();
    let lines = markdown.lines().count();
    let headings = markdown
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();

    ui.heading("Document");
    egui::Grid::new("document_stats")
        .num_columns(2)
        .spacing([18.0, 6.0])
        .show(ui, |ui| {
            ui.label("Lines");
            ui.monospace(lines.to_string());
            ui.end_row();

            ui.label("Words");
            ui.monospace(words.to_string());
            ui.end_row();

            ui.label("Characters");
            ui.monospace(chars.to_string());
            ui.end_row();

            ui.label("Headings");
            ui.monospace(headings.to_string());
            ui.end_row();
        });
}

fn draw_markdown_preview(ui: &mut egui::Ui, markdown: &str) {
    let mut in_code_block = false;
    let mut code = String::new();

    for line in markdown.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            if in_code_block {
                draw_code_block(ui, &code);
                code.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code.push_str(line);
            code.push('\n');
            continue;
        }

        if let Some(text) = trimmed.strip_prefix("# ") {
            ui.add_space(4.0);
            ui.heading(text);
        } else if let Some(text) = trimmed.strip_prefix("## ") {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(text).size(20.0).strong());
        } else if let Some(text) = trimmed.strip_prefix("### ") {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(text).size(17.0).strong());
        } else if let Some(text) = trimmed.strip_prefix("- ") {
            ui.horizontal_wrapped(|ui| {
                ui.label("•");
                ui.label(text);
            });
        } else if trimmed.is_empty() {
            ui.add_space(8.0);
        } else {
            ui.label(line);
        }
    }

    if in_code_block && !code.is_empty() {
        draw_code_block(ui, &code);
    }
}

fn draw_code_block(ui: &mut egui::Ui, code: &str) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgb(31, 34, 40))
        .inner_margin(egui::Margin::same(10))
        .corner_radius(4.0);

    frame.show(ui, |ui| {
        ui.monospace(code.trim_end());
    });
}
