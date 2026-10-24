use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{Context, Panel, TextureHandle, TextureOptions};
use launcher_background::control::BackgroundStatus;
use launcher_core::{
	Engine, EngineEvent, InstalledApp, Preparation, Stage, UpdateSettings, UpdateStatus,
};
use launcher_model::{InstallationId, ThemePreference, UpdateMode};
use launcher_storage::AppPolicy;
use tokio::runtime::Handle;

use crate::commands::{Commands, Message};
use crate::screens::sheet::{self, Outcome, Sheet};
use crate::screens::{
	self, Action, AddView, ExploreView, LibraryView, Screen, SettingsAction, SettingsView,
};
use crate::{branding, theme};

pub(crate) struct LauncherApp {
	commands: Commands,
	context: Context,
	messages: Receiver<Message>,
	logo: TextureHandle,

	screen: Screen,
	explore: ExploreView,
	add: AddView,
	installed: Vec<InstalledApp>,
	progress: HashMap<InstallationId, screens::library::Progress>,
	sheet: Option<Sheet>,
	/// The app an update review belongs to.
	update_of: Option<InstallationId>,
	theme: ThemePreference,
	notice: Option<String>,
	update_settings: UpdateSettings,
	background: Option<BackgroundStatus>,
	statuses: HashMap<InstallationId, UpdateStatus>,
	policies: HashMap<InstallationId, AppPolicy>,
	custom_age: String,
	age_error: Option<String>,
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
			context: context.clone(),
			messages,
			logo,
			screen: Screen::Explore,
			explore: ExploreView::new(),
			add: AddView::default(),
			installed: Vec::new(),
			progress: HashMap::new(),
			sheet: None,
			update_of: None,
			theme: initial_theme,
			notice: None,
			update_settings: UpdateSettings {
				mode: UpdateMode::Manual,
				minimum_age: launcher_model::MinimumAge::DEFAULT,
			},
			background: None,
			statuses: HashMap::new(),
			policies: HashMap::new(),
			custom_age: String::new(),
			age_error: None,
		}
	}

	fn drain_messages(&mut self) {
		while let Ok(message) = self.messages.try_recv() {
			match message {
				Message::Failed(text) => {
					self.notice = Some(text);
					self.add.lookup_failed();

					// A failed lookup or plan should not leave a spinner up.
					if matches!(self.sheet, Some(Sheet::Working(_))) {
						self.sheet = None;
					}
				}
				Message::Explore { request, rows } => self.explore.receive(request, rows),
				Message::Installed(apps) => self.installed = apps,
				Message::Prepared(preparation) => self.show_preparation(preparation),
				Message::CustomLookedUp(lookup) => self.add.set_lookup(lookup),
				Message::CustomChosen(ready) => {
					self.update_of = None;
					self.sheet = Some(Sheet::Review {
						ready,
						update_of: None,
						acknowledged: false,
					});
				}
				Message::Uninstalled { name, report } => {
					self.sheet = Some(Sheet::Removed { name, report });
					self.commands.refresh_installed();
				}
				Message::Engine(event) => self.handle_event(event),
				Message::UpdateInfo(settings, background) => {
					self.update_settings = *settings;
					self.background = Some(*background);
				}
				Message::Statuses(statuses) => {
					self.statuses = statuses
						.into_iter()
						.map(|status| (status.installation, status))
						.collect();
				}
				Message::Policies(policies) => self.policies = policies,
				Message::Notice(text) => {
					if !text.is_empty() {
						self.notice = Some(text);
					}
				}
				Message::Diagnostics(text) => {
					self.context.copy_text(text);
					self.sheet = Some(Sheet::Message {
						title: "Copied".into(),
						lines: vec!["A support report is on your clipboard. It has no file paths or app names.".into()],
					});
				}
			}
		}
	}

	fn show_preparation(&mut self, preparation: Preparation) {
		self.sheet = Some(match preparation {
			Preparation::Ready(ready) => Sheet::Review {
				ready,
				update_of: self.update_of.take(),
				acknowledged: false,
			},
			Preparation::Choose(candidates) => Sheet::Choose(candidates),
			Preparation::Unavailable(reason) => Sheet::Message {
				title: "Not available".into(),
				lines: vec![reason],
			},
		});
	}

	fn handle_event(&mut self, event: EngineEvent) {
		let EngineEvent::Operation(event) = event else {
			return;
		};

		let (label, fraction) = match &event.stage {
			Stage::Accepted => ("Starting".to_owned(), None),
			Stage::Downloading { received, total } => (
				format!("Downloading {}", screens::sheet::bytes(*received)),
				total.map(|total| (*received as f64 / total.max(1) as f64) as f32),
			),
			Stage::Verifying => ("Checking the download".into(), None),
			Stage::Inspecting => ("Inspecting the package".into(), None),
			Stage::Extracting => ("Unpacking".into(), None),
			Stage::Applying => ("Finishing".into(), None),
			Stage::Removing => ("Removing".into(), None),
			Stage::Completed | Stage::Failed { .. } | Stage::RecoveryRequired { .. } => {
				self.progress.remove(&event.installation);
				// What was true before this operation no longer is.
				self.statuses.remove(&event.installation);
				self.commands.refresh_installed();

				if let Stage::Failed { message } | Stage::RecoveryRequired { message } =
					&event.stage
				{
					self.notice = Some(message.clone());
				}

				return;
			}
		};

		self.progress.insert(
			event.installation,
			screens::library::Progress { label, fraction },
		);
	}

	fn perform(&mut self, action: Action) {
		match action {
			Action::Install { slug } => {
				self.update_of = None;
				self.sheet = Some(Sheet::Working("Checking the latest release…".into()));
				self.commands.prepare_install(slug);
			}
			Action::Update(id) => {
				self.update_of = Some(id);
				self.sheet = Some(Sheet::Working("Checking for a newer version…".into()));
				self.commands.prepare_update(id);
			}
			Action::Launch(id) => self.commands.launch(id),
			Action::SetPolicy(id, policy) => self.commands.set_app_policy(id, policy),
			Action::Pin(id, pinned) => self.commands.set_pinned(id, pinned),
			Action::CheckUpdates => self.commands.check_updates(),
			Action::Uninstall(app) => self.sheet = Some(Sheet::ConfirmUninstall(app)),
			Action::LookUp(url) => self.commands.look_up_custom(url),
			Action::ChooseFile {
				preparation,
				asset_id,
			} => {
				self.sheet = Some(Sheet::Working("Preparing…".into()));
				self.commands.choose_custom_file(*preparation, asset_id);
			}
		}
	}

	fn show_sheet(&mut self, context: &Context) {
		let Some(current) = &mut self.sheet else {
			return;
		};

		match sheet::show(context, current) {
			Outcome::Stay => {}
			Outcome::Close => self.sheet = None,
			Outcome::Install {
				ready,
				update_of,
				acknowledged,
			} => {
				self.sheet = None;
				self.commands.install(*ready, acknowledged, update_of);
			}
			Outcome::Uninstall(app) => {
				self.sheet = None;
				self.commands.uninstall(app);
			}
			Outcome::SetMode { mode, acknowledged } => {
				self.sheet = None;
				self.commands.set_update_mode(mode, acknowledged);
			}
		}
	}

	/// Switching to Manual needs no question. Anything automatic asks first,
	/// with the box unchecked.
	fn pick_mode(&mut self, mode: UpdateMode) {
		if mode.is_automatic() {
			self.sheet = Some(Sheet::ConfirmMode {
				mode,
				acknowledged: false,
			});
		} else {
			self.commands.set_update_mode(mode, false);
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

		let mut action = None;

		egui::CentralPanel::default().show(ui, |ui| {
			if let Some(notice) = self.notice.clone() {
				ui.horizontal(|ui| {
					ui.colored_label(ui.visuals().error_fg_color, notice);

					if ui.small_button("Dismiss").clicked() {
						self.notice = None;
					}
				});
			}

			match self.screen {
				Screen::Explore => action = self.explore.show(ui, &self.commands),
				Screen::Library => {
					action = screens::library(
						ui,
						&LibraryView {
							apps: &self.installed,
							busy: &self.progress,
							statuses: &self.statuses,
							policies: &self.policies,
							mode: self.update_settings.mode,
							global_age: self.update_settings.minimum_age,
						},
					);
				}
				Screen::Add => action = self.add.show(ui),
				Screen::Settings => {
					let view = SettingsView {
						theme: self.theme,
						updates: self.update_settings,
						background: self.background.as_ref(),
						custom_age: &mut self.custom_age,
						age_error: self.age_error.as_deref(),
					};

					match screens::settings(ui, view) {
						SettingsAction::None => {}
						SettingsAction::Theme(theme) => self.change_theme(&context, theme),
						SettingsAction::CopyDiagnostics => self.commands.diagnostics(),
						SettingsAction::PickMode(mode) => self.pick_mode(mode),
						SettingsAction::SetMinimumAge(age) => {
							self.age_error = None;
							self.commands.set_minimum_age(age);
						}
						SettingsAction::BadAge(why) => self.age_error = Some(why),
						SettingsAction::StopBackground => self.commands.stop_background_updates(),
					}
				}
			}
		});

		if let Some(action) = action {
			self.perform(action);
		}

		self.show_sheet(&context);
	}
}
