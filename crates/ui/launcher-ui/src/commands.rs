//! Typed commands from the interface to the engine.
//!
//! Each command runs on the async runtime, so the render loop never waits on
//! storage, the network, or search. Results come back as [`Message`]s, and
//! the interface repaints when one arrives.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use launcher_core::{CatalogState, Engine, ExploreQuery, ExploreRow};
use launcher_model::ThemePreference;
use tokio::runtime::Handle;

/// What a finished command reports back to the interface.
#[derive(Debug)]
pub(crate) enum Message {
	/// A user-safe description of something that failed.
	Failed(String),
	/// Results for the search request numbered `request`.
	Explore { request: u64, rows: Vec<ExploreRow> },
}

/// A handle for submitting commands.
pub(crate) struct Commands {
	engine: Arc<Engine>,
	runtime: Handle,
	messages: Sender<Message>,
	context: egui::Context,
}

impl Commands {
	pub(crate) fn new(
		engine: Arc<Engine>,
		runtime: Handle,
		messages: Sender<Message>,
		context: egui::Context,
	) -> Self {
		let commands = Self {
			engine,
			runtime,
			messages,
			context,
		};

		commands.repaint_on_catalog_changes();
		commands.refresh_catalog();

		commands
	}

	/// The latest catalog state. Never blocks.
	pub(crate) fn catalog_state(&self) -> Arc<CatalogState> {
		self.engine.catalog().state()
	}

	pub(crate) fn refresh_catalog(&self) {
		let engine = Arc::clone(&self.engine);

		self.runtime
			.spawn(async move { engine.catalog().refresh().await });
	}

	pub(crate) fn search(&self, request: u64, query: ExploreQuery) {
		let engine = Arc::clone(&self.engine);
		let messages = self.messages.clone();
		let context = self.context.clone();

		self.runtime.spawn(async move {
			let rows = engine.explore(query).await;

			let _ = messages.send(Message::Explore { request, rows });
			context.request_repaint();
		});
	}

	pub(crate) fn set_theme(&self, theme: ThemePreference) {
		let engine = Arc::clone(&self.engine);
		let messages = self.messages.clone();
		let context = self.context.clone();

		self.runtime.spawn(async move {
			if let Err(error) = engine.set_theme(theme).await {
				tracing::warn!(%error, "could not save the theme");

				let _ = messages.send(Message::Failed(error.user_message().to_owned()));
				context.request_repaint();
			}
		});
	}

	/// Wakes the interface whenever the catalog state changes.
	fn repaint_on_catalog_changes(&self) {
		let mut changes = self.engine.catalog().subscribe();
		let context = self.context.clone();

		self.runtime.spawn(async move {
			while changes.changed().await.is_ok() {
				context.request_repaint();
			}
		});
	}
}
