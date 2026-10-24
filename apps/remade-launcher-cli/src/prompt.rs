use std::io::{BufRead, IsTerminal, Write};

use crate::context::Ctx;
use crate::errors::CliError;

/// Asks a yes/no question. `assume_yes` answers it without asking.
///
/// `--no-input` never approves anything, and with no terminal to ask on the
/// answer is an error rather than a silent "yes": a script must say so with
/// `--yes`.
pub(crate) fn confirm(ctx: &Ctx, question: &str, assume_yes: bool) -> Result<bool, CliError> {
	if assume_yes {
		return Ok(true);
	}

	if ctx.no_input || !std::io::stdin().is_terminal() {
		return Err(CliError::approval(
			"This needs your confirmation, and there is no way to ask. Run it again with --yes to confirm.",
		));
	}

	eprint!("{question} [y/N] ");
	let _ = std::io::stderr().flush();

	let mut answer = String::new();

	std::io::stdin()
		.lock()
		.read_line(&mut answer)
		.map_err(|_| CliError::approval("The answer could not be read."))?;

	Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}
