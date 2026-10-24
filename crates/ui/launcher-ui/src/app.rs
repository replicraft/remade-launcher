use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{Context, Panel, TextureHandle, TextureOptions};
use launcher_core::Engine;
use launcher_model::ThemePreference;
use tokio::runtime::Handle;

use crate::commands::{Commands, Message};
use crate::screens::{self, ExploreView, Screen};
use crate::{branding, theme};

pub(crate) struct LauncherApp {
	commands: Commands,
	messages: Receiver<Message>,
	logo: TextureHandle,

	screen: Screen,
	explore: ExploreView,
	theme: ThemePreference,
	notice: Option<String>,
}

impl LauncherApp {
	pub(crate) fn new(
		creation: &eframe::CreationContext<'_>,
		engine: Arc<Engine>,
		runtime: Handle,
		initial_theme: ThemePreference,
	) -> Self {
		let context = &creation.egui_ctx;

		theme::install(context);
		theme::select(context, initial_theme);

		let (sender, messages) = mpsc::channel();
		let logo = context.load_texture("logo", branding::logo_image(), TextureOptions::LINEAR);

		Self {
			commands: Commands::new(engine, runtime, sender, context.clone()),
			messages,
			logo,
			screen: Screen::Explore,
			explore: ExploreView::new(),
			theme: initial_theme,
			notice: None,
		}
	}

	fn drain_messages(&mut self) {
		while let Ok(message) = self.messages.try_recv() {
			match message {
				Message::Failed(text) => self.notice = Some(text),
				Message::Explore { request, rows } => self.explore.receive(request, rows),
			}
		}
	}

	fn change_theme(&mut self, context: &Context, theme: ThemePreference) {
		self.theme = theme;

		theme::select(context, theme);
		self.commands.set_theme(theme);
	}
}

impl eframe::App for LauncherApp {
	fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
		let context = ui.ctx().clone();

		self.drain_messages();

		Panel::left("navigation")
			.resizable(false)
			.exact_size(theme::SIDEBAR_WIDTH)
			.show(ui, |ui| {
				screens::navigation(ui, &self.logo, &mut self.screen)
			});

		egui::CentralPanel::default().show(ui, |ui| match self.screen {
			Screen::Explore => self.explore.show(ui, &self.commands),
			Screen::Library => screens::library(ui),
			Screen::Settings => {
				let chosen = screens::settings(ui, self.theme, self.notice.as_deref());

				if let Some(theme) = chosen {
					self.change_theme(&context, theme);
				}
			}
		});
	}
}
