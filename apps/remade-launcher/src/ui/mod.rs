//! The Remade interface.
//!
//! The interface never touches storage, the network, or the filesystem
//! itself. It sends typed commands to the engine through [`commands`] and
//! renders what comes back.

mod app;
mod branding;
mod commands;
mod components;
mod demo;
mod icons;
mod menu;
mod notifications;
mod screens;
mod theme;
mod transfers;

use std::sync::Arc;

use launcher_core::Engine;
use launcher_model::product;
use tokio::runtime::Handle;

/// Opens the main window and blocks until it is closed.
///
/// This must run on the main thread. `runtime` is where the interface spawns
/// its engine commands.
pub fn run(engine: Arc<Engine>, runtime: Handle) -> eframe::Result {
	let initial_theme = runtime.block_on(engine.theme());
	let preferences = runtime.block_on(engine.preferences());
	// Open at the size the window had when it was last closed.
	let size = preferences
		.layout
		.window
		.map_or([1280.0, 820.0], |(width, height)| [width, height]);

	let options = eframe::NativeOptions {
		viewport: egui::ViewportBuilder::default()
			.with_title(product::NAME)
			.with_app_id(product::APP_ID)
			.with_icon(branding::window_icon())
			.with_inner_size(size)
			.with_min_inner_size([880.0, 600.0]),
		..Default::default()
	};

	eframe::run_native(
		product::NAME,
		options,
		Box::new(move |creation| {
			Ok(Box::new(app::LauncherApp::new(
				creation,
				engine,
				runtime,
				initial_theme,
			)))
		}),
	)
}
