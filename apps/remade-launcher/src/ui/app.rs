use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use egui::{Context, Key, Modifiers, Panel, TextureHandle, TextureOptions};
use launcher_background::control::BackgroundStatus;
use launcher_core::{
	Engine, EngineEvent, InstalledApp, OperationSummary, Preparation, Stage, UpdateSettings,
	UpdateStatus,
};
use launcher_model::{
	InstallationId, Preferences, StartPage, ThemePreference, UpdateMode, product,
};
use launcher_storage::AppPolicy;
use tokio::runtime::Handle;

use crate::ui::commands::{Commands, Message};
use crate::ui::components::{ViewMode, dialog_close, dialog_ok};
use crate::ui::demo::{self, Demo};
use crate::ui::menu::{AppMenu, MenuCommand};
use crate::ui::notifications;
use crate::ui::screens::navigation::{self, NavAction, Sidebar};
use crate::ui::screens::sheet::{self, Outcome, Sheet};
use crate::ui::screens::{
	self, Action, AddView, ExploreView, LibraryState, LibraryView, Screen, SettingsAction,
	SettingsInputs, SettingsView, UpdatesView,
};
use crate::ui::transfers::{TransferKey, Transfers};
use crate::ui::{branding, theme};

/// A pretend download, played out over a few seconds.
struct DemoDownload {
	key: TransferKey,
	slug: String,
	name: String,
	/// The installed app being updated, if this is an update.
	updating: Option<InstallationId>,
	total: u64,
	started: std::time::Instant,
}

pub(crate) struct LauncherApp {
	commands: Commands,
	context: Context,
	messages: Receiver<Message>,
	logo: TextureHandle,
	menu: AppMenu,
	/// Invented data standing in for the real engine's, when asked for.
	demo: Option<Demo>,
	sidebar_collapsed: bool,
	/// Bring the About section into view the next time Settings draws.
	show_about: bool,

	screen: Screen,
	explore: ExploreView,
	view_mode: ViewMode,
	add: AddView,
	/// The Add from GitHub dialog is showing.
	adding: bool,
	library: LibraryState,
	/// The app whose update settings dialog is open.
	app_settings: Option<InstallationId>,
	/// The person has picked a screen, so startup must not move them.
	navigated: bool,
	/// Installs and updates in flight, shown in the tray at the bottom.
	transfers: Transfers,
	/// Demo downloads in flight, so a download can be watched.
	demo_downloads: Vec<DemoDownload>,
	/// Demo uninstalls in flight, so the removal can be watched.
	demo_removals: Vec<(InstalledApp, std::time::Instant)>,
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

	/// What the person has chosen in Settings, as last saved.
	prefs: Preferences,
	prefs_loaded: bool,
	/// The first list of installed apps has arrived, so the start page can be
	/// chosen knowing whether there are any.
	installed_loaded: bool,
	/// The layout changed at this moment. It is saved once it settles.
	layout_changed_at: Option<std::time::Instant>,
	settings_inputs: SettingsInputs,
	token_state: launcher_core::TokenState,
	rate_limit: Option<Result<launcher_core::RateLimit, String>>,
	storage: Option<launcher_core::StorageReport>,
	/// How many updates the last notification mentioned, so the same ones are
	/// not announced twice.
	notified_updates: usize,
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
			menu: AppMenu::install(context),
			demo: demo::enabled()
				.then(|| Demo::new(time::OffsetDateTime::now_utc().unix_timestamp())),
			sidebar_collapsed: false,
			show_about: false,
			screen: Screen::Explore,
			explore: ExploreView::new(),
			view_mode: ViewMode::default(),
			add: AddView::default(),
			adding: false,
			library: LibraryState::default(),
			app_settings: None,
			navigated: false,
			demo_removals: Vec::new(),
			transfers: Transfers::default(),
			demo_downloads: Vec::new(),
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
			prefs: Preferences::default(),
			prefs_loaded: false,
			installed_loaded: false,
			layout_changed_at: None,
			settings_inputs: SettingsInputs::default(),
			token_state: launcher_core::TokenState::Absent,
			rate_limit: None,
			storage: None,
			notified_updates: 0,
		}
	}

	fn drain_messages(&mut self) {
		while let Ok(message) = self.messages.try_recv() {
			match message {
				Message::Failed(text) => {
					self.notice = Some(text);
					self.add.lookup_failed();
					self.transfers.drop_waiting();

					// A failed lookup or plan should not leave a spinner up.
					if matches!(self.sheet, Some(Sheet::Working(_))) {
						self.sheet = None;
					}
				}
				Message::Explore { .. }
				| Message::Installed(_)
				| Message::Statuses(_)
				| Message::Policies(_)
				| Message::History(_)
					if self.demo.is_some() => {}
				Message::Explore { request, rows } => self.explore.receive(request, rows),
				Message::Installed(apps) => {
					self.installed = apps;
					self.installed_loaded = true;
					self.choose_start_page();
				}
				Message::Preferences(prefs) => self.receive_preferences(*prefs),
				Message::Token(state) => {
					self.token_state = state;
					self.rate_limit = None;
				}
				Message::RateLimit(result) => self.rate_limit = Some(result),
				Message::Storage(report) => self.storage = Some(*report),
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
					self.notify_about_updates();
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
					self.commands.refresh_preferences();
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

	/// In demo mode the invented data always wins over whatever the engine
	/// reports, so nothing real leaks into the screens.
	fn apply_demo(&mut self) {
		let Some(demo) = &self.demo else {
			return;
		};

		self.installed.clone_from(&demo.installed);
		self.statuses.clone_from(&demo.statuses);
		self.policies.clone_from(&demo.policies);
		self.history.clone_from(&demo.history);
		self.checked = true;

		self.installed_loaded = true;
		self.choose_start_page();
		self.advance_demo_downloads();
		self.advance_demo_removals();
	}

	/// Starts a pretend download for a catalog app, or for an installed one
	/// that is being updated.
	fn start_demo_download(&mut self, slug: String, updating: Option<InstallationId>) {
		let Some(demo) = &self.demo else {
			return;
		};

		let name = demo
			.catalog
			.iter()
			.find(|row| row.slug == slug)
			.map_or_else(|| slug.clone(), |row| row.name.clone());
		let key = match updating {
			Some(id) => TransferKey::Installation(id),
			None => TransferKey::Slug(slug.clone()),
		};

		// Installing something twice at once would only confuse the demo.
		if self
			.demo_downloads
			.iter()
			.any(|download| download.key == key)
		{
			return;
		}

		self.transfers
			.begin(key.clone(), name.clone(), Some(slug.clone()), "Starting");
		self.demo_downloads.push(DemoDownload {
			total: demo::download_size(&slug),
			key,
			slug,
			name,
			updating,
			started: std::time::Instant::now(),
		});
	}

	/// Plays each pretend download: a download at a steady speed, a check,
	/// then unpacking, and then the app appears in the Library.
	fn advance_demo_downloads(&mut self) {
		const SPEED: f64 = 9_500_000.0;

		if self.demo_downloads.is_empty() {
			return;
		}

		let mut finished = Vec::new();

		for download in &self.demo_downloads {
			let seconds = download.started.elapsed().as_secs_f64();
			let download_time = download.total as f64 / SPEED;
			let total = Some(download.total);

			if seconds < download_time {
				let label = format!(
					"Downloading {}",
					screens::sheet::bytes((seconds * SPEED) as u64)
				);

				self.transfers
					.set(&download.key, &label, (seconds * SPEED) as u64, total);
			} else if seconds < download_time + 0.8 {
				self.transfers.set(
					&download.key,
					"Checking the download",
					download.total,
					total,
				);
			} else if seconds < download_time + 2.0 {
				self.transfers
					.set(&download.key, "Unpacking", download.total, total);
			} else {
				finished.push(download.key.clone());
			}
		}

		self.context.request_repaint();

		for key in finished {
			let Some(position) = self
				.demo_downloads
				.iter()
				.position(|download| download.key == key)
			else {
				continue;
			};

			let download = self.demo_downloads.remove(position);

			self.transfers.complete(&key);
			self.finish_demo_download(&download);
		}
	}

	/// Puts a finished demo download into the in-memory Library.
	fn finish_demo_download(&mut self, download: &DemoDownload) {
		let now = time::OffsetDateTime::now_utc().unix_timestamp();
		let Some(demo) = &mut self.demo else {
			return;
		};

		match download.updating {
			Some(id) => {
				let new_version = demo
					.statuses
					.get(&id)
					.and_then(|status| status.available.as_ref())
					.map(|update| update.version.clone());

				if let (Some(app), Some(version)) = (
					demo.installed.iter_mut().find(|app| app.id == id),
					new_version,
				) {
					app.version = version;
					app.installed_at = now;

					let fresh = demo::up_to_date(app);

					demo.statuses.insert(id, fresh);
				}
			}
			None => {
				let Some(row) = demo.catalog.iter().find(|row| row.slug == download.slug) else {
					return;
				};

				let app = InstalledApp {
					id: InstallationId::new(),
					name: download.name.clone(),
					repository: row.repository.clone(),
					version: "v1.0.0".into(),
					curated: true,
					catalog_slug: Some(download.slug.clone()),
					root: format!("/demo/apps/{}", download.slug),
					launchable: true,
					evidence: "local-observed".into(),
					installed_at: now,
				};

				demo.statuses.insert(app.id, demo::up_to_date(&app));
				demo.installed.push(app);
			}
		}
	}

	/// Plays an uninstall: a progress bar for a moment, then the app is gone
	/// from the in-memory list and the result is shown. Nothing real is
	/// touched.
	fn advance_demo_removals(&mut self) {
		const DURATION: f32 = 1.8;

		if self.demo_removals.is_empty() {
			return;
		}

		let mut finished = Vec::new();

		for (app, started) in &self.demo_removals {
			let done = started.elapsed().as_secs_f32() / DURATION;

			if done >= 1.0 {
				finished.push(app.clone());
			} else {
				self.progress.insert(
					app.id,
					screens::library::Progress {
						label: "Removing files".into(),
						fraction: Some(done),
						stoppable: false,
					},
				);
			}
		}

		self.context.request_repaint();

		for app in finished {
			self.demo_removals
				.retain(|(pending, _)| pending.id != app.id);
			self.progress.remove(&app.id);

			if let Some(demo) = &mut self.demo {
				demo.installed.retain(|kept| kept.id != app.id);
				demo.statuses.remove(&app.id);
				demo.policies.remove(&app.id);
			}

			self.installed.retain(|kept| kept.id != app.id);
			self.sheet = Some(Sheet::Removed {
				name: app.name,
				report: launcher_core::RemovalReport {
					removed_files: 214,
					kept_modified: vec!["settings.json".into()],
					kept_unknown: vec!["saved/workspace.db".into()],
					leftover_folder: Some(app.root),
				},
			});
		}
	}

	/// Actions in demo mode change only the invented data, or explain that
	/// nothing real happens.
	fn perform_demo(&mut self, action: Action) {
		let say = |text: &str| Sheet::Message {
			title: "Demo mode".into(),
			lines: vec![text.to_owned()],
		};

		match action {
			Action::Pin(id, pinned) => {
				if let Some(demo) = &mut self.demo {
					demo.policies.entry(id).or_default().pinned_release_id = pinned.then_some(1);
				}
			}
			Action::SetPolicy(id, policy) => {
				if let Some(demo) = &mut self.demo {
					demo.policies.insert(id, policy);
				}
			}
			Action::ReleaseNotes { app, tag, .. } => {
				self.sheet = Some(Sheet::Notes {
					text: demo::notes(&app, &tag),
					app,
					version: tag,
				});
			}
			Action::Install { slug } => self.start_demo_download(slug, None),
			Action::Update(id) => {
				let slug = self
					.installed
					.iter()
					.find(|app| app.id == id)
					.and_then(|app| app.catalog_slug.clone());

				if let Some(slug) = slug {
					self.start_demo_download(slug, Some(id));
				}
			}
			Action::Uninstall(app) => self.sheet = Some(Sheet::ConfirmUninstall(app)),
			Action::CheckUpdates | Action::Cancel => {}
			Action::OpenAppSettings(id) => self.app_settings = Some(id),
			Action::LookUp(_) => {
				self.add.lookup_failed();
				self.sheet = Some(say("Looking up repositories is turned off in demo mode."));
			}
			_ => {
				self.sheet = Some(say(
					"This would change your apps, so it does nothing in demo mode.",
				))
			}
		}
	}

	fn show_preparation(&mut self, preparation: Preparation) {
		let update_of = self.update_of.take();

		match preparation {
			// A reviewed app with nothing unusual about it installs at once.
			// The tray shows how it goes. Anything that needs a decision, an
			// unreviewed source or a file that changed under the same version,
			// still gets a dialog.
			Preparation::Ready(ready) if !ready.unreviewed && !ready.rebuilt => {
				self.commands.install(*ready, false, update_of);
			}
			Preparation::Ready(ready) => {
				self.transfers.drop_waiting();
				self.sheet = Some(Sheet::Review {
					ready,
					update_of,
					acknowledged: false,
				});
			}
			Preparation::Choose(candidates) => {
				self.transfers.drop_waiting();
				self.sheet = Some(Sheet::Choose(candidates));
			}
			Preparation::Unavailable(reason) => {
				self.transfers.drop_waiting();
				self.sheet = Some(Sheet::Message {
					title: "Not available".into(),
					lines: vec![reason],
				});
			}
		}
	}

	fn handle_event(&mut self, event: EngineEvent) {
		let EngineEvent::Operation(event) = event else {
			return;
		};

		let (label, received, total) = match &event.stage {
			Stage::Accepted => ("Starting".to_owned(), 0, None),
			Stage::Downloading { received, total } => (
				format!("Downloading {}", screens::sheet::bytes(*received)),
				*received,
				*total,
			),
			Stage::Verifying => ("Checking the download".into(), 0, None),
			Stage::Inspecting => ("Inspecting the package".into(), 0, None),
			Stage::Extracting => ("Unpacking".into(), 0, None),
			Stage::Applying => ("Finishing".into(), 0, None),
			Stage::Removing => ("Removing".into(), 0, None),
			Stage::Completed | Stage::Failed { .. } | Stage::RecoveryRequired { .. } => {
				self.progress.remove(&event.installation);
				// What was true before this operation no longer is.
				self.statuses.remove(&event.installation);
				self.commands.refresh_installed();
				self.commands.refresh_history();

				self.notify_about_finish(event.installation, &event.stage);

				match &event.stage {
					Stage::Completed => self.transfers.finish(event.installation),
					Stage::Failed { message } => {
						self.transfers.fail(event.installation, message.clone());
					}
					Stage::RecoveryRequired { message } => {
						self.transfers.fail(event.installation, message.clone());
						self.notice = Some(message.clone());
					}
					_ => {}
				}

				return;
			}
		};

		let stoppable = !matches!(event.stage, Stage::Applying | Stage::Removing);
		let fraction = total.map(|total: u64| (received as f64 / total.max(1) as f64) as f32);

		self.progress.insert(
			event.installation,
			screens::library::Progress {
				label: label.clone(),
				fraction,
				stoppable,
			},
		);

		// Removals have their own card progress; the tray is for downloads.
		if !matches!(event.stage, Stage::Removing) {
			let installed = &self.installed;

			self.transfers.progress(
				event.installation,
				|| {
					installed
						.iter()
						.find(|app| app.id == event.installation)
						.map_or_else(|| "An app".to_owned(), |app| app.name.clone())
				},
				label,
				received,
				total,
				stoppable,
			);
		}
	}

	fn perform(&mut self, action: Action) {
		// Looking at a listing changes nothing, so demo mode allows it too.
		if let Action::ViewListing(slug) = action {
			self.explore.select(slug);
			self.go(Screen::Explore);

			return;
		}

		if self.demo.is_some() {
			return self.perform_demo(action);
		}

		match action {
			Action::Install { slug } => {
				let name = self.explore.name_of(&slug).unwrap_or_else(|| slug.clone());

				self.update_of = None;
				self.transfers.begin(
					TransferKey::Slug(slug.clone()),
					name,
					Some(slug.clone()),
					"Checking the latest release…",
				);
				self.commands.prepare_install(slug);
			}
			Action::Update(id) => {
				let app = self.installed.iter().find(|app| app.id == id);
				let name = app.map_or_else(|| "An app".to_owned(), |app| app.name.clone());
				let slug = app.and_then(|app| app.catalog_slug.clone());

				self.update_of = Some(id);
				self.transfers.begin(
					TransferKey::Installation(id),
					name,
					slug,
					"Checking for a newer version…",
				);
				self.commands.prepare_update(id);
			}
			Action::Launch(id) => self.commands.launch(id),
			Action::SetPolicy(id, policy) => self.commands.set_app_policy(id, policy),
			Action::Pin(id, pinned) => self.commands.set_pinned(id, pinned),
			Action::CheckUpdates => self.commands.check_updates(),
			Action::OpenAppSettings(id) => self.app_settings = Some(id),
			Action::Cancel => self.commands.cancel_operation(),
			// Handled before this match.
			Action::ViewListing(_) => {}
			Action::UpdateAll => {
				self.sheet = Some(Sheet::Working("Checking what can be updated…".into()));
				self.commands.plan_all_updates();
			}
			Action::ReleaseNotes { slug, tag, app } => {
				self.sheet = Some(Sheet::Working("Loading release notes…".into()));
				self.commands.release_notes(slug, tag, app);
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

				if self.demo.is_some() {
					self.demo_removals.push((app, std::time::Instant::now()));
				} else {
					self.commands.uninstall(app);
				}
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

	/// Cmd/Ctrl+, opens Settings and Cmd/Ctrl+B folds the sidebar. The
	/// native menu sends the same commands on macOS.
	fn handle_shortcuts(&mut self, context: &Context) {
		let (settings, sidebar) = context.input_mut(|input| {
			(
				input.consume_key(Modifiers::COMMAND, Key::Comma),
				input.consume_key(Modifiers::COMMAND, Key::B),
			)
		});

		if sidebar {
			self.sidebar_collapsed = !self.sidebar_collapsed;
		}

		if settings {
			self.go(Screen::Settings);
		}

		for command in self.menu.poll() {
			match command {
				MenuCommand::Settings => self.go(Screen::Settings),
				MenuCommand::About => {
					self.show_about = true;
					self.go(Screen::Settings);
				}
			}
		}
	}

	/// Starts on the Library when there is something in it, and on Explore
	/// when there is not. Only the first answer counts, and never after the
	/// person has chosen a screen.
	fn choose_start_page(&mut self) {
		// Wait until both the saved preferences and the list of apps are in,
		// and never move someone who has already picked a page.
		if self.navigated || !self.prefs_loaded || !self.installed_loaded {
			return;
		}

		self.navigated = true;

		self.screen = start_screen(
			self.prefs.start_page,
			&self.prefs.layout.last_page,
			!self.installed.is_empty(),
		);

		if self.screen == Screen::Settings {
			self.refresh_settings_data();
		}
	}

	/// Takes the saved preferences and puts them to use.
	fn receive_preferences(&mut self, prefs: Preferences) {
		self.prefs = prefs;
		self.prefs_loaded = true;

		self.sidebar_collapsed = self.prefs.layout.sidebar_collapsed;
		self.view_mode = if self.prefs.layout.grid_view {
			ViewMode::Grid
		} else {
			ViewMode::List
		};
		self.explore.set_sort_key(&self.prefs.layout.explore_sort);
		self.library.set_sort_key(&self.prefs.layout.library_sort);
		self.apply_scale();
		self.choose_start_page();
	}

	fn apply_scale(&self) {
		self.context
			.set_zoom_factor(f32::from(self.prefs.ui_scale) / 100.0);
	}

	/// A change made in Settings: use it now and save it.
	fn change_preferences(&mut self, prefs: Preferences) {
		self.prefs = prefs;
		self.apply_scale();
		self.commands.save_preferences(self.prefs.clone());
	}

	/// Notices when the layout changes and saves it once it settles, so
	/// dragging the window edge does not write on every frame.
	fn remember_layout(&mut self, context: &Context) {
		const SETTLE: std::time::Duration = std::time::Duration::from_millis(800);

		// A demo is not the person's real use, so it leaves their saved
		// layout alone.
		if !self.prefs_loaded || self.demo.is_some() {
			return;
		}

		// The size is kept in unscaled points, so it means the same thing next
		// time whatever the interface scale is then.
		let window = context
			.input(|input| input.viewport().inner_rect)
			.map(|rect| {
				(
					rect.width() * context.zoom_factor(),
					rect.height() * context.zoom_factor(),
				)
			});

		let layout = launcher_model::Layout {
			sidebar_collapsed: self.sidebar_collapsed,
			grid_view: self.view_mode == ViewMode::Grid,
			explore_sort: self.explore.sort_key().to_owned(),
			library_sort: self.library.sort_key().to_owned(),
			last_page: self.screen.key().to_owned(),
			window: window.or(self.prefs.layout.window),
		};

		if !same_window(&layout, &self.prefs.layout) {
			self.prefs.layout = layout;
			self.layout_changed_at = Some(std::time::Instant::now());
		}

		if let Some(changed) = self.layout_changed_at {
			if changed.elapsed() >= SETTLE {
				self.layout_changed_at = None;
				self.commands.save_preferences(self.prefs.clone());
			} else {
				context.request_repaint_after(SETTLE);
			}
		}
	}

	fn refresh_settings_data(&mut self) {
		self.commands.refresh_token();
		self.commands.refresh_storage();
	}

	/// Tells the person, if they asked and are looking elsewhere, that updates
	/// are waiting.
	fn notify_about_updates(&mut self) {
		let waiting = screens::updates::available(&self.statuses).len();
		let new = waiting > self.notified_updates;

		self.notified_updates = waiting;

		if new && self.prefs.notifications.updates_available && !self.window_focused() {
			let (title, body) = notifications::updates_available(waiting);

			notifications::send(&title, &body);
		}
	}

	/// Tells the person, if they asked and are looking elsewhere, that an
	/// install or update has finished or failed.
	fn notify_about_finish(&self, installation: InstallationId, stage: &Stage) {
		if !self.prefs.notifications.install_finished || self.window_focused() {
			return;
		}

		let name = self
			.transfers
			.name_of(installation)
			.unwrap_or_else(|| "An app".to_owned());

		match stage {
			Stage::Completed => {
				let (title, body) = notifications::install_finished(&name);

				notifications::send(&title, &body);
			}
			Stage::Failed { .. } | Stage::RecoveryRequired { .. } => {
				let (title, body) = notifications::install_failed(&name);

				notifications::send(&title, &body);
			}
			_ => {}
		}
	}

	/// Opens a folder in the system's file manager.
	fn show_folder(&self, path: &std::path::Path) {
		if let Ok(url) = launcher_net::Url::from_directory_path(path) {
			self.context.open_url(egui::OpenUrl::new_tab(url.as_str()));
		}
	}

	fn window_focused(&self) -> bool {
		self.context
			.input(|input| input.viewport().focused)
			.unwrap_or(true)
	}

	fn go(&mut self, screen: Screen) {
		self.navigated = true;

		if screen == Screen::Settings && self.screen != Screen::Settings {
			self.refresh_settings_data();
		}

		if screen == Screen::Updates {
			self.commands.refresh_history();
		}

		self.screen = screen;
	}

	/// The downloads tray: a floating card in the bottom-right corner, over
	/// whatever page is showing. It stays out of the way of everything else
	/// and offers a way to the Library.
	fn show_tray(&mut self, context: &Context) {
		if !self.transfers.prune() {
			return;
		}

		context.request_repaint_after(std::time::Duration::from_millis(250));

		let mut stop = false;
		let mut library = false;
		let on_library = self.screen == Screen::Library;

		egui::Area::new(egui::Id::unique("transfers"))
			.order(egui::Order::Foreground)
			.anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-24.0, -24.0))
			.show(context, |ui| {
				theme::floating_frame(context).show(ui, |ui| {
					ui.set_width(430.0);

					stop = self.transfers.show(ui);

					if !on_library {
						ui.add_space(12.0);

						ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
							library = ui
								.button(format!("{}  Go to Library", crate::ui::icons::BOOKS))
								.clicked();
						});
					}
				});
			});

		if stop {
			self.commands.cancel_operation();
		}

		if library {
			self.library.show_recent();
			self.go(Screen::Library);
		}
	}

	/// One app's update settings, in a dialog.
	fn show_app_settings(&mut self, context: &Context) {
		let Some(id) = self.app_settings else {
			return;
		};

		let Some(app) = self.installed.iter().find(|app| app.id == id).cloned() else {
			self.app_settings = None;

			return;
		};

		let policy = self.policies.get(&id).cloned().unwrap_or_default();
		let global_age = self.update_settings.minimum_age;
		let mut action = None;
		let mut close = false;

		let response = egui::Modal::new(egui::Id::unique("app-settings"))
			.frame(theme::dialog_frame(context))
			.show(context, |ui| {
				ui.set_width(420.0);

				let origin = ui.cursor().min;

				ui.heading(format!("{} update settings", app.name));
				ui.add_space(10.0);

				action = screens::app_settings::app_settings(ui, &app, &policy, global_age);

				ui.add_space(14.0);

				if dialog_ok(ui, "Done") || dialog_close(ui, origin, 420.0) {
					close = true;
				}
			});

		if close || response.should_close() {
			self.app_settings = None;
		}

		if let Some(action) = action {
			self.perform(action);
		}
	}

	/// The Add from GitHub dialog.
	fn show_add(&mut self, context: &Context) {
		if !self.adding {
			return;
		}

		let mut action = None;
		let mut close = false;

		let response = egui::Modal::new(egui::Id::unique("add-dialog"))
			.frame(theme::dialog_frame(context))
			.show(context, |ui| {
				ui.set_width(520.0);

				let origin = ui.cursor().min;

				action = self.add.show(ui);

				ui.add_space(14.0);

				// Both the corner × and a button, so it is easy to leave. This
				// one sits on the left, away from the lookup button.
				if ui.button("Close").clicked() || dialog_close(ui, origin, 520.0) {
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
		self.apply_demo();
		self.handle_shortcuts(&context);

		let waiting = screens::updates::available(&self.statuses).len();
		let mut go = None;

		let sidebar_fill = navigation::fill(&context);
		let sidebar_width = egui::lerp(
			theme::SIDEBAR_COLLAPSED_WIDTH..=theme::SIDEBAR_WIDTH,
			context.animate_bool_with_time(
				egui::Id::unique("sidebar-open"),
				!self.sidebar_collapsed,
				0.15,
			),
		);

		Panel::left("navigation")
			.frame(egui::Frame::new().fill(sidebar_fill))
			.resizable(false)
			.exact_size(sidebar_width)
			.show(ui, |ui| {
				let sidebar = Sidebar {
					logo: &self.logo,
					current: self.screen,
					updates: waiting,
					collapsed: self.sidebar_collapsed,
				};

				match screens::navigation(ui, sidebar) {
					NavAction::None => {}
					NavAction::Go(screen) => go = Some(screen),
					NavAction::AddFromGithub => self.adding = true,
					NavAction::ToggleCollapsed => self.sidebar_collapsed = !self.sidebar_collapsed,
				}
			});

		if let Some(screen) = go {
			self.go(screen);
		}

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
				Screen::Explore => {
					action = self.explore.show(
						ui,
						&self.commands,
						self.demo.as_ref().map(|demo| demo.catalog.as_slice()),
						&mut self.view_mode,
					);
				}
				Screen::Library => {
					action = screens::library(
						ui,
						&mut LibraryView {
							apps: &self.installed,
							busy: &self.progress,
							statuses: &self.statuses,
							policies: &self.policies,
							mode: self.update_settings.mode,
							state: &mut self.library,
							view: &mut self.view_mode,
							demo: self.demo.is_some(),
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
						logo: &self.logo,
						show_about: std::mem::take(&mut self.show_about),
						prefs: &self.prefs,
						inputs: &mut self.settings_inputs,
						token: &self.token_state,
						rate: self.rate_limit.as_ref(),
						storage: self.storage.as_ref(),
					};

					match screens::settings(ui, view) {
						SettingsAction::None => {}
						SettingsAction::Theme(theme) => self.change_theme(&context, theme),
						SettingsAction::CopyDiagnostics => self.commands.diagnostics(),
						SettingsAction::PickMode(mode) => self.pick_mode(mode),
						SettingsAction::SetMinimumAge(age) => self.commands.set_minimum_age(age),
						SettingsAction::RestoreDefaults => self.sheet = Some(Sheet::ConfirmRestore),
						SettingsAction::StopBackground => self.commands.stop_background_updates(),
						SettingsAction::Preferences(prefs) => self.change_preferences(*prefs),
						SettingsAction::SaveToken(token) => self.commands.save_token(token),
						SettingsAction::ClearToken => self.commands.clear_token(),
						SettingsAction::CheckRateLimit => self.commands.check_rate_limit(),
						SettingsAction::ShowFolder(path) => self.show_folder(&path),
						SettingsAction::ClearLeftovers => self.commands.clear_leftovers(),
						SettingsAction::ClearCache => self.commands.clear_cache(),
						SettingsAction::TestNotification => {
							notifications::send(product::NAME, "This is a test notification.");
						}
					}
				}
			}
		});

		if let Some(action) = action {
			self.perform(action);
		}

		self.remember_layout(&context);
		self.show_tray(&context);
		self.show_app_settings(&context);
		self.show_add(&context);
		self.show_sheet(&context);
	}
}

/// Whether two layouts are the same, treating window sizes less than a point
/// apart as equal.
fn same_window(a: &launcher_model::Layout, b: &launcher_model::Layout) -> bool {
	let near = |left: f32, right: f32| (left - right).abs() < 1.0;

	let windows_match = match (a.window, b.window) {
		(Some((aw, ah)), Some((bw, bh))) => near(aw, bw) && near(ah, bh),
		(None, None) => true,
		_ => false,
	};

	windows_match
		&& a.sidebar_collapsed == b.sidebar_collapsed
		&& a.grid_view == b.grid_view
		&& a.explore_sort == b.explore_sort
		&& a.library_sort == b.library_sort
		&& a.last_page == b.last_page
}

/// Which page to open on. `last_page` is the name saved when the launcher was
/// last used.
fn start_screen(start: StartPage, last_page: &str, any_installed: bool) -> Screen {
	match start {
		StartPage::Library => Screen::Library,
		StartPage::Explore => Screen::Explore,
		StartPage::LastUsed => Screen::from_key(last_page).unwrap_or(Screen::Explore),
		StartPage::Auto if any_installed => Screen::Library,
		StartPage::Auto => Screen::Explore,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn automatic_opens_the_library_only_when_something_is_in_it() {
		assert_eq!(start_screen(StartPage::Auto, "", true), Screen::Library);
		assert_eq!(start_screen(StartPage::Auto, "", false), Screen::Explore);
	}

	#[test]
	fn a_fixed_choice_is_honoured_either_way() {
		assert_eq!(start_screen(StartPage::Explore, "", true), Screen::Explore);
		assert_eq!(start_screen(StartPage::Library, "", false), Screen::Library);
	}

	#[test]
	fn where_i_left_off_uses_the_saved_page_and_survives_nonsense() {
		assert_eq!(
			start_screen(StartPage::LastUsed, "updates", false),
			Screen::Updates
		);
		assert_eq!(
			start_screen(StartPage::LastUsed, "settings", true),
			Screen::Settings
		);
		assert_eq!(
			start_screen(StartPage::LastUsed, "no-such-page", true),
			Screen::Explore
		);
		assert_eq!(start_screen(StartPage::LastUsed, "", true), Screen::Explore);
	}

	#[test]
	fn layout_comparison_ignores_sub_point_window_jitter() {
		let base = launcher_model::Layout {
			window: Some((1280.0, 820.0)),
			..launcher_model::Layout::default()
		};
		let jitter = launcher_model::Layout {
			window: Some((1280.4, 819.7)),
			..base.clone()
		};
		let resized = launcher_model::Layout {
			window: Some((1300.0, 820.0)),
			..base.clone()
		};
		let folded = launcher_model::Layout {
			sidebar_collapsed: true,
			..base.clone()
		};

		assert!(same_window(&base, &jitter));
		assert!(!same_window(&base, &resized));
		assert!(!same_window(&base, &folded));
	}
}
