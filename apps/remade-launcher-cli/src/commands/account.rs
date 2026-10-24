//! `auth` and `reviews`.
//!
//! `auth` manages the optional GitHub token that raises the lookup limit.
//! It is not a Remade account: the launcher never needs to sign in to a
//! community service to install, update, or remove anything.
//!
//! `reviews` belongs to a community service that does not exist yet. The
//! commands are here so scripts get a clear, typed answer instead of a guess,
//! and they send nothing anywhere.

use std::io::{IsTerminal, Read};
use std::sync::Arc;

use launcher_core::{Engine, TokenState};
use serde_json::json;

use crate::cli::{AuthAction, ReviewsAction};
use crate::context::Ctx;
use crate::errors::CliError;
use crate::exit::ExitStatus;
use crate::output::refuse;

/// What a token's presence means, as a stable word.
fn state_code(state: &TokenState) -> &'static str {
	match state {
		TokenState::Absent => "none",
		TokenState::Stored => "stored",
		TokenState::FromEnvironment => "environment",
		TokenState::Unavailable(_) => "credential_store_unavailable",
	}
}

fn state_text(state: &TokenState) -> String {
	match state {
		TokenState::Absent => {
			"No GitHub token is set. Lookups are limited to 60 an hour.".to_owned()
		}
		TokenState::Stored => {
			"A GitHub token is saved in your system's credential store.".to_owned()
		}
		TokenState::FromEnvironment => format!(
			"A token comes from {} in your environment.",
			launcher_core::TOKEN_VARIABLE
		),
		TokenState::Unavailable(why) => format!(
			"Your system's credential store is not available ({why}), so a token cannot be kept."
		),
	}
}

/// Reads a token from standard input, hidden when it is a terminal.
fn read_token(ctx: &Ctx, from_stdin: bool) -> Result<String, CliError> {
	if from_stdin {
		let mut text = String::new();

		std::io::stdin()
			.take(4096)
			.read_to_string(&mut text)
			.map_err(|_| CliError::usage("The token could not be read."))?;

		return Ok(text);
	}

	if ctx.no_input || !std::io::stdin().is_terminal() {
		return Err(CliError::usage(
			"There is no way to ask for the token. Pass it on standard input with --with-token.",
		));
	}

	rpassword::prompt_password("GitHub token (it will not be shown): ")
		.map_err(|_| CliError::usage("The token could not be read."))
}

pub(crate) async fn auth(
	ctx: &Ctx,
	engine: &Arc<Engine>,
	action: &AuthAction,
) -> Result<ExitStatus, CliError> {
	match action {
		AuthAction::Status => {
			let state = engine.token_state().await;
			let limit = if ctx.offline {
				None
			} else {
				engine.rate_limit().await.ok()
			};

			if ctx.machine() {
				ctx.emit(json!({
					"github_token": state_code(&state),
					"rate_limit": limit.as_ref().map(|limit| json!({
						"limit": limit.limit,
						"remaining": limit.remaining,
						"resets_at": crate::output::iso(limit.resets_at),
						"signed_in": limit.signed_in,
					})),
					"community_account": "unavailable",
				}));
			} else {
				println!("{}", state_text(&state));

				match &limit {
					Some(limit) => println!(
						"{} of {} lookups left; resets {}.",
						limit.remaining,
						limit.limit,
						crate::output::when(limit.resets_at)
					),
					None if ctx.offline => println!("Offline, so the limit was not checked."),
					None => println!("GitHub could not say where the limit stands."),
				}

				println!(
					"A Remade community account is not available yet. Nothing here needs one."
				);
			}

			Ok(ExitStatus::Success)
		}
		AuthAction::Login { with_token, device } => {
			if *device {
				return Ok(refuse(
					ctx,
					"AUTH_UNAVAILABLE",
					ExitStatus::Unsupported,
					"Signing in through a browser needs an app registered with GitHub, which this build does not have. Use --with-token and a token you created at github.com/settings/tokens.",
				));
			}

			let token = read_token(ctx, *with_token)?;

			engine.set_github_token(&token).await?;

			if ctx.machine() {
				ctx.emit(json!({ "github_token": state_code(&engine.token_state().await) }));
			} else {
				println!("Saved. Lookups can now use your token's higher limit.");
				println!("It needs no permissions: everything the launcher reads is public.");
			}

			Ok(ExitStatus::Success)
		}
		AuthAction::Logout => {
			engine.clear_github_token().await?;

			if ctx.machine() {
				ctx.emit(json!({ "github_token": "none" }));
			} else {
				println!("The saved token was removed.");
			}

			Ok(ExitStatus::Success)
		}
	}
}

pub(crate) fn reviews(ctx: &Ctx, action: &ReviewsAction) -> ExitStatus {
	// Every action gets the same honest answer.
	let _ = action;

	refuse(
		ctx,
		"COMMUNITY_UNAVAILABLE",
		ExitStatus::Unsupported,
		"Reviews need the Remade community service, which does not exist yet. Nothing was sent, and installing, updating, and removing apps never need it.",
	)
}
