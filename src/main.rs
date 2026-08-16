mod app;
mod catalog;
mod download;
mod engine;
mod paths;

use eframe::egui;

fn main() -> eframe::Result {
    let paths = paths::AppPaths::discover().map_err(app_creation_error)?;
    paths.ensure().map_err(app_creation_error)?;
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        persistence_path: Some(paths.state_path.clone()),
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([900.0, 620.0])
            .with_app_id("io.codex.chess-voice-studio"),
        ..Default::default()
    };

    eframe::run_native(
        "Chess Voice Studio",
        options,
        Box::new(move |creation_context| {
            Ok(Box::new(app::ChessVoiceApp::new(creation_context, paths)))
        }),
    )
}

fn app_creation_error(message: String) -> eframe::Error {
    eframe::Error::AppCreation(Box::new(std::io::Error::other(message)))
}
