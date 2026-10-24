//! How the launcher reaches the internet, and the GitHub token.

use egui::{RichText, TextEdit, Ui};
use launcher_core::{RateLimit, TokenState};
use launcher_model::preferences::ProxyProblem;
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
		&t!("settings-proxy"),
		Some(&t!("settings-proxy.description")),
		|ui| {
			let mode = match &prefs.network.proxy {
				Proxy::System => 0,
				Proxy::Direct => 1,
				Proxy::Custom(_) => 2,
			};
			let mut chosen = mode;
			let options = [
				(0, t!("settings-proxy-auto")),
				(1, t!("settings-proxy-none")),
				(2, t!("settings-proxy-custom")),
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
					ui.colored_label(ui.visuals().warn_fg_color, proxy_problem_text(problem));
				}

				ui.label(
					RichText::new(t!("settings-proxy-hint"))
						.small()
						.color(theme::muted(ui)),
				);

				if ui.button(t!("settings-proxy-use")).clicked() {
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
		&t!("settings-timeout"),
		Some(&t!("settings-timeout.description")),
		|ui| {
			let mut chosen = prefs.network.timeout_seconds;
			let options: Vec<(u32, String)> = [15, 30, 60, 120, 300]
				.into_iter()
				.map(|seconds| (seconds, t!("settings-seconds", count = i64::from(seconds))))
				.collect();

			// A value saved by hand that is not on the list still shows as itself.
			let mut options = options;

			if !options.iter().any(|(seconds, _)| *seconds == chosen) {
				options.push((chosen, t!("settings-seconds", count = i64::from(chosen))));
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
		RichText::new(t!("settings-github"))
			.family(theme::semibold())
			.size(15.0),
	);
	ui.add(
		egui::Label::new(
			RichText::new(t!("settings-github-intro"))
				.small()
				.color(theme::muted(ui)),
		)
		.wrap(),
	);
	ui.add_space(8.0);

	match view.token {
		TokenState::Stored => {
			ui.label(format!(
				"{}  {}",
				crate::ui::icons::SEAL_CHECK,
				t!("settings-token-stored")
			));

			if ui.button(t!("settings-token-remove")).clicked() {
				*action = SettingsAction::ClearToken;
			}
		}
		TokenState::FromEnvironment => {
			ui.label(t!(
				"settings-token-environment",
				name = launcher_core::TOKEN_VARIABLE
			));
		}
		TokenState::Unavailable(why) => {
			ui.colored_label(ui.visuals().warn_fg_color, t!("settings-token-unavailable"));
			ui.label(RichText::new(why).small().color(theme::muted(ui)));
		}
		TokenState::Absent => {
			ui.horizontal(|ui| {
				ui.add(
					TextEdit::singleline(&mut inputs.token_text)
						.password(true)
						.hint_text(t!("settings-token-paste"))
						.desired_width(280.0),
				);

				let ready = !inputs.token_text.trim().is_empty();

				if ui
					.add_enabled(ready, egui::Button::new(t!("settings-token-save")))
					.clicked()
				{
					*action = SettingsAction::SaveToken(std::mem::take(&mut inputs.token_text));
				}
			});
		}
	}

	ui.add_space(8.0);

	ui.horizontal(|ui| {
		if ui.button(t!("settings-rate-check")).clicked() {
			*action = SettingsAction::CheckRateLimit;
		}

		match view.rate {
			Some(Ok(limit)) => {
				let resets = launcher_model::local_time::friendly(
					limit.resets_at,
					time::OffsetDateTime::now_utc().unix_timestamp(),
				);

				ui.label(t!(
					"settings-rate-left",
					remaining = limit.remaining as i64,
					limit = limit.limit as i64,
					resets = resets.to_lowercase(),
					note = if limit.signed_in {
						t!("settings-rate-token")
					} else {
						t!("settings-rate-anonymous")
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

/// Checks a proxy address, giving what was wrong if it is not usable.
fn check_proxy_text(text: &str) -> Result<(), ProxyProblem> {
	launcher_model::preferences::check_proxy(text)
}

fn proxy_problem_text(problem: ProxyProblem) -> String {
	match problem {
		ProxyProblem::Empty => t!("settings-proxy-problem-empty"),
		ProxyProblem::NotAnAddress => t!("settings-proxy-problem-address"),
		ProxyProblem::UnsupportedScheme => t!("settings-proxy-problem-scheme"),
		ProxyProblem::HasCredentials => t!("settings-proxy-problem-credentials"),
	}
}
