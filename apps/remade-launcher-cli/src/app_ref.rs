//! Finding an installed app from what a person typed.

use launcher_core::{AdoptedApp, InstalledApp};
use launcher_model::{ErrorCode, LauncherError};

/// Matches `reference` against an app's ID, catalog name, or display
/// name, in that order of precision, and refuses to guess between several.
pub(crate) fn find<'a>(
	apps: &'a [InstalledApp],
	reference: &str,
) -> Result<&'a InstalledApp, LauncherError> {
	let wanted = reference.trim().to_lowercase();

	let by = |keep: &dyn Fn(&InstalledApp) -> bool| -> Vec<&'a InstalledApp> {
		apps.iter().filter(|app| keep(app)).collect()
	};

	let attempts: [Vec<&InstalledApp>; 3] = [
		by(&|app| app.id.matches(&wanted)),
		by(&|app| {
			app.catalog_slug
				.as_deref()
				.is_some_and(|slug| slug.eq_ignore_ascii_case(&wanted))
		}),
		by(&|app| app.name.to_lowercase() == wanted),
	];

	for matches in attempts {
		match matches.as_slice() {
			[] => continue,
			[only] => return Ok(only),
			several => {
				let names: Vec<String> = several
					.iter()
					.map(|app| format!("{} ({})", app.name, short_id(app)))
					.collect();

				return Err(LauncherError::new(
					ErrorCode::InvalidSourceUrl,
					format!(
						"\"{reference}\" matches more than one app: {}. Use the ID.",
						names.join(", ")
					),
				));
			}
		}
	}

	Err(LauncherError::new(
		ErrorCode::InstallationNotFound,
		format!("No installed app matches \"{reference}\". See `remade list`."),
	))
}

/// Matches `reference` against watched apps the same way, or finds nothing.
pub(crate) fn find_adopted<'a>(
	apps: &'a [AdoptedApp],
	reference: &str,
) -> Result<Option<&'a AdoptedApp>, LauncherError> {
	let wanted = reference.trim().to_lowercase();
	let attempts: [Vec<&AdoptedApp>; 3] = [
		apps.iter().filter(|app| app.id.matches(&wanted)).collect(),
		apps.iter()
			.filter(|app| {
				app.catalog_slug
					.as_deref()
					.is_some_and(|slug| slug.eq_ignore_ascii_case(&wanted))
			})
			.collect(),
		apps.iter()
			.filter(|app| app.name.to_lowercase() == wanted)
			.collect(),
	];

	for matches in attempts {
		match matches.as_slice() {
			[] => continue,
			[only] => return Ok(Some(only)),
			_ => {
				return Err(LauncherError::new(
					ErrorCode::InvalidSourceUrl,
					format!("\"{reference}\" matches more than one watched app. Use the ID."),
				));
			}
		}
	}

	Ok(None)
}

/// The short form of the ID, enough to tell apps apart.
pub(crate) fn short_id(app: &InstalledApp) -> String {
	app.id.short()
}

#[cfg(test)]
mod tests {
	use launcher_model::InstallationId;

	use super::*;

	fn app(name: &str, slug: Option<&str>) -> InstalledApp {
		InstalledApp {
			id: InstallationId::new(),
			name: name.into(),
			repository: "o/r".into(),
			version: "v1".into(),
			curated: slug.is_some(),
			catalog_slug: slug.map(str::to_owned),
			root: String::new(),
			launchable: true,
			evidence: String::new(),
			installed_at: 0,
		}
	}

	#[test]
	fn matches_by_slug_name_and_id() {
		let apps = vec![
			app("Sample Editor", Some("sample-editor")),
			app("Other", None),
		];

		assert_eq!(find(&apps, "sample-editor").unwrap().name, "Sample Editor");
		assert_eq!(find(&apps, "SAMPLE EDITOR").unwrap().name, "Sample Editor");
		assert_eq!(find(&apps, "other").unwrap().name, "Other");

		let prefix = short_id(&apps[1]);
		assert_eq!(find(&apps, &prefix).unwrap().name, "Other");
	}

	#[test]
	fn refuses_unknown_and_ambiguous_references() {
		let apps = vec![app("Twin", None), app("Twin", None)];

		assert_eq!(
			find(&apps, "twin").unwrap_err().code(),
			ErrorCode::InvalidSourceUrl
		);
		assert_eq!(
			find(&apps, "nothing").unwrap_err().code(),
			ErrorCode::InstallationNotFound
		);
		assert!(find(&apps, "ab").is_err(), "a tiny fragment is not an ID");
	}
}
