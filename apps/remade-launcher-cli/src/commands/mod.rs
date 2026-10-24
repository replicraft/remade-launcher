mod browse;
mod catalog;
mod daemon;
mod installed;
mod lifecycle;
mod misc;
mod shared;
mod updates;

use std::sync::Arc;

use launcher_core::Engine;

use crate::cli::{Cli, Command, OperationsAction};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;

/// A stable name for each command, used in JSON results.
pub(crate) fn name(command: &Command) -> &'static str {
	match command {
		Command::Search(_) => "search",
		Command::Explore(_) => "explore",
		Command::Info(_) => "info",
		Command::Platforms(_) => "platforms",
		Command::Releases(_) => "releases",
		Command::Changelog(_) => "changelog",
		Command::Catalog { .. } => "catalog",
		Command::Trust { .. } => "trust",
		Command::List => "list",
		Command::Outdated(_) => "outdated",
		Command::Check(_) => "check",
		Command::History(_) => "history",
		Command::Operations { action } => match action {
			OperationsAction::List { .. } => "operations list",
			OperationsAction::Show { .. } => "operations show",
			OperationsAction::Cancel { .. } => "operations cancel",
			OperationsAction::Recover => "operations recover",
		},
		Command::Plan { .. } => "plan",
		Command::Apply(_) => "apply",
		Command::Install(_) => "install",
		Command::Update(_) => "update",
		Command::Uninstall(_) => "uninstall",
		Command::Launch(_) => "launch",
		Command::Verify(_) => "verify",
		Command::Doctor(_) => "doctor",
		Command::Forget(_) => "forget",
		Command::Repair(_) => "repair",
		Command::Pin(_) => "pin",
		Command::Unpin(_) => "unpin",
		Command::Pins => "pins",
		Command::Path(_) => "path",
		Command::Updates { .. } => "updates",
		Command::Daemon { .. } => "daemon",
		Command::Config { .. } => "config",
		Command::SelfUpdate { .. } => "self-update",
		Command::Cache { .. } => "cache",
		Command::Completions { .. } => "completions",
		Command::About => "about",
		Command::Diagnostics => "diagnostics",
	}
}

pub(crate) async fn dispatch(
	ctx: &Arc<Ctx>,
	engine: &Arc<Engine>,
	cli: &Cli,
) -> Result<ExitStatus, CliError> {
	match &cli.command {
		Command::Search(args) => browse::search(ctx, engine, args).await,
		Command::Explore(args) => browse::explore(ctx, engine, args).await,
		Command::Info(args) => browse::info(ctx, engine, args).await,
		Command::Platforms(args) => browse::platforms(ctx, engine, args).await,
		Command::Releases(args) => browse::releases(ctx, engine, args).await,
		Command::Changelog(args) => browse::changelog(ctx, engine, args).await,
		Command::Catalog { action } => catalog::run(ctx, engine, action).await,
		Command::Trust { action } => browse::trust(ctx, engine, action).await,
		Command::List => installed::list(ctx, engine).await,
		Command::Outdated(args) => installed::outdated(ctx, engine, args).await,
		Command::Check(args) => installed::check(ctx, engine, args).await,
		Command::History(args) => installed::history(ctx, engine, args).await,
		Command::Operations { action } => installed::operations(ctx, engine, action).await,
		Command::Plan { action } => lifecycle::plan(ctx, engine, action).await,
		Command::Apply(args) => lifecycle::apply(ctx, engine, args).await,
		Command::Install(args) => lifecycle::install(ctx, engine, args).await,
		Command::Update(args) => lifecycle::update(ctx, engine, args).await,
		Command::Uninstall(args) => lifecycle::uninstall(ctx, engine, args).await,
		Command::Launch(args) => lifecycle::launch(ctx, engine, args).await,
		Command::Verify(args) => installed::verify(ctx, engine, args).await,
		Command::Doctor(args) => installed::doctor(ctx, engine, args).await,
		Command::Forget(args) => lifecycle::forget(ctx, engine, args).await,
		Command::Repair(args) => lifecycle::repair(ctx, engine, args).await,
		Command::Pin(args) => lifecycle::pin(ctx, engine, args, true).await,
		Command::Unpin(args) => lifecycle::pin(ctx, engine, args, false).await,
		Command::Pins => installed::pins(ctx, engine).await,
		Command::Path(args) => installed::path(ctx, engine, args).await,
		Command::Updates { action } => updates::run(ctx, engine, action).await,
		Command::Daemon { action } => daemon::run(ctx, engine, action).await,
		Command::Config { action } => misc::config(ctx, engine, action).await,
		Command::SelfUpdate { action } => Ok(misc::self_update(ctx, action)),
		Command::Cache { action } => misc::cache(ctx, engine, action).await,
		Command::Completions { shell } => Ok(misc::completions(*shell)),
		Command::About => Ok(misc::about(ctx, engine)),
		Command::Diagnostics => misc::diagnostics(ctx, engine).await,
	}
}

/// Completions for `shell`, without opening the launcher's data.
pub(crate) fn completions_only(shell: clap_complete::Shell) -> ExitStatus {
	misc::completions(shell)
}
