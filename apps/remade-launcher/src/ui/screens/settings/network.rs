//! How the launcher reaches the internet, and the GitHub token.

use egui::{RichText, TextEdit, Ui};
use launcher_core::{RateLimit, TokenState};
use launcher_model::{Preferences, Proxy};

use super::{SettingsAction, SettingsInputs};
use crate::ui::components::{choice_row, setting_row};
use crate::ui::theme;

pub(super) struct NetworkView<'a> {
	pub(super) prefs: &'a Preferences,
	pub(super) token: &'a TokenState,
	pub(super) rate: Option<&'a Result<RateLimit, String>>,
}

pub(super) fn network(
	ui: &mut Ui,
	view: &NetworkView<'_>,
	inputs: &mut SettingsInputs,
	action: &mut SettingsAction,
) {
	proxy(ui, view.prefs, inputs, action);
	timeout(ui, view.prefs, action);

	ui.add_space(4.0);
	ui.separator();
	ui.add_space(10.0);

	github(ui, view, inputs, action);
}

fn proxy(
	ui: &mut Ui,
	prefs: &Preferences,
	inputs: &mut SettingsInputs,
	action: &mut SettingsAction,
) {
	setting_row(
		ui,
		"Proxy",
		Some(
			"Automatic uses your system's proxy settings, including the usual environment variables.",
		),
		|ui| {
			let mode = match &prefs.network.proxy {
				Proxy::System => 0,
				Proxy::Direct => 1,
				Proxy::Custom(_) => 2,
			};
			let mut chosen = mode;
			let options = [
				(0, "Automatic (system)".to_owned()),
				(1, "No proxy".to_owned()),
				(2, "Custom address".to_owned()),
			];

			if choice_row(ui, "proxy-mode", &mut chosen, &options) && chosen != mode {
				let mut next = prefs.clone();

				match chosen {
					0 => next.network.proxy = Proxy::System,
					1 => next.network.proxy = Proxy::Direct,
					_ => {
						// Wait for an address before saving a custom proxy.
						inputs.proxy_editing = true;
						inputs.proxy_error = None;
					}
				}

				if chosen != 2 {
					inputs.proxy_editing = false;
					*action = SettingsAction::Preferences(Box::new(next));
				}
			}

			if mode == 2 || inputs.proxy_editing {
				ui.add_space(4.0);

				ui.add(
					TextEdit::singleline(&mut inputs.proxy_text)
						.hint_text("http://proxy.example:8080")
						.desired_width(260.0),
				);

				if let Some(problem) = inputs.proxy_error {
					ui.colored_label(ui.visuals().warn_fg_color, problem);
				}

				ui.label(RichText::new("http://, https://, or socks5://. Leave user names and passwords out: they would be saved unprotected.").small().color(theme::muted(ui)));

				if ui.button("Use this proxy").clicked() {
					match check_proxy_text(&inputs.proxy_text) {
						Ok(()) => {
							let mut next = prefs.clone();

							next.network.proxy = Proxy::Custom(inputs.proxy_text.trim().to_owned());
							inputs.proxy_error = None;
							inputs.proxy_editing = false;
							*action = SettingsAction::Preferences(Box::new(next));
						}
						Err(problem) => inputs.proxy_error = Some(problem),
					}
				}
			}
		},
	);
}

fn timeout(ui: &mut Ui, prefs: &Preferences, action: &mut SettingsAction) {
	setting_row(
		ui,
		"Request timeout",
		Some("How long to wait for a server before giving up. Slow connections may need longer."),
		|ui| {
			let mut chosen = prefs.network.timeout_seconds;
			let options: Vec<(u32, String)> = [15, 30, 60, 120, 300]
				.into_iter()
				.map(|seconds| (seconds, format!("{seconds} seconds")))
				.collect();

			// A value saved by hand that is not on the list still shows as itself.
			let mut options = options;

			if !options.iter().any(|(seconds, _)| *seconds == chosen) {
				options.push((chosen, format!("{chosen} seconds")));
			}

			if choice_row(ui, "timeout", &mut chosen, &options) {
				let mut next = prefs.clone();

				next.network.timeout_seconds = chosen;
				*action = SettingsAction::Preferences(Box::new(next));
			}
		},
	);
}

fn github(
	ui: &mut Ui,
	view: &NetworkView<'_>,
	inputs: &mut SettingsInputs,
	action: &mut SettingsAction,
) {
	ui.label(
		RichText::new("GitHub access")
			.family(theme::semibold())
			.size(15.0),
	);
	ui.add(
		egui::Label::new(
			RichText::new("GitHub allows 60 lookups an hour to anyone, and 5,000 with a free token. The launcher remembers what it has already asked, so most checks cost nothing. Add a token only if you keep a lot of apps and still hit the limit. A token with no permissions is enough, because everything the launcher reads is public.")
				.small()
				.color(theme::muted(ui)),
		)
		.wrap(),
	);
	ui.add_space(8.0);

	match view.token {
		TokenState::Stored => {
			ui.label(format!(
				"{}  A token is saved in your system's credential store.",
				crate::ui::icons::SEAL_CHECK
			));

			if ui.button("Remove token").clicked() {
				*action = SettingsAction::ClearToken;
			}
		}
		TokenState::FromEnvironment => {
			ui.label(format!(
				"A token comes from the {} setting in your environment.",
				launcher_core::TOKEN_VARIABLE
			));
		}
		TokenState::Unavailable(why) => {
			ui.colored_label(
				ui.visuals().warn_fg_color,
				"Your system's credential store is not available, so a token cannot be kept safely.",
			);
			ui.label(RichText::new(why).small().color(theme::muted(ui)));
		}
		TokenState::Absent => {
			ui.horizontal(|ui| {
				ui.add(
					TextEdit::singleline(&mut inputs.token_text)
						.password(true)
						.hint_text("Paste a token")
						.desired_width(280.0),
				);

				let ready = !inputs.token_text.trim().is_empty();

				if ui
					.add_enabled(ready, egui::Button::new("Save token"))
					.clicked()
				{
					*action = SettingsAction::SaveToken(std::mem::take(&mut inputs.token_text));
				}
			});
		}
	}

	ui.add_space(8.0);

	ui.horizontal(|ui| {
		if ui.button("Check my limit").clicked() {
			*action = SettingsAction::CheckRateLimit;
		}

		match view.rate {
			Some(Ok(limit)) => {
				let resets = launcher_model::local_time::friendly(
					limit.resets_at,
					time::OffsetDateTime::now_utc().unix_timestamp(),
				);

				ui.label(format!(
					"{} of {} lookups left. Resets {}. {}",
					limit.remaining,
					limit.limit,
					resets.to_lowercase(),
					if limit.signed_in {
						"Using your token."
					} else {
						"Not signed in."
					}
				));
			}
			Some(Err(why)) => {
				ui.colored_label(ui.visuals().warn_fg_color, why);
			}
			None => {}
		}
	});
}

/// Checks a proxy address, giving the message to show if it is not usable.
fn check_proxy_text(text: &str) -> Result<(), &'static str> {
	launcher_model::preferences::check_proxy(text).map_err(|problem| problem.message())
}
