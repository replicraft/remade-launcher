use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{Context, Key, Modifiers, Panel, TextureHandle, TextureOptions};
use launcher_background::control::BackgroundStatus;
use launcher_core::{
	Engine, EngineEvent, InstalledApp, OperationSummary, Preparation, Stage, UpdateSettings,
	UpdateStatus,
};
use launcher_model::{InstallationId, ThemePreference, UpdateMode};
use launcher_storage::AppPolicy;
use tokio::runtime::Handle;

use crate::commands::{Commands, Message};
use crate::screens::navigation::{self, NavAction, Sidebar};
use crate::screens::sheet::{self, Outcome, Sheet};
use crate::screens::{
	self, Action, AddView, ExploreView, LibraryView, Screen, SettingsAction, SettingsView,
	UpdatesView,
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
	/// The Add from GitHub dialog is showing.
	adding: bool,
	/// What the sidebar search holds.
	search: String,
	history: Vec<OperationSummary>,
	/// An update check has finished this session.
	checked: bool,
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
			adding: false,
			search: String::new(),
			history: Vec::new(),
			checked: false,
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
					self.checked = true;
					self.statuses = statuses
						.into_iter()
						.map(|status| (status.installation, status))
						.collect();
				}
				Message::Policies(policies) => self.policies = policies,
				Message::History(history) => self.history = history,
				Message::Notes { app, version, text } => {
					self.sheet = Some(Sheet::Notes { app, version, text });
				}
				Message::UpdatesPlanned(planned) => {
					let ready: Vec<_> = planned
						.into_iter()
						.filter(|item| item.ready.is_some())
						.collect();

					self.sheet =
						Some(if ready.is_empty() {
							Sheet::Message {
			title: "Nothing to update".into(),
			lines: vec!["No installed app has an update that can be installed right now.".into()],
		}
						} else {
							Sheet::ConfirmUpdateAll(ready)
						});
				}
				Message::Restored(settings, background) => {
					self.update_settings = *settings;
					self.background = Some(*background);
					self.policies.clear();
					self.statuses.clear();
					self.change_theme(&self.context.clone(), ThemePreference::default());
					self.notice = None;
					self.sheet = Some(Sheet::Message {
						title: "Defaults restored".into(),
						lines: vec![
							"Appearance and update settings are back to their defaults.".into(),
						],
					});
				}
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
				self.commands.refresh_history();

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
			Action::UpdateAll => {
				self.sheet = Some(Sheet::Working("Checking what can be updated…".into()));
				self.commands.plan_all_updates();
			}
			Action::ReleaseNotes { slug, tag, app } => {
				self.sheet = Some(Sheet::Working("Loading release notes…".into()));
				self.commands.release_notes(slug, tag, app);
			}
			Action::ShowInLibrary => {
				self.search.clear();
				self.screen = Screen::Library;
			}
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
			Outcome::UpdateAll(updates) => {
				self.sheet = None;
				self.commands.apply_updates(updates);
			}
			Outcome::RestoreDefaults => {
				self.sheet = None;
				self.commands.restore_defaults();
			}
		}
	}

	/// Installed apps whose name or repository contains the search text.
	fn installed_matches(&self) -> Vec<InstalledApp> {
		let needle = self.search.trim().to_lowercase();

		if needle.is_empty() {
			return Vec::new();
		}

		self.installed
			.iter()
			.filter(|app| {
				app.name.to_lowercase().contains(&needle)
					|| app.repository.to_lowercase().contains(&needle)
			})
			.cloned()
			.collect()
	}

	fn go(&mut self, screen: Screen) {
		if screen == Screen::Updates {
			self.commands.refresh_history();
		}

		self.screen = screen;
	}

	/// The Add from GitHub dialog.
	fn show_add(&mut self, context: &Context) {
		if !self.adding {
			return;
		}

		let mut action = None;
		let mut close = false;

		let response = egui::Modal::new(egui::Id::unique("add-dialog")).show(context, |ui| {
			ui.set_width(520.0);

			action = self.add.show(ui);

			if ui.button("Close").clicked() {
				close = true;
			}
		});

		if close || response.should_close() {
			self.adding = false;
		}

		if let Some(action) = action {
			// Choosing a file hands over to the review sheet.
			if matches!(action, Action::ChooseFile { .. }) {
				self.adding = false;
			}

			self.perform(action);
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

		// Cmd/Ctrl+K jumps to the search field from anywhere.
		if context.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::K)) {
			context.memory_mut(|memory| memory.request_focus(navigation::search_id()));
		}

		let waiting = screens::updates::available(&self.statuses).len();
		let mut go = None;

		let sidebar_fill = theme::palette_for(&context).sidebar;

		Panel::left("navigation")
			.frame(egui::Frame::new().fill(sidebar_fill))
			.resizable(false)
			.exact_size(theme::SIDEBAR_WIDTH)
			.show(ui, |ui| {
				let sidebar = Sidebar {
					logo: &self.logo,
					current: self.screen,
					updates: waiting,
					search: &mut self.search,
				};

				match screens::navigation(ui, sidebar) {
					NavAction::None => {}
					NavAction::Go(screen) => go = Some(screen),
					NavAction::AddFromGithub => self.adding = true,
					NavAction::Searched => {
						if !self.search.trim().is_empty() {
							go = Some(Screen::Explore);
						}
					}
				}
			});

		if let Some(screen) = go {
			self.go(screen);
		}

		let mut action = None;
		let matches = self.installed_matches();

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
				Screen::Explore => {
					action = self
						.explore
						.show(ui, &self.commands, &self.search, &matches);
				}
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
				Screen::Updates => {
					action = screens::updates(
						ui,
						&UpdatesView {
							statuses: &self.statuses,
							busy: &self.progress,
							history: &self.history,
							mode: self.update_settings.mode,
							checked: self.checked,
						},
					);
				}
				Screen::Settings => {
					let view = SettingsView {
						theme: self.theme,
						updates: self.update_settings,
						background: self.background.as_ref(),
					};

					match screens::settings(ui, view) {
						SettingsAction::None => {}
						SettingsAction::Theme(theme) => self.change_theme(&context, theme),
						SettingsAction::CopyDiagnostics => self.commands.diagnostics(),
						SettingsAction::PickMode(mode) => self.pick_mode(mode),
						SettingsAction::SetMinimumAge(age) => self.commands.set_minimum_age(age),
						SettingsAction::RestoreDefaults => self.sheet = Some(Sheet::ConfirmRestore),
						SettingsAction::StopBackground => self.commands.stop_background_updates(),
					}
				}
			}
		});

		if let Some(action) = action {
			self.perform(action);
		}

		self.show_add(&context);
		self.show_sheet(&context);
	}
}
