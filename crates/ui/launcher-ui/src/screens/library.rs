use egui::Ui;

use super::{page, unavailable};

pub(crate) fn library(ui: &mut Ui) {
	page(ui, "Library", |ui| {
		unavailable(
			ui,
			"Nothing installed yet",
			"Apps you install with Remade Launcher will appear here.",
		);
	});
}
