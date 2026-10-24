//! Runs the real binary against an empty data folder.
//!
//! The production build trusts no catalog keys yet, so these tests cover
//! everything that must work, and fail clearly, without a verified catalog.
//! Catalog, install, and update behavior is tested in `launcher-core` with
//! fixture keys, which a release binary cannot be told to trust.

#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

struct Run {
	_data: tempfile::TempDir,
	data_path: std::path::PathBuf,
}

fn run() -> Run {
	let data = tempfile::tempdir().unwrap();
	let data_path = data.path().join("data");

	Run {
		_data: data,
		data_path,
	}
}

impl Run {
	fn remade(&self, arguments: &[&str]) -> Output {
		Command::new(env!("CARGO_BIN_EXE_remade"))
			.args(arguments)
			.env("REMADE_DATA_DIR", &self.data_path)
			.output()
			.unwrap()
	}
}

fn text(bytes: &[u8]) -> String {
	String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn help_lists_every_command() {
	let output = run().remade(&["--help"]);
	let help = text(&output.stdout);

	assert!(output.status.success());

	for command in [
		"search",
		"info",
		"list",
		"install",
		"update",
		"uninstall",
		"run",
		"catalog",
	] {
		assert!(help.contains(command), "help is missing {command}");
	}
}

#[test]
fn an_unknown_command_is_a_usage_error() {
	let output = run().remade(&["frobnicate"]);

	assert_eq!(output.status.code(), Some(2));
}

#[test]
fn list_is_empty_but_succeeds_and_json_is_valid() {
	let run = run();

	let plain = run.remade(&["list"]);
	assert!(plain.status.success());
	assert!(text(&plain.stderr).contains("Nothing is installed"));

	let json = run.remade(&["--json", "list"]);
	assert!(json.status.success());
	assert_eq!(
		serde_json::from_slice::<serde_json::Value>(&json.stdout).unwrap(),
		serde_json::json!([])
	);
}

#[test]
fn searching_without_a_verified_catalog_says_why_instead_of_guessing() {
	let output = run().remade(&["--offline", "search", "editor"]);

	assert!(!output.status.success());
	assert!(
		text(&output.stderr).contains("no saved catalog"),
		"{}",
		text(&output.stderr)
	);
	assert!(
		text(&output.stdout).is_empty(),
		"nothing is printed as a result"
	);
}

#[test]
fn the_production_build_explains_it_has_no_signing_keys() {
	let output = run().remade(&["search", "editor"]);
	let stderr = text(&output.stderr);

	assert!(!output.status.success());
	assert!(stderr.contains("no catalog signing keys"), "{stderr}");
}

#[test]
fn catalog_status_reports_no_catalog_and_fails() {
	let output = run().remade(&["--json", "catalog", "status"]);
	let status: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

	assert!(!output.status.success());
	assert_eq!(status["available"], false);
}

#[test]
fn installing_offline_is_refused_before_anything_happens() {
	let run = run();
	let output = run.remade(&["--offline", "install", "sample-editor", "--yes"]);

	assert!(!output.status.success());
	assert!(text(&output.stderr).contains("needs the network"));
	assert!(!run.data_path.join("apps").exists());
}

#[test]
fn installing_from_an_unreviewed_url_needs_a_real_github_link() {
	let output = run().remade(&[
		"install",
		"--from",
		"https://github.com.evil.example/o/r",
		"--yes",
		"--trust-unreviewed-source",
	]);

	assert!(!output.status.success());
	assert!(
		text(&output.stderr).contains("not a supported GitHub"),
		"{}",
		text(&output.stderr)
	);
}

#[test]
fn removing_or_running_something_that_is_not_installed_fails_clearly() {
	let run = run();

	for arguments in [["uninstall", "ghost"], ["run", "ghost"]] {
		let output = run.remade(&arguments);

		assert!(!output.status.success());
		assert!(
			text(&output.stderr).contains("No installed app matches"),
			"{arguments:?}"
		);
	}
}

#[test]
fn flags_that_only_make_sense_together_are_enforced() {
	let run = run();

	assert_eq!(run.remade(&["install"]).status.code(), Some(2));
	assert_eq!(
		run.remade(&["install", "x", "--from", "https://github.com/o/r"])
			.status
			.code(),
		Some(2)
	);
	assert_eq!(
		run.remade(&["install", "x", "--trust-unreviewed-source"])
			.status
			.code(),
		Some(2)
	);
}
