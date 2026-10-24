//! The menu bar.
//!
//! macOS gets a native application menu, with Settings on Cmd+, as the
//! platform expects. Other platforms have no global menu bar, so they rely
//! on the same shortcut handled inside the window.

use std::sync::mpsc::Receiver;

/// What a menu item asks the interface to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuCommand {
	Settings,
	About,
}

pub(crate) struct AppMenu {
	commands: Receiver<MenuCommand>,
	#[cfg(target_os = "macos")]
	_menu: muda::Menu,
}

impl AppMenu {
	/// Installs the menu. `context` is woken whenever an item is chosen.
	#[cfg(target_os = "macos")]
	pub(crate) fn install(context: &egui::Context) -> Self {
		use std::sync::Mutex;
		use std::sync::mpsc;

		use launcher_model::product;
		use muda::accelerator::{Accelerator, Code, Modifiers};
		use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

		// The menu bar names the application after its process. Unpackaged
		// runs would otherwise show the executable's file name.
		objc2_foundation::NSProcessInfo::processInfo()
			.setProcessName(&objc2_foundation::NSString::from_str(product::NAME));

		let (sender, commands) = mpsc::channel();
		let sender = Mutex::new(sender);
		let context = context.clone();

		let about = MenuItem::with_id("about", format!("About {}", product::NAME), true, None);
		let settings = MenuItem::with_id(
			"settings",
			"Settings…",
			true,
			Some(Accelerator::new(Modifiers::META, Code::Comma)),
		);

		MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
			let command = match event.id().0.as_str() {
				"about" => MenuCommand::About,
				"settings" => MenuCommand::Settings,
				_ => return,
			};

			if let Ok(sender) = sender.lock() {
				let _ = sender.send(command);
			}

			context.request_repaint();
		}));

		let application = Submenu::new(product::NAME, true);
		let _ = application.append_items(&[
			&about,
			&PredefinedMenuItem::separator(),
			&settings,
			&PredefinedMenuItem::separator(),
			&PredefinedMenuItem::services(None),
			&PredefinedMenuItem::separator(),
			&PredefinedMenuItem::hide(None),
			&PredefinedMenuItem::hide_others(None),
			&PredefinedMenuItem::show_all(None),
			&PredefinedMenuItem::separator(),
			&PredefinedMenuItem::quit(None),
		]);

		let menu = Menu::new();
		let _ = menu.append(&application);

		menu.init_for_nsapp();

		Self {
			commands,
			_menu: menu,
		}
	}

	#[cfg(not(target_os = "macos"))]
	pub(crate) fn install(_context: &egui::Context) -> Self {
		let (_sender, commands) = std::sync::mpsc::channel();

		Self { commands }
	}

	/// Everything chosen since the last call.
	pub(crate) fn poll(&self) -> Vec<MenuCommand> {
		self.commands.try_iter().collect()
	}
}
