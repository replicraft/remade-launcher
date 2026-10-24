//! Runs the real binary against an empty data folder.
//!
//! The production build trusts no catalog keys yet, so these tests cover
//! everything that must work, and fail clearly, without a verified catalog.
//! Catalog, install, update, and automatic-update behavior is tested in
//! `launcher-core` with fixture keys, which a release binary cannot be told
//! to trust.
//!
//! Nothing here registers a login item or touches the user's real launcher
//! data: every run gets its own folder.

#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

use serde_json::Value;

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
			.env_remove("NO_COLOR")
			.output()
			.unwrap()
	}

	fn json(&self, arguments: &[&str]) -> (i32, Value, String) {
		let mut full = vec!["--json"];
		full.extend_from_slice(arguments);

		let output = self.remade(&full);
		let stdout = String::from_utf8_lossy(&output.stdout);
		let value: Value = serde_json::from_str(stdout.trim())
			.unwrap_or_else(|error| panic!("not one JSON object: {error}\n{stdout}"));

		(
			output.status.code().unwrap(),
			value,
			String::from_utf8_lossy(&output.stderr).into_owned(),
		)
	}
}

fn text(bytes: &[u8]) -> String {
	String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn help_lists_the_commands_of_the_contract() {
	let help = text(&run().remade(&["--help"]).stdout);

	for command in [
		"search",
		"explore",
		"info",
		"platforms",
		"releases",
		"changelog",
		"catalog",
		"trust",
		"list",
		"outdated",
		"check",
		"history",
		"operations",
		"plan",
		"apply",
		"install",
		"update",
		"uninstall",
		"launch",
		"verify",
		"doctor",
		"forget",
		"pin",
		"unpin",
		"pins",
		"path",
		"updates",
		"daemon",
		"config",
		"self-update",
		"cache",
		"completions",
		"about",
	] {
		assert!(help.contains(command), "help is missing {command}");
	}
}

#[test]
fn usage_mistakes_exit_with_the_usage_status() {
	let run = run();

	for arguments in [
		&["frobnicate"][..],
		&["install"],
		&["--json", "--output", "ndjson", "list"],
		&["updates", "age", "set"],
		&["install", "x", "--accept-unreviewed-source"],
	] {
		assert_eq!(
			run.remade(arguments).status.code(),
			Some(2),
			"{arguments:?}"
		);
	}
}

#[test]
fn json_results_are_one_versioned_object() {
	let (code, value, _) = run().json(&["list"]);

	assert_eq!(code, 0);
	assert_eq!(value["schema_version"], 1);
	assert_eq!(value["command"], "list");
	assert_eq!(value["ok"], true);
	assert_eq!(value["data"]["managed"], serde_json::json!([]));
	assert_eq!(value["warnings"], serde_json::json!([]));
}

#[test]
fn json_errors_are_one_typed_object_and_humans_get_stderr() {
	let (code, value, stderr) = run().json(&["search", "editor"]);

	assert_eq!(code, 6, "no trusted keys is a trust failure");
	assert_eq!(value["ok"], false);
	assert_eq!(value["error"]["code"], "CATALOG_KEYS_NOT_CONFIGURED");
	assert_eq!(value["error"]["exit_status"], 6);
	assert!(stderr.contains("no catalog signing keys"), "{stderr}");
	assert!(value.get("data").is_none(), "an error carries no results");
}

#[test]
fn offline_commands_say_so_and_do_nothing() {
	let run = run();

	let (code, value, _) = run.json(&["--offline", "search", "editor"]);
	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(5, Some("CATALOG_UNAVAILABLE_OFFLINE"))
	);

	for arguments in [
		&["--offline", "install", "sample-editor", "--yes"][..],
		&["--offline", "outdated"],
		&["--offline", "releases", "x"],
	] {
		let (code, value, _) = run.json(arguments);

		assert_eq!(
			(code, value["error"]["code"].as_str()),
			(5, Some("OFFLINE")),
			"{arguments:?}"
		);
	}

	assert!(!run.data_path.join("apps").exists());
}

#[test]
fn a_bad_source_link_is_refused_before_any_request() {
	for url in [
		"https://github.com.evil.example/o/r",
		"http://github.com/o/r",
		"https://github.com/o",
	] {
		let (code, value, _) = run().json(&["install", url, "--yes"]);

		assert_eq!(
			(code, value["error"]["code"].as_str()),
			(2, Some("INVALID_SOURCE_URL")),
			"{url}"
		);
	}
}

#[test]
fn things_that_are_not_installed_fail_with_a_typed_error() {
	let run = run();

	for arguments in [
		&["uninstall", "ghost", "--yes"][..],
		&["launch", "ghost"],
		&["verify", "ghost"],
		&["path", "ghost"],
		&["pin", "ghost"],
		&["forget", "ghost", "--yes"],
	] {
		let (code, value, _) = run.json(arguments);

		assert_eq!(
			(code, value["error"]["code"].as_str()),
			(2, Some("INSTALLATION_NOT_FOUND")),
			"{arguments:?}"
		);
	}
}

#[test]
fn run_is_an_alias_for_launch() {
	let (code, value, _) = run().json(&["run", "ghost"]);

	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(2, Some("INSTALLATION_NOT_FOUND"))
	);
}

#[test]
fn plans_that_do_not_exist_cannot_be_shown_or_applied() {
	let run = run();

	for arguments in [
		&["plan", "show", "deadbeef"][..],
		&["apply", "deadbeef", "--yes"],
	] {
		let (code, value, _) = run.json(arguments);

		assert_eq!(
			(code, value["error"]["code"].as_str()),
			(7, Some("PLAN_CHANGED")),
			"{arguments:?}"
		);
	}
}

#[test]
fn history_and_operations_start_empty() {
	let run = run();

	assert_eq!(
		run.json(&["history"]).1["data"]["operations"],
		serde_json::json!([])
	);
	assert_eq!(
		run.json(&["operations", "list"]).1["data"]["operations"],
		serde_json::json!([])
	);

	let (code, value, _) = run.json(&["operations", "cancel", "x"]);
	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(4, Some("CANCEL_UNSUPPORTED"))
	);

	let (code, value, _) = run.json(&["operations", "recover"]);
	assert_eq!(
		(code, value["data"]["needs_attention"].as_u64()),
		(0, Some(0))
	);
}

#[test]
fn update_mode_defaults_to_manual_and_changing_it_needs_an_acknowledgement() {
	let run = run();

	let (_, value, _) = run.json(&["updates", "mode"]);
	assert_eq!(value["data"]["update_mode"], "manual");
	assert_eq!(value["data"]["minimum_release_age_seconds"], 3600);

	// Neither --no-input nor a missing terminal can answer for the user.
	for arguments in [
		&["--no-input", "updates", "mode", "set", "on-launch"][..],
		&["updates", "mode", "set", "daemon"],
	] {
		let (code, value, _) = run.json(arguments);

		assert_eq!(
			(code, value["error"]["code"].as_str()),
			(3, Some("CONSENT_REQUIRED")),
			"{arguments:?}"
		);
	}

	assert_eq!(
		run.json(&["updates", "mode"]).1["data"]["update_mode"],
		"manual"
	);
}

#[test]
fn on_launch_mode_can_be_chosen_with_a_recorded_acknowledgement_and_starts_nothing() {
	let run = run();

	let (code, value, _) = run.json(&["updates", "mode", "set", "on-launch", "--accept-risks"]);

	assert_eq!(code, 0);
	assert_eq!(value["data"]["update_mode"], "on-launch");
	assert_eq!(value["data"]["started_an_update"], false);
	assert_eq!(
		run.json(&["updates", "mode"]).1["data"]["update_mode"],
		"on-launch"
	);

	let (code, value, _) = run.json(&["updates", "mode", "set", "manual"]);
	assert_eq!(
		(code, value["data"]["update_mode"].as_str()),
		(0, Some("manual"))
	);
}

#[test]
fn the_minimum_release_age_is_saved_in_seconds_and_validated() {
	let run = run();

	let (code, value, _) = run.json(&["updates", "age", "set", "24h"]);
	assert_eq!(
		(code, value["data"]["minimum_release_age_seconds"].as_u64()),
		(0, Some(86_400))
	);
	assert_eq!(
		run.json(&["updates", "age"]).1["data"]["minimum_release_age_seconds"],
		86_400
	);

	assert_eq!(
		run.json(&["updates", "age", "set", "0"]).1["data"]["minimum_release_age_seconds"],
		0
	);

	for bad in ["1w", "", "31d", "-1h", "abc"] {
		let (code, value, _) = run.json(&["updates", "age", "set", bad]);

		assert_eq!(
			(code, value["error"]["code"].as_str()),
			(2, Some("USAGE")),
			"{bad:?}"
		);
	}
}

#[test]
fn the_background_updater_reports_its_true_state_and_is_off_by_default() {
	let (code, value, _) = run().json(&["daemon", "status"]);
	let data = &value["data"];

	assert_eq!(code, 0);
	assert_eq!(data["desired_state"], "disabled");
	assert_eq!(data["check_interval_seconds"], 86_400);
	assert!(
		[
			"not_installed",
			"unsupported",
			"registered_not_running",
			"failed"
		]
		.contains(&data["actual_state"].as_str().unwrap()),
		"{data}"
	);
	assert_ne!(data["actual_state"], "running");
}

#[test]
fn registering_the_updater_without_consent_changes_nothing() {
	let run = run();

	let (code, value, _) = run.json(&["daemon", "register"]);

	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(3, Some("CONSENT_REQUIRED"))
	);
	assert_eq!(
		run.json(&["updates", "mode"]).1["data"]["update_mode"],
		"manual"
	);
}

#[test]
fn check_now_without_a_running_updater_is_only_a_look() {
	let (code, value, _) = run().json(&["--offline", "daemon", "check-now"]);

	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(5, Some("OFFLINE"))
	);
}

#[test]
fn settings_can_be_read_and_changed_through_config() {
	let run = run();

	assert_eq!(
		run.json(&["config", "get", "theme"]).1["data"]["value"],
		"system"
	);

	run.json(&["config", "set", "theme", "dark"]);
	assert_eq!(
		run.json(&["config", "get", "theme"]).1["data"]["value"],
		"dark"
	);

	let (code, value, _) = run.json(&["config", "set", "update.mode", "daemon"]);
	assert_eq!((code, value["error"]["code"].as_str()), (2, Some("USAGE")));

	let (code, _, _) = run.json(&["config", "get", "nothing"]);
	assert_eq!(code, 2);

	assert_eq!(
		run.json(&["config", "list"]).1["data"]["settings"]
			.as_array()
			.unwrap()
			.len(),
		3
	);
}

#[test]
fn the_launcher_cannot_update_itself_yet_and_says_so() {
	let run = run();

	let (code, value, _) = run.json(&["self-update", "check"]);
	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(4, Some("SELF_UPDATE_UNAVAILABLE"))
	);

	let (code, value, _) = run.json(&["self-update", "status"]);
	assert_eq!(
		(code, value["data"]["available"].as_bool()),
		(0, Some(false))
	);
}

#[test]
fn trust_status_lists_no_keys_in_this_build() {
	let (code, value, _) = run().json(&["trust", "status"]);

	assert_eq!(code, 0);
	assert_eq!(value["data"]["feed_id"], "wiki.remade.launcher-catalog");
	assert_eq!(value["data"]["trusted_key_ids"], serde_json::json!([]));
}

#[test]
fn doctor_cache_pins_and_diagnostics_work_on_a_fresh_install() {
	let run = run();

	let (code, value, _) = run.json(&["doctor"]);
	assert_eq!((code, value["data"]["healthy"].as_bool()), (0, Some(true)));

	assert_eq!(
		run.json(&["cache", "status"]).1["data"]["leftover_folders"],
		0
	);
	assert_eq!(
		run.json(&["pins"]).1["data"]["pinned"],
		serde_json::json!([])
	);
	assert!(
		run.json(&["diagnostics"]).1["data"]["report"]
			.as_str()
			.unwrap()
			.contains("Installed apps: 0")
	);
}

#[test]
fn about_and_completions_need_no_catalog() {
	let run = run();

	let (_, value, _) = run.json(&["about"]);
	assert_eq!(value["data"]["name"], "Remade Launcher");
	assert!(value["data"]["os"].is_string());

	let output = run.remade(&["completions", "bash"]);
	assert!(output.status.success());
	assert!(text(&output.stdout).contains("remade"));
}

#[test]
fn ndjson_commands_that_do_not_stream_still_print_one_line() {
	let output = run().remade(&["--output", "ndjson", "list"]);
	let out = text(&output.stdout);

	assert_eq!(out.trim().lines().count(), 1);
	assert!(serde_json::from_str::<Value>(out.trim()).is_ok());
}

#[test]
fn quiet_mode_keeps_results_and_drops_chatter() {
	let output = run().remade(&["--quiet", "list"]);

	assert!(output.status.success());
	assert!(text(&output.stderr).is_empty(), "{}", text(&output.stderr));
}

#[test]
fn replanning_an_update_for_an_unknown_app_is_a_typed_failure() {
	let (code, value, _) = run().json(&["plan", "update", "ghost"]);

	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(2, Some("INSTALLATION_NOT_FOUND"))
	);
}

#[test]
fn repair_is_honestly_unavailable() {
	let (code, value, _) = run().json(&["plan", "repair", "ghost"]);

	assert_eq!(
		(code, value["error"]["code"].as_str()),
		(4, Some("REPAIR_UNSUPPORTED"))
	);
}
