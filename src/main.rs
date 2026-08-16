mod app;
mod catalog;
mod download;
mod engine;
mod paths;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 620.0])
            .with_app_id("io.codex.chess-voice-studio"),
        ..Default::default()
    };

    eframe::run_native(
        "Chess Voice Studio",
        options,
        Box::new(|creation_context| Ok(Box::new(app::ChessVoiceApp::new(creation_context)))),
    )
}
