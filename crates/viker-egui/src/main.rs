use anyhow::Result;
use viker_core::config;
use viker_egui::gui_app::GuiApp;

fn main() -> Result<()> {
    let path = std::env::args().nth(1);
    let config_result = config::Config::load();

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Viker")
            .with_inner_size([1024.0, 768.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Viker",
        native_options,
        Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, path, config_result)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
