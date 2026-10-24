//! What the person needs to know about the catalog itself.
//!
//! Network trouble and rejected bytes are worded differently, because they
//! mean different things: one is "try again later", the other is "do not
//! trust this".

use egui::Ui;
use launcher_core::{CatalogState, Origin, ProblemKind};
use time::OffsetDateTime;

use crate::ui::commands::Commands;
use crate::ui::components::{Tone, notice};
use crate::ui::{icons, theme};

/// What the banner has to say, separated from how it is drawn.
#[derive(Debug, PartialEq, Eq)]
struct Message {
	tone: Tone,
	icon: &'static str,
	headline: String,
	detail: Option<String>,
	/// Offers a retry.
	retry: bool,
}

/// The facts a banner depends on, as plain values.
#[derive(Default)]
struct Facts {
	has_catalog: bool,
	/// When a cached catalog was accepted, if that is where it came from.
	cached_on: Option<i64>,
	checked_at: Option<i64>,
	expired_on: Option<i64>,
	problem: Option<(ProblemKind, String)>,
	clock_warning: bool,
	refreshing: bool,
}

fn facts(state: &CatalogState) -> Facts {
	let mut facts = Facts {
		has_catalog: state.catalog.is_some(),
		problem: state
			.problem
			.as_ref()
			.map(|problem| (problem.kind, problem.message.clone())),
		clock_warning: state.clock_warning,
		refreshing: state.refreshing,
		..Facts::default()
	};

	match &state.origin {
		Some(Origin::Cache { accepted_at }) => facts.cached_on = Some(accepted_at.unix_timestamp()),
		Some(Origin::Network { at }) => facts.checked_at = Some(at.unix_timestamp()),
		None => {}
	}

	if let Some(catalog) = &state.catalog
		&& state.freshness(OffsetDateTime::now_utc()) == Some(launcher_catalog::Freshness::Expired)
	{
		facts.expired_on = Some(catalog.expires_at().unix_timestamp());
	}

	facts
}

fn messages(facts: &Facts) -> Vec<Message> {
	let date = launcher_model::local_time::date;
	let mut shown = Vec::new();

	match &facts.problem {
		Some((ProblemKind::Network, _)) => shown.push(match (facts.has_catalog, facts.cached_on) {
			(true, Some(cached)) => Message {
				tone: Tone::Warning,
				icon: icons::WIFI_SLASH,
				headline: "Could not reach Remade servers, using cached catalog info.".into(),
				detail: Some(format!("The cached copy was verified on {}. New apps and releases may be missing.", date(cached))),
				retry: true,
			},
			(true, None) => Message {
				tone: Tone::Warning,
				icon: icons::WIFI_SLASH,
				headline: "Could not reach Remade servers, using catalog info you already have.".into(),
				detail: None,
				retry: true,
			},
			(false, _) => Message {
				tone: Tone::Warning,
				icon: icons::WIFI_SLASH,
				headline: "Could not reach Remade servers.".into(),
				detail: Some("There is no cached catalog to fall back on yet. Check your connection and try again.".into()),
				retry: true,
			},
		}),
		Some((kind, why)) => {
			let headline = match kind {
				ProblemKind::Network => unreachable!("handled above"),
				ProblemKind::Security => "The catalog was rejected.",
				ProblemKind::Unusable => "The catalog could not be used.",
				ProblemKind::NoTrustedKeys => "The catalog is not configured.",
				ProblemKind::Storage => "The catalog could not be saved.",
			};

			shown.push(Message {
				tone: Tone::Warning,
				icon: icons::WARNING_CIRCLE,
				headline: headline.into(),
				detail: Some(why.clone()),
				retry: true,
			});
		}
		None => {}
	}

	if let Some(expired) = facts.expired_on {
		shown.push(Message {
			tone: Tone::Warning,
			icon: icons::WARNING_CIRCLE,
			headline: format!("This catalog expired on {}.", date(expired)),
			detail: Some("You can browse it, but new installs and updates are on hold until it is refreshed.".into()),
			retry: true,
		});
	}

	if facts.clock_warning {
		shown.push(Message {
			tone: Tone::Warning,
			icon: icons::WARNING_CIRCLE,
			headline: "Your computer's clock looks wrong.".into(),
			detail: Some(
				"It is earlier than a time the launcher already trusted. Check the date and time."
					.into(),
			),
			retry: false,
		});
	}

	shown
}

pub(super) fn banner(ui: &mut Ui, state: &CatalogState, commands: &Commands) {
	draw(ui, &facts(state), commands);
}

/// The banner for the demo catalog: offline, with a cached copy, so the
/// warning can be seen without unplugging anything.
pub(super) fn demo_banner(ui: &mut Ui, commands: &Commands) {
	let now = time::OffsetDateTime::now_utc().unix_timestamp();

	draw(
		ui,
		&Facts {
			has_catalog: true,
			cached_on: Some(now - 3 * 86_400),
			problem: Some((ProblemKind::Network, String::new())),
			..Facts::default()
		},
		commands,
	);
}

fn draw(ui: &mut Ui, facts: &Facts, commands: &Commands) {
	let problems = messages(facts);

	for message in &problems {
		let button = (message.retry && !facts.refreshing).then_some("Try again");

		if notice(
			ui,
			message.tone,
			message.icon,
			&message.headline,
			message.detail.as_deref(),
			button,
		) {
			commands.refresh_catalog();
		}

		ui.add_space(6.0);
	}

	ui.horizontal(|ui| {
		if facts.refreshing {
			ui.spinner();
			ui.label("Checking for catalog updates…");

			return;
		}

		let line = match (facts.checked_at, facts.cached_on, facts.has_catalog) {
			(Some(at), _, true) => format!(
				"Verified catalog, checked {}.",
				launcher_model::local_time::date(at)
			),
			(_, Some(on), true) if problems.is_empty() => format!(
				"Using verified catalog cached on {}.",
				launcher_model::local_time::date(on)
			),
			(_, _, false) if problems.is_empty() => "No catalog has been loaded yet.".to_owned(),
			_ => String::new(),
		};

		if !line.is_empty() {
			ui.label(egui::RichText::new(line).small().color(theme::muted(ui)));
		}

		if problems.is_empty() && ui.small_button("Check for updates").clicked() {
			commands.refresh_catalog();
		}
	});

	ui.add_space(6.0);
}

#[cfg(test)]
mod tests {
	use super::*;

	fn headline(facts: &Facts) -> Vec<String> {
		messages(facts)
			.into_iter()
			.map(|message| message.headline)
			.collect()
	}

	#[test]
	fn offline_with_a_cached_catalog_says_so_plainly() {
		let shown = messages(&Facts {
			has_catalog: true,
			cached_on: Some(1_792_874_520),
			problem: Some((ProblemKind::Network, "no route".into())),
			..Facts::default()
		});

		assert_eq!(shown.len(), 1);
		assert_eq!(
			shown[0].headline,
			"Could not reach Remade servers, using cached catalog info."
		);
		assert_eq!(shown[0].tone, Tone::Warning);
		assert!(shown[0].retry);
		assert!(shown[0].detail.as_deref().unwrap().contains("verified on"));
	}

	#[test]
	fn offline_with_nothing_cached_says_there_is_nothing_to_fall_back_on() {
		let shown = messages(&Facts {
			problem: Some((ProblemKind::Network, String::new())),
			..Facts::default()
		});

		assert_eq!(shown[0].headline, "Could not reach Remade servers.");
		assert!(
			shown[0]
				.detail
				.as_deref()
				.unwrap()
				.contains("no cached catalog")
		);
	}

	#[test]
	fn a_rejected_catalog_is_never_worded_like_a_network_problem() {
		let shown = headline(&Facts {
			problem: Some((ProblemKind::Security, "bad signature".into())),
			..Facts::default()
		});

		assert_eq!(shown, ["The catalog was rejected."]);
	}

	#[test]
	fn a_healthy_catalog_shows_no_warning() {
		assert!(
			messages(&Facts {
				has_catalog: true,
				checked_at: Some(1),
				..Facts::default()
			})
			.is_empty()
		);
	}
}
