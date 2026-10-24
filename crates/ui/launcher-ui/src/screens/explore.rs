use egui::Ui;

use super::{page, unavailable};

pub(crate) fn explore(ui: &mut Ui) {
	page(ui, "Explore", |ui| {
		unavailable(
			ui,
			"The catalog is not connected yet",
			"Browsing apps needs the signed Remade catalog, which arrives in a later milestone.",
		);
	});
}
