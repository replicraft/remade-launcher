use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

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

	/// Print machine-readable JSON where a command lists things.
	#[arg(long, global = true)]
	pub(crate) json: bool,

	#[command(subcommand)]
	pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
	/// Search the verified catalog.
	Search(SearchArgs),
	/// Show everything the catalog says about one app.
	Info(InfoArgs),
	/// List what is installed.
	List,
	/// Install an app from the catalog, or from a GitHub repository you choose.
	Install(InstallArgs),
	/// Update installed catalog apps.
	Update(UpdateArgs),
	/// Remove an installed app. Files you made or edited are kept.
	Uninstall(UninstallArgs),
	/// Start an installed app.
	Run(RunArgs),
	/// Check or refresh the catalog.
	Catalog {
		#[command(subcommand)]
		action: CatalogAction,
	},
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
pub(crate) struct InfoArgs {
	/// The app's catalog name, such as `sample-editor`.
	pub(crate) slug: String,
}

#[derive(Debug, Args)]
pub(crate) struct InstallArgs {
	/// A catalog app, or with `--from`, ignored.
	#[arg(required_unless_present = "from", conflicts_with = "from")]
	pub(crate) slug: Option<String>,

	/// Install from a public GitHub repository or release link instead.
	/// Remade has not reviewed apps added this way.
	#[arg(long, value_name = "URL")]
	pub(crate) from: Option<String>,

	/// With `--from`: the exact file to install, when the release has several.
	#[arg(long, value_name = "FILENAME")]
	pub(crate) file: Option<String>,

	/// Do not ask for confirmation.
	#[arg(short, long)]
	pub(crate) yes: bool,

	/// With `--from`: confirm you trust this unreviewed source. Required in
	/// addition to `--yes` when not running interactively.
	#[arg(long)]
	pub(crate) trust_unreviewed_source: bool,
}

#[derive(Debug, Args)]
pub(crate) struct UpdateArgs {
	/// One app to update. Without it, every installed catalog app is checked.
	pub(crate) app: Option<String>,

	/// Only report what could be updated.
	#[arg(long)]
	pub(crate) check: bool,

	/// Do not ask for confirmation.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct UninstallArgs {
	/// The app's name, catalog name, or its short ID from `remade list`.
	pub(crate) app: String,

	/// Do not ask for confirmation.
	#[arg(short, long)]
	pub(crate) yes: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RunArgs {
	/// The app's name, catalog name, or its short ID from `remade list`.
	pub(crate) app: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CatalogAction {
	/// Download and verify the latest catalog.
	Refresh,
	/// Show which catalog is in use and how fresh it is.
	Status,
}
