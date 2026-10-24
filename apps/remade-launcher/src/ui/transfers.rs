//! Installs and updates in flight, and the tray at the bottom of the window
//! that shows them.
//!
//! The tray is a status display, not a question: nothing here asks for
//! approval. Reviewed apps start installing the moment they are chosen, and
//! the tray shows how it is going.

use std::time::{Duration, Instant};

use egui::{Align, Layout, ProgressBar, RichText, Ui};
use launcher_model::InstallationId;

use crate::ui::components::{app_tile_for, icon_button};
use crate::ui::screens::sheet::bytes;
use crate::ui::{icons, theme};

/// How long a finished install stays in the tray.
const DONE_FOR: Duration = Duration::from_secs(5);
const FAILED_FOR: Duration = Duration::from_secs(10);
/// At most this many rows show; the rest are counted.
const MAX_ROWS: usize = 3;

/// Which install a row belongs to. Before the engine has chosen an
/// installation ID, a row is known only by its catalog slug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TransferKey {
	Slug(String),
	Installation(InstallationId),
}

#[derive(Clone, Debug)]
pub(crate) enum Phase {
	Working,
	Done(Instant),
	Failed(String, Instant),
}

#[derive(Clone, Debug)]
pub(crate) struct Transfer {
	pub(crate) key: TransferKey,
	pub(crate) name: String,
	pub(crate) slug: Option<String>,
	pub(crate) label: String,
	pub(crate) received: u64,
	pub(crate) total: Option<u64>,
	pub(crate) started: Instant,
	pub(crate) stoppable: bool,
	pub(crate) phase: Phase,
}

impl Transfer {
	fn fraction(&self) -> Option<f32> {
		self.total
			.filter(|total| *total > 0)
			.map(|total| (self.received as f64 / total as f64).clamp(0.0, 1.0) as f32)
	}

	/// Bytes per second since the transfer began, once it has moved any.
	fn speed(&self) -> Option<f64> {
		let seconds = self.started.elapsed().as_secs_f64();

		(self.received > 0 && seconds > 0.25).then(|| self.received as f64 / seconds)
	}
}

#[derive(Default)]
pub(crate) struct Transfers {
	items: Vec<Transfer>,
}

impl Transfers {
	/// The name shown for an installation's row, if it has one.
	pub(crate) fn name_of(&self, id: InstallationId) -> Option<String> {
		self.items
			.iter()
			.find(|item| item.key == TransferKey::Installation(id))
			.map(|item| item.name.clone())
	}

	/// Starts a row, replacing one with the same key.
	pub(crate) fn begin(
		&mut self,
		key: TransferKey,
		name: String,
		slug: Option<String>,
		label: &str,
	) {
		self.items.retain(|item| item.key != key);
		self.items.push(Transfer {
			key,
			name,
			slug,
			label: label.to_owned(),
			received: 0,
			total: None,
			started: Instant::now(),
			stoppable: false,
			phase: Phase::Working,
		});
	}

	/// Records progress for an installation. A row begun under a slug is
	/// adopted the first time the engine reports an ID. Returns false if
	/// there was no row to adopt and `name` was needed to make one.
	pub(crate) fn progress(
		&mut self,
		id: InstallationId,
		name: impl FnOnce() -> String,
		label: String,
		received: u64,
		total: Option<u64>,
		stoppable: bool,
	) {
		let key = TransferKey::Installation(id);

		if !self.items.iter().any(|item| item.key == key) {
			// Adopt the oldest row still waiting on an ID.
			match self.items.iter_mut().find(|item| {
				matches!(item.key, TransferKey::Slug(_)) && matches!(item.phase, Phase::Working)
			}) {
				Some(waiting) => waiting.key = key.clone(),
				None => self.begin(key.clone(), name(), None, &label),
			}
		}

		if let Some(item) = self.items.iter_mut().find(|item| item.key == key) {
			item.label = label;
			item.received = received.max(item.received);
			item.total = total.or(item.total);
			item.stoppable = stoppable;
		}
	}

	pub(crate) fn finish(&mut self, id: InstallationId) {
		if let Some(item) = self
			.items
			.iter_mut()
			.find(|item| item.key == TransferKey::Installation(id))
		{
			item.phase = Phase::Done(Instant::now());
			item.stoppable = false;
		}
	}

	pub(crate) fn fail(&mut self, id: InstallationId, message: String) {
		if let Some(item) = self
			.items
			.iter_mut()
			.find(|item| item.key == TransferKey::Installation(id))
		{
			item.phase = Phase::Failed(message, Instant::now());
			item.stoppable = false;
		}
	}

	/// Sets a row's state directly, for the demo.
	pub(crate) fn set(
		&mut self,
		key: &TransferKey,
		label: &str,
		received: u64,
		total: Option<u64>,
	) {
		if let Some(item) = self.items.iter_mut().find(|item| &item.key == key) {
			item.label = label.to_owned();
			item.received = received;
			item.total = total;
			item.stoppable = false;
		}
	}

	pub(crate) fn complete(&mut self, key: &TransferKey) {
		if let Some(item) = self.items.iter_mut().find(|item| &item.key == key) {
			item.phase = Phase::Done(Instant::now());
		}
	}

	/// Drops rows that are still waiting on a plan and have moved nothing,
	/// such as after an error or when a review dialog takes over.
	pub(crate) fn drop_waiting(&mut self) {
		self.items.retain(|item| {
			!(matches!(item.phase, Phase::Working) && item.received == 0 && !item.stoppable)
		});
	}

	/// Forgets rows that have been done or failed long enough. Returns true
	/// while something is still moving or about to expire, so the caller can
	/// keep repainting.
	pub(crate) fn prune(&mut self) -> bool {
		self.items.retain(|item| match &item.phase {
			Phase::Working => true,
			Phase::Done(at) => at.elapsed() < DONE_FOR,
			Phase::Failed(_, at) => at.elapsed() < FAILED_FOR,
		});

		!self.items.is_empty()
	}

	/// Draws the rows. Returns true if Stop was pressed.
	pub(crate) fn show(&self, ui: &mut Ui) -> bool {
		let mut stop = false;
		let shown = self.items.iter().rev().take(MAX_ROWS);

		for (index, item) in shown.enumerate() {
			if index > 0 {
				ui.add_space(10.0);
			}

			stop |= row(ui, item);
		}

		if self.items.len() > MAX_ROWS {
			ui.add_space(6.0);
			ui.label(
				RichText::new(format!("and {} more", self.items.len() - MAX_ROWS))
					.small()
					.color(theme::muted(ui)),
			);
		}

		stop
	}
}

fn row(ui: &mut Ui, item: &Transfer) -> bool {
	let palette = theme::palette(ui);
	let mut stop = false;

	ui.horizontal(|ui| {
		app_tile_for(ui, item.slug.as_deref(), &item.name, 44.0);
		ui.add_space(6.0);

		ui.vertical(|ui| {
			ui.set_width((ui.available_width() - 60.0).max(120.0));
			ui.spacing_mut().item_spacing.y = 4.0;

			ui.horizontal(|ui| {
				ui.label(RichText::new(&item.name).family(theme::semibold()));

				ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
					ui.label(
						RichText::new(status_line(item))
							.small()
							.color(palette.text_muted),
					);
				});
			});

			match &item.phase {
				Phase::Working => {
					let bar = match item.fraction() {
						Some(fraction) => ProgressBar::new(fraction),
						None => ProgressBar::new(0.0).animate(true),
					};

					ui.add(
						bar.desired_height(8.0)
							.desired_width(ui.available_width())
							.corner_radius(4),
					);
					ui.label(RichText::new(&item.label).small().color(palette.text_muted));
				}
				Phase::Done(_) => {
					ui.label(
						RichText::new(format!("{}  Installed", icons::SEAL_CHECK))
							.color(palette.accent),
					);
				}
				Phase::Failed(message, _) => {
					ui.colored_label(ui.visuals().warn_fg_color, message);
				}
			}
		});

		if item.stoppable && icon_button(ui, icons::X, "Stop", 28.0).clicked() {
			stop = true;
		}
	});

	stop
}

/// "12.3 MB of 48.3 MB · 9.1 MB/s", as much as is known.
fn status_line(item: &Transfer) -> String {
	if !matches!(item.phase, Phase::Working) || item.received == 0 {
		return String::new();
	}

	let mut parts = Vec::new();

	parts.push(match item.total {
		Some(total) => format!("{} of {}", bytes(item.received), bytes(total)),
		None => bytes(item.received),
	});

	if let Some(speed) = item.speed()
		&& item.fraction().is_none_or(|fraction| fraction < 1.0)
	{
		parts.push(format!("{}/s", bytes(speed as u64)));
	}

	parts.join(" · ")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn id() -> InstallationId {
		InstallationId::new()
	}

	#[test]
	fn the_first_engine_report_adopts_the_row_waiting_on_an_id() {
		let mut transfers = Transfers::default();
		let installation = id();

		transfers.begin(
			TransferKey::Slug("quill".into()),
			"Quill".into(),
			Some("quill".into()),
			"Checking",
		);
		transfers.progress(
			installation,
			|| "Other".into(),
			"Downloading".into(),
			10,
			Some(100),
			true,
		);

		assert_eq!(transfers.items.len(), 1);
		assert_eq!(
			transfers.items[0].key,
			TransferKey::Installation(installation)
		);
		assert_eq!(transfers.items[0].name, "Quill");
	}

	#[test]
	fn an_unexpected_install_gets_a_row_of_its_own() {
		let mut transfers = Transfers::default();

		transfers.progress(
			id(),
			|| "From the CLI".into(),
			"Downloading".into(),
			1,
			None,
			true,
		);

		assert_eq!(transfers.items[0].name, "From the CLI");
	}

	#[test]
	fn progress_never_goes_backwards() {
		let mut transfers = Transfers::default();
		let installation = id();

		transfers.progress(installation, || "A".into(), "x".into(), 50, Some(100), true);
		transfers.progress(installation, || "A".into(), "x".into(), 20, Some(100), true);

		assert_eq!(transfers.items[0].received, 50);
		assert_eq!(transfers.items[0].fraction(), Some(0.5));
	}

	#[test]
	fn finished_rows_expire_but_working_ones_stay() {
		let mut transfers = Transfers::default();
		let (done, working) = (id(), id());

		transfers.progress(done, || "Done".into(), "x".into(), 1, None, false);
		transfers.progress(working, || "Working".into(), "x".into(), 1, None, true);
		transfers.finish(done);
		transfers.items[0].phase = Phase::Done(Instant::now() - DONE_FOR - Duration::from_secs(1));

		assert!(transfers.prune());
		assert_eq!(transfers.items.len(), 1);
		assert_eq!(transfers.items[0].name, "Working");
	}

	#[test]
	fn dropping_waiting_rows_leaves_real_transfers_alone() {
		let mut transfers = Transfers::default();

		transfers.begin(TransferKey::Slug("a".into()), "A".into(), None, "Checking");
		transfers.progress(id(), || "B".into(), "Downloading".into(), 5, Some(10), true);
		transfers.begin(TransferKey::Slug("c".into()), "C".into(), None, "Checking");
		transfers.drop_waiting();

		assert_eq!(transfers.items.len(), 1);
		assert_eq!(
			transfers.items[0].name, "A",
			"the row that was adopted keeps its name"
		);
	}
}
