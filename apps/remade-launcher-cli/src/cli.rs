use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

/// Search the Remade catalog, and install, update, and remove apps.
#[derive(Debug, Parser)]
#[command(name = "remade", version, about, propagate_version = true)]
pub(crate) struct Cli {
	/// Use this folder for the launcher's data instead of the default.
	#[arg(long, global = true, env = "REMADE_DATA_DIR", value_name = "DIR")]
	pub(crate) data_dir: Option<PathBuf>,

	/// Never contact the network. Use the saved catalog only.
	#[arg(long, global = true)]
	pub(crate) offline: bool,

	/// Print one JSON object. Shorthand for `--output json`.
	#[arg(long, global = true, conflicts_with = "output")]
	pub(crate) json: bool,

	/// How to print results.
	#[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
	pub(crate) output: OutputFormat,

	/// Print only what was asked for.
	#[arg(short, long, global = true)]
	pub(crate) quiet: bool,

	/// Do not use color.
	#[arg(long, global = true, env = "NO_COLOR")]
	pub(crate) no_color: bool,

	/// Do not show progress bars.
	#[arg(long, global = true)]
	pub(crate) no_progress: bool,

	/// Never ask a question. A command that needs an answer fails instead.
	#[arg(long, global = true)]
	pub(crate) no_input: bool,

	/// Log what the launcher is doing to stderr.
	#[arg(short, long, global = true)]
	pub(crate) verbose: bool,

	/// Give up on the whole command after this many seconds.
	#[arg(long, global = true, value_name = "SECONDS")]
	pub(crate) timeout: Option<u64>,

	#[command(subcommand)]
	pub(crate) command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum OutputFormat {
	Text,
	Json,
	/// One JSON object per line, for streaming progress.
	Ndjson,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
	/// Search the verified catalog.
	Search(SearchArgs),
	/// Browse the catalog. Like `search` with no words.
	Explore(ExploreArgs),
	/// Show everything the catalog says about one app.
	Info(SlugArgs),
	/// Show which packages run on this device, and why.
	Platforms(OptionalSlugArgs),
	/// List an app's releases and which of them the launcher would use.
	Releases(ReleasesArgs),
	/// Show release notes as plain text.
	Changelog(ChangelogArgs),
	/// Show, refresh, or verify the catalog.
	Catalog {
		#[command(subcommand)]
		action: CatalogAction,
	},
	/// Explain which keys and checks the launcher relies on.
	Trust {
		#[command(subcommand)]
		action: TrustAction,
	},
	/// List installed apps.
	List,
	/// Look for updates and list what has one.
	Outdated(OutdatedArgs),
	/// Look for updates to installed apps. Installs nothing.
	Check(CheckArgs),
	/// Show recent installs, updates, and removals.
	History(HistoryArgs),
	/// Inspect or recover operations.
	Operations {
		#[command(subcommand)]
		action: OperationsAction,
	},
	/// Work out exactly what an install, update, or removal would do.
	Plan {
		#[command(subcommand)]
		action: PlanAction,
	},
	/// Run a saved plan after you approve it.
	Apply(ApplyArgs),
	/// Install an app from the catalog, or from a GitHub repository link.
	Install(InstallArgs),
	/// Update an app, or every app with `--all`.
	Update(UpdateArgs),
	/// Remove an installed app. Files you made or edited are kept.
	Uninstall(UninstallArgs),
	/// Start an installed app.
	#[command(alias = "run")]
	Launch(AppArgs),
	/// Compare an app's files with what was installed.
	Verify(AppArgs),
	/// Check the launcher's own state. Changes nothing.
	Doctor(OptionalAppArgs),
	/// Stop managing an app without touching its files.
	Forget(ForgetArgs),
	/// Look for damage, and put the app back if there is some.
	Repair(RepairArgs),
	/// Watch an app that is already on this computer. It is never changed.
	Adopt(AdoptArgs),
	/// Hold an app at its current version.
	Pin(AppArgs),
	/// Let a pinned app update again.
	Unpin(AppArgs),
	/// List pinned apps.
	Pins,
	/// Print the folder an app is installed in.
	Path(AppArgs),
	/// Control automatic updates.
	Updates {
		#[command(subcommand)]
		action: UpdatesAction,
	},
	/// Control the background updater.
	Daemon {
		#[command(subcommand)]
		action: DaemonAction,
	},
	/// Read or change settings.
	Config {
		#[command(subcommand)]
		action: ConfigAction,
	},
	/// Update the launcher itself.
	SelfUpdate {
		#[command(subcommand)]
		action: SelfUpdateAction,
	},
	/// Manage the launcher's scratch space.
	Cache {
		#[command(subcommand)]
		action: CacheAction,
	},
	/// Print shell completions.
	Completions { shell: clap_complete::Shell },
	/// Print version and build details.
	About,
	/// Print a support report with no private details in it.
	Diagnostics,
}

#[derive(Debug, Args)]
pub(crate) struct SearchArgs {
	/// Words to look for in names, summaries, categories, and repositories.
	pub(crate) query: Vec<String>,

	/// Include apps with no package for this device.
	#[arg(long)]
	pub(crate) all: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ExploreArgs {
	/// Include apps with no package for this device.
	#[arg(long)]
	pub(crate) all: bool,
}

#[derive(Debug, Args)]
pub(crate) struct SlugArgs {
	/// The app's catalog name, such as `sample-editor`.
	pub(crate) slug: String,
}

#[derive(Debug, Args)]
pub(crate) struct OptionalSlugArgs {
	pub(crate) slug: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ReleasesArgs {
	pub(crate) slug: String,

	/// Show at most this many.
	#[arg(short = 'n', long, default_value_t = 20)]
	pub(crate) limit: usize,
}

#[derive(Debug, Args)]
pub(crate) struct ChangelogArgs {
	pub(crate) slug: String,

	/// A release tag. Defaults to the newest stable release.
	#[arg(long, value_name = "TAG")]
	pub(crate) release: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct AppArgs {
	/// The app's name, catalog name, or its short ID from `remade list`.
	pub(crate) app: String,
}

#[derive(Debug, Args)]
pub(crate) struct OptionalAppArgs {
	pub(crate) app: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct AdoptArgs {
	/// The app's own folder, such as /opt/quill or a folder holding its .app.
	pub(crate) path: String,

	/// A name to list it under. The folder's name is used if you leave it out.
	#[arg(long)]
	pub(crate) name: Option<String>,

	/// The catalog entry you say this is. A claim only: nothing is updated or
	/// removed because of it.
	#[arg(long = "as", value_name = "CATALOG-NAME")]
	pub(crate) catalog_name: Option<String>,

	/// The file that starts it, relative to the folder.
	#[arg(long, value_name = "FILE")]
	pub(crate) entry: Option<String>,

	/// Show what would be watched and stop.
	#[arg(long)]
	pub(crate) preview: bool,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RepairArgs {
	pub(crate) app: String,

	/// Say what is wrong and what a repair would do, then stop.
	#[arg(long)]
	pub(crate) preview: bool,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ForgetArgs {
	pub(crate) app: String,

	/// Do not ask for confirmation.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct OutdatedArgs {
	/// Also list apps that are up to date.
	#[arg(long)]
	pub(crate) all: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
	pub(crate) app: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct HistoryArgs {
	/// Show only this app's operations.
	pub(crate) app: Option<String>,

	/// How many operations to show.
	#[arg(short = 'n', long, default_value_t = 20)]
	pub(crate) limit: u32,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CatalogAction {
	/// Show which catalog is in use and how fresh it is.
	Status,
	/// Download and verify the latest catalog.
	Refresh,
	/// Check the saved catalog's signature and structure again.
	Verify,
	/// List every app in the catalog.
	List,
	/// Show one catalog entry.
	Show { slug: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum TrustAction {
	/// Which feed and keys this build trusts.
	Status,
	/// The trusted catalog key IDs.
	Keys,
	/// What an app's packages are checked against.
	Explain { slug: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum OperationsAction {
	/// Recent operations, newest first.
	List {
		#[arg(short = 'n', long, default_value_t = 20)]
		limit: u32,
	},
	/// One operation in full.
	Show { id: String },
	/// Ask an operation to stop at its next safe point.
	Cancel { id: String },
	/// Settle operations a crash left unfinished.
	Recover,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PlanAction {
	/// Plan installing an app from the catalog, or from a GitHub link.
	Install(PlanInstallArgs),
	/// Plan updating an installed app.
	Update { app: String },
	/// Plan removing an installed app.
	Uninstall { app: String },
	/// Plan repairing an installed app.
	Repair { app: String },
	/// Show a saved plan.
	Show { id: String },
}

#[derive(Debug, Args)]
pub(crate) struct PlanInstallArgs {
	/// A catalog name, or a github.com repository or release link.
	pub(crate) target: String,

	/// For a GitHub link: the exact file to install.
	#[arg(long, value_name = "FILENAME")]
	pub(crate) file: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ApplyArgs {
	/// A plan ID from `remade plan`.
	pub(crate) plan: String,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,

	/// For a source Remade has not reviewed: the exact `owner/repository` you
	/// accept. It must match the plan.
	#[arg(long, value_name = "OWNER/REPOSITORY")]
	pub(crate) accept_unreviewed_source: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct InstallArgs {
	/// A catalog name, or a github.com repository or release link.
	pub(crate) target: String,

	/// For a GitHub link: the exact file to install.
	#[arg(long, value_name = "FILENAME")]
	pub(crate) file: Option<String>,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,

	/// For a source Remade has not reviewed: the exact `owner/repository` you
	/// accept. It must match the plan.
	#[arg(long, value_name = "OWNER/REPOSITORY")]
	pub(crate) accept_unreviewed_source: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
	/// One app to update. Leave it out and use `--all` for every app.
	pub(crate) app: Option<String>,

	/// Update every installed catalog app that has an update.
	#[arg(long, conflicts_with = "app")]
	pub(crate) all: bool,

	/// Show what would be updated and stop.
	#[arg(long)]
	pub(crate) preview: bool,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct UninstallArgs {
	pub(crate) app: String,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum UpdatesAction {
	/// Show or change how apps are updated.
	Mode {
		#[command(subcommand)]
		action: Option<ModeAction>,
	},
	/// Show or change how old a release must be before an automatic update.
	Age {
		#[command(subcommand)]
		action: Option<AgeAction>,
	},
	/// Show or change one app's automatic-update policy.
	Policy(PolicyArgs),
	/// Look for updates and show whether each would install automatically.
	Check,
	/// Run an update pass now, on your own initiative.
	Run(RunArgs),
	/// Why recent automatic updates did or did not happen.
	Decisions(DecisionsArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum ModeAction {
	/// Choose how apps are updated.
	Set(ModeSetArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ModeSetArgs {
	/// `manual`, `on-launch`, or `daemon`.
	pub(crate) mode: String,

	/// For an automatic mode: confirm you understand a bad release could be
	/// installed without asking. Recorded.
	#[arg(long)]
	pub(crate) accept_risks: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AgeAction {
	/// Set the delay, such as `1h`, `24h`, `3d`, or `0` for none.
	Set {
		duration: String,

		/// Set it for one app instead of everywhere.
		#[arg(long, value_name = "APP")]
		app: Option<String>,
	},
	/// Go back to the global delay for one app.
	Reset {
		#[arg(long, value_name = "APP")]
		app: String,
	},
}

#[derive(Debug, Args)]
pub(crate) struct PolicyArgs {
	pub(crate) app: String,

	/// Stop automatic updates for this app.
	#[arg(long, conflicts_with = "resume_auto")]
	pub(crate) pause_auto: bool,

	/// Allow automatic updates for this app again.
	#[arg(long)]
	pub(crate) resume_auto: bool,

	/// Let this app update to pre-releases.
	#[arg(long, conflicts_with = "no_prerelease")]
	pub(crate) prerelease: bool,

	/// Go back to stable releases only.
	#[arg(long)]
	pub(crate) no_prerelease: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
	/// Show what would be updated and stop.
	#[arg(long)]
	pub(crate) dry_run: bool,

	/// Approve without asking.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct DecisionsArgs {
	pub(crate) app: String,

	#[arg(short = 'n', long, default_value_t = 20)]
	pub(crate) limit: u32,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DaemonAction {
	/// Whether the background updater is registered and running.
	Status,
	/// Register the updater to start when you sign in.
	Register(RegisterArgs),
	/// Remove the updater's login registration.
	Unregister,
	/// Start the registered updater now.
	Start,
	/// Stop the running updater. It may start again at next sign-in.
	Stop,
	/// Ask the running updater to look for updates now.
	CheckNow,
	/// Show the updater's log.
	Logs {
		#[arg(long, default_value_t = 100)]
		tail: usize,
	},
}

#[derive(Debug, Args)]
pub(crate) struct RegisterArgs {
	/// Confirm you understand what an always-on updater can do. Recorded.
	#[arg(long)]
	pub(crate) accept_risks: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ConfigAction {
	List,
	Get { key: String },
	Set { key: String, value: String },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SelfUpdateAction {
	Check,
	Apply,
	Status,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CacheAction {
	/// What is in the launcher's scratch space.
	Status,
	/// Remove scratch folders no unfinished operation needs.
	Prune,
}
