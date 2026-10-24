//! Typed commands from the interface to the engine.
//!
//! Each command runs on the async runtime, so the render loop never waits on
//! storage. A failure comes back as a [`Message`] for the interface to show.

use std::sync::Arc;
use std::sync::mpsc::Sender;

use launcher_core::Engine;
use launcher_model::ThemePreference;
use tokio::runtime::Handle;

/// What a finished command reports back to the interface.
#[derive(Debug)]
pub(crate) enum Message {
	/// A user-safe description of something that failed.
	Failed(String),
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
		Self {
			engine,
			runtime,
			messages,
			context,
		}
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
}
