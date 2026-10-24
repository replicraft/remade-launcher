//! Typed commands from the interface to the engine.
//!
//! Each command runs on the async runtime, so the render loop never waits on
//! storage, the network, hashing, or extraction. Results come back as
//! [`Message`]s, and the interface repaints when one arrives.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use launcher_background::control::{self, BackgroundStatus};
use launcher_core::{
	CatalogState, CustomPreparation, Engine, EngineEvent, ExploreQuery, ExploreRow, InstalledApp,
	LaunchOutcome, ModeConsent, OperationSummary, OperatorUpdate, Preparation, Ready,
	RemovalReport, UpdateSettings, UpdateStatus, custom_plan,
};
use launcher_model::{InstallationId, MinimumAge, ThemePreference, UpdateMode};
use launcher_resolver::Consent;
use launcher_storage::AppPolicy;
use tokio::runtime::Handle;

/// What a finished command reports back to the interface.
#[derive(Debug)]
pub(crate) enum Message {
	/// A user-safe description of something that failed.
	Failed(String),
	/// Results for the search request numbered `request`.
	Explore {
		request: u64,
		rows: Vec<ExploreRow>,
	},
	Installed(Vec<InstalledApp>),
	Prepared(Preparation),
	/// The plan for the file chosen from a user-added source.
	CustomChosen(Box<Ready>),
	CustomLookedUp(CustomPreparation),
	Uninstalled {
		name: String,
		report: RemovalReport,
	},
	Engine(EngineEvent),
	Diagnostics(String),
	/// The saved update settings and the background updater's real state.
	UpdateInfo(Box<UpdateSettings>, Box<BackgroundStatus>),
	Statuses(Vec<UpdateStatus>),
	Policies(HashMap<InstallationId, AppPolicy>),
	/// A sentence about what an automatic update pass did.
	Notice(String),
	History(Vec<OperationSummary>),
	/// Release notes for one app's available version.
	Notes {
		app: String,
		version: String,
		text: String,
	},
	/// Updates that can be applied together, awaiting the person's go-ahead.
	UpdatesPlanned(Vec<OperatorUpdate>),
	/// Preferences are back to their defaults.
	Restored(Box<UpdateSettings>, Box<BackgroundStatus>),
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

		commands.forward_engine_events();
		commands.repaint_on_catalog_changes();
		commands.refresh_catalog();
		commands.refresh_installed();
		commands.refresh_update_info();
		commands.launcher_started();

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
		self.spawn(move |engine| async move {
			Some(Message::Explore {
				request,
				rows: engine.explore(query).await,
			})
		});
	}

	pub(crate) fn refresh_installed(&self) {
		self.spawn_result(|engine| async move { engine.installed().await.map(Message::Installed) });
	}

	pub(crate) fn prepare_install(&self, slug: String) {
		self.spawn_result(move |engine| async move {
			engine.prepare_install(&slug).await.map(Message::Prepared)
		});
	}

	pub(crate) fn prepare_update(&self, installation: InstallationId) {
		self.spawn_result(move |engine| async move {
			engine
				.prepare_update(installation)
				.await
				.map(Message::Prepared)
		});
	}

	pub(crate) fn look_up_custom(&self, url: String) {
		self.spawn_result(move |engine| async move {
			engine
				.prepare_custom(&url)
				.await
				.map(Message::CustomLookedUp)
		});
	}

	pub(crate) fn choose_custom_file(&self, preparation: CustomPreparation, asset_id: u64) {
		self.spawn_result(move |_| async move {
			let now = time::OffsetDateTime::now_utc().unix_timestamp();

			custom_plan(&preparation, asset_id, now)
				.map(|ready| Message::CustomChosen(Box::new(ready)))
		});
	}

	/// Approves and runs `ready`. `update` names the app being replaced.
	pub(crate) fn install(
		&self,
		ready: Ready,
		acknowledged_unreviewed: bool,
		update: Option<InstallationId>,
	) {
		self.spawn_result(move |engine| async move {
			let consent = Consent {
				source_shown: ready.source_display.clone(),
				acknowledged_unreviewed,
			};

			match update {
				Some(installation) => engine.update(installation, &ready, &consent).await,
				None => engine.install(&ready, &consent).await,
			}?;

			engine.installed().await.map(Message::Installed)
		});
	}

	pub(crate) fn uninstall(&self, app: InstalledApp) {
		self.spawn_result(move |engine| async move {
			let now = time::OffsetDateTime::now_utc().unix_timestamp();
			let report = engine.lifecycle().uninstall(app.id, now).await?;

			Ok(Message::Uninstalled {
				name: app.name,
				report,
			})
		});
	}

	pub(crate) fn launch(&self, installation: InstallationId) {
		self.spawn_result(move |engine| async move {
			engine.lifecycle().launch(installation).await?;

			engine.installed().await.map(Message::Installed)
		});
	}

	pub(crate) fn diagnostics(&self) {
		self.spawn_result(
			|engine| async move { engine.diagnostics().await.map(Message::Diagnostics) },
		);
	}

	/// Reloads the update settings, per-app policies, and the background
	/// updater's true state.
	pub(crate) fn refresh_update_info(&self) {
		self.spawn(|engine| async move {
			let settings = engine.update_settings().await;
			let background = control::status(&engine).await;

			Some(Message::UpdateInfo(
				Box::new(settings),
				Box::new(background),
			))
		});

		self.spawn(|engine| async move {
			let mut policies = HashMap::new();

			for app in engine.installed().await.ok()? {
				policies.insert(app.id, engine.app_update_policy(app.id).await.ok()?);
			}

			Some(Message::Policies(policies))
		});
	}

	/// What a genuine launcher start does. Called once, when the window opens.
	pub(crate) fn launcher_started(&self) {
		self.spawn_result(|engine| async move {
			Ok(match engine.launcher_started().await? {
				LaunchOutcome::AlreadyHandled => Message::Notice(String::new()),
				LaunchOutcome::Checked(statuses) => Message::Statuses(statuses),
				LaunchOutcome::Cycle(report) => {
					let text = if report.already_running {
						"Another update pass was already running.".to_owned()
					} else if report.updated() > 0 || report.failed() > 0 {
						format!(
							"Automatic updates: {} updated, {} failed.",
							report.updated(),
							report.failed()
						)
					} else {
						String::new()
					};

					Message::Notice(text)
				}
			})
		});
	}

	/// Asks the running install or update to stop at the next safe point.
	pub(crate) fn cancel_operation(&self) {
		self.engine.lifecycle().request_cancel();
	}

	pub(crate) fn refresh_history(&self) {
		self.spawn_result(|engine| async move { engine.history(30).await.map(Message::History) });
	}

	pub(crate) fn release_notes(&self, slug: String, tag: String, app: String) {
		self.spawn_result(move |engine| async move {
			let notes = engine.changelog(&slug, Some(&tag)).await?;

			Ok(Message::Notes {
				app,
				version: notes.version,
				text: notes.text,
			})
		});
	}

	/// Looks up what updating everything would do. Applies nothing.
	pub(crate) fn plan_all_updates(&self) {
		self.spawn_result(|engine| async move {
			engine
				.plan_operator_updates(None)
				.await
				.map(Message::UpdatesPlanned)
		});
	}

	/// Applies the updates the person just reviewed, one after another.
	pub(crate) fn apply_updates(&self, updates: Vec<OperatorUpdate>) {
		self.spawn_result(|engine| async move {
			for update in updates {
				let Some(ready) = update.ready else {
					continue;
				};

				let consent = Consent {
					source_shown: ready.source_display.clone(),
					acknowledged_unreviewed: false,
				};

				if let Err(error) = engine
					.update(update.status.installation, &ready, &consent)
					.await
				{
					tracing::warn!(%error, "an update in a batch failed");
				}
			}

			engine.installed().await.map(Message::Installed)
		});
	}

	/// Resets preferences. A background updater registered with the system is
	/// removed first, so no login item is left behind.
	pub(crate) fn restore_defaults(&self) {
		self.spawn_result(|engine| async move {
			let consent = ModeConsent {
				actor: "desktop".into(),
				acknowledged_risks: false,
			};

			let _ = control::apply_mode(&engine, UpdateMode::Manual, &consent, None).await;
			engine.restore_defaults().await?;

			let settings = engine.update_settings().await;
			let background = control::status(&engine).await;

			Ok(Message::Restored(Box::new(settings), Box::new(background)))
		});
	}

	/// A read-only look for updates. Installs nothing.
	pub(crate) fn check_updates(&self) {
		self.spawn_result(|engine| async move {
			engine.check_updates(None).await.map(Message::Statuses)
		});
	}

	pub(crate) fn set_update_mode(&self, mode: UpdateMode, acknowledged: bool) {
		self.spawn_result(move |engine| async move {
			let consent = ModeConsent {
				actor: "desktop".into(),
				acknowledged_risks: acknowledged,
			};

			control::apply_mode(&engine, mode, &consent, None)
				.await
				.map_err(|error| match error {
					control::ControlError::Engine(error) => error,
					other => launcher_model::LauncherError::new(
						launcher_model::ErrorCode::ConsentRequired,
						other.to_string(),
					),
				})?;

			let settings = engine.update_settings().await;
			let background = control::status(&engine).await;

			Ok(Message::UpdateInfo(
				Box::new(settings),
				Box::new(background),
			))
		});
	}

	pub(crate) fn set_minimum_age(&self, age: MinimumAge) {
		self.spawn_result(move |engine| async move {
			engine.set_minimum_age(age).await?;

			let settings = engine.update_settings().await;
			let background = control::status(&engine).await;

			Ok(Message::UpdateInfo(
				Box::new(settings),
				Box::new(background),
			))
		});
	}

	pub(crate) fn set_app_policy(&self, installation: InstallationId, policy: AppPolicy) {
		self.spawn_result(move |engine| async move {
			engine.set_app_update_policy(installation, policy).await?;

			let mut policies = HashMap::new();

			for app in engine.installed().await? {
				policies.insert(app.id, engine.app_update_policy(app.id).await?);
			}

			Ok(Message::Policies(policies))
		});
	}

	pub(crate) fn set_pinned(&self, installation: InstallationId, pinned: bool) {
		self.spawn_result(move |engine| async move {
			if pinned {
				engine.pin(installation).await?
			} else {
				engine.unpin(installation).await?
			}

			let mut policies = HashMap::new();

			for app in engine.installed().await? {
				policies.insert(app.id, engine.app_update_policy(app.id).await?);
			}

			Ok(Message::Policies(policies))
		});
	}

	pub(crate) fn stop_background_updates(&self) {
		self.spawn_result(|engine| async move {
			let _ = control::stop(&engine).await;

			let settings = engine.update_settings().await;
			let background = control::status(&engine).await;

			Ok(Message::UpdateInfo(
				Box::new(settings),
				Box::new(background),
			))
		});
	}

	pub(crate) fn set_theme(&self, theme: ThemePreference) {
		self.spawn_result(move |engine| async move {
			engine.set_theme(theme).await?;

			engine.installed().await.map(Message::Installed)
		});
	}

	/// Runs `work` on the runtime and reports its message, if any.
	fn spawn<F, Fut>(&self, work: F)
	where
		F: FnOnce(Arc<Engine>) -> Fut + Send + 'static,
		Fut: std::future::Future<Output = Option<Message>> + Send + 'static,
	{
		let engine = Arc::clone(&self.engine);
		let (messages, context) = (self.messages.clone(), self.context.clone());

		self.runtime.spawn(async move {
			if let Some(message) = work(engine).await {
				let _ = messages.send(message);
				context.request_repaint();
			}
		});
	}

	/// Like [`Self::spawn`], but a failure becomes a user-safe message.
	fn spawn_result<F, Fut>(&self, work: F)
	where
		F: FnOnce(Arc<Engine>) -> Fut + Send + 'static,
		Fut: std::future::Future<Output = Result<Message, launcher_model::LauncherError>>
			+ Send
			+ 'static,
	{
		self.spawn(move |engine| async move {
			Some(match work(engine).await {
				Ok(message) => message,
				Err(error) => {
					tracing::warn!(%error, "a command failed");

					Message::Failed(error.user_message().to_owned())
				}
			})
		});
	}

	/// Passes engine events, such as install progress, to the interface.
	fn forward_engine_events(&self) {
		let mut events = self.engine.subscribe();
		let (messages, context) = (self.messages.clone(), self.context.clone());

		self.runtime.spawn(async move {
			loop {
				match events.recv().await {
					Ok(event) => {
						let _ = messages.send(Message::Engine(event));
						context.request_repaint();
					}
					// A slow interface missed some progress. Terminal events
					// are followed by an installed-list refresh, so state
					// still converges.
					Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
					Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
				}
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
