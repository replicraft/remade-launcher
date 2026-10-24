use std::io::{BufRead, IsTerminal, Write};

use launcher_model::{ErrorCode, LauncherError};

/// Asks a yes/no question. `assume_yes` answers it without asking.
///
/// When there is no terminal to ask on, the answer is an error rather than a
/// silent "yes": a script must opt in with `--yes`.
pub(crate) fn confirm(question: &str, assume_yes: bool) -> Result<bool, LauncherError> {
	if assume_yes {
		return Ok(true);
	}

	if !std::io::stdin().is_terminal() {
		return Err(LauncherError::new(
			ErrorCode::ConfirmationRequired,
			"This needs your confirmation, and there is no terminal to ask on. Run it again with --yes to confirm.",
		));
	}

	eprint!("{question} [y/N] ");
	let _ = std::io::stderr().flush();

	let mut answer = String::new();
	std::io::stdin()
		.lock()
		.read_line(&mut answer)
		.map_err(|error| {
			LauncherError::new(
				ErrorCode::ConfirmationRequired,
				"The answer could not be read.",
			)
			.with_source(error)
		})?;

	Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}
