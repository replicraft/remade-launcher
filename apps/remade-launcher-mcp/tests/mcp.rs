//! Talks to the real server over standard input and output, the way an
//! assistant's host does.

#![allow(clippy::unwrap_used)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use serde_json::{Value, json};

struct Session {
	child: Child,
	input: ChildStdin,
	output: Receiver<Value>,
	next_id: u64,
	_data: tempfile::TempDir,
}

impl Session {
	fn start(flags: &[&str]) -> Self {
		let data = tempfile::tempdir().unwrap();
		let mut child = Command::new(env!("CARGO_BIN_EXE_remade-launcher-mcp"))
			.args(flags)
			.env("REMADE_DATA_DIR", data.path().join("data"))
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.spawn()
			.unwrap();

		let input = child.stdin.take().unwrap();
		let stdout = child.stdout.take().unwrap();
		let (sender, output) = channel();

		std::thread::spawn(move || {
			for line in BufReader::new(stdout).lines().map_while(Result::ok) {
				if let Ok(value) = serde_json::from_str::<Value>(&line) {
					let _ = sender.send(value);
				}
			}
		});

		let mut session = Self {
			child,
			input,
			output,
			next_id: 0,
			_data: data,
		};
		let init = session.request(
			"initialize",
			json!({
				"protocolVersion": "2025-06-18",
				"capabilities": {},
				"clientInfo": { "name": "test", "version": "0" }
			}),
		);

		assert!(
			init["result"]["capabilities"]["tools"].is_object(),
			"{init}"
		);

		session.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

		session
	}

	fn send(&mut self, message: &Value) {
		writeln!(self.input, "{message}").unwrap();
		self.input.flush().unwrap();
	}

	fn request(&mut self, method: &str, params: Value) -> Value {
		self.next_id += 1;

		let id = self.next_id;

		self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));

		loop {
			let reply = self
				.output
				.recv_timeout(Duration::from_secs(20))
				.expect("the server answered");

			if reply["id"] == json!(id) {
				return reply;
			}
		}
	}

	fn call(&mut self, tool: &str, arguments: Value) -> Value {
		self.request(
			"tools/call",
			json!({ "name": tool, "arguments": arguments }),
		)
	}
}

impl Drop for Session {
	fn drop(&mut self) {
		let _ = self.child.kill();
	}
}

fn tool_names(session: &mut Session) -> Vec<String> {
	let listed = session.request("tools/list", json!({}));

	listed["result"]["tools"]
		.as_array()
		.unwrap()
		.iter()
		.map(|tool| tool["name"].as_str().unwrap().to_owned())
		.collect()
}

fn failed(reply: &Value) -> bool {
	reply["result"]["isError"] == json!(true) || reply["error"].is_object()
}

fn message(reply: &Value) -> String {
	reply["result"]["content"][0]["text"]
		.as_str()
		.map(str::to_owned)
		.unwrap_or_else(|| reply.to_string())
}

#[test]
fn the_tools_are_offered_in_three_tiers_and_applying_is_always_listed_but_guarded() {
	let mut session = Session::start(&[]);
	let names = tool_names(&mut session);

	for tool in [
		"search_catalog",
		"app_info",
		"list_installed",
		"check_updates",
		"changelog",
		"plan_install",
		"plan_update",
		"plan_repair",
		"plan_uninstall",
		"show_plan",
		"apply_plan",
	] {
		assert!(names.contains(&tool.to_owned()), "missing {tool}");
	}
}

#[test]
fn without_permission_an_assistant_cannot_apply_anything() {
	let mut session = Session::start(&[]);
	let reply = session.call(
		"apply_plan",
		json!({ "plan_id": "0000000000000000", "app": "x", "version": "v1" }),
	);

	assert!(failed(&reply), "{reply}");
	assert!(
		message(&reply).contains("without permission"),
		"{}",
		message(&reply)
	);
}

#[test]
fn reading_works_with_nothing_installed_and_unknown_things_are_clear_errors() {
	let mut session = Session::start(&[]);
	let listed = session.call("list_installed", json!({}));

	assert!(!failed(&listed), "{listed}");

	let value: Value = serde_json::from_str(&message(&listed)).unwrap();

	assert_eq!(value["managed"], json!([]));

	let unknown = session.call("plan_uninstall", json!({ "app": "ghost" }));

	assert!(failed(&unknown));
	assert!(
		message(&unknown).contains("No installed app"),
		"{}",
		message(&unknown)
	);

	let no_plan = session.call("show_plan", json!({ "plan_id": "feedfacefeedface" }));

	assert!(failed(&no_plan));
}

#[test]
fn even_with_permission_the_plan_must_exist_and_match() {
	let mut session = Session::start(&["--allow-apply"]);
	let reply = session.call(
		"apply_plan",
		json!({ "plan_id": "feedfacefeedface", "app": "x", "version": "v1" }),
	);

	assert!(failed(&reply), "{reply}");
	assert!(
		!message(&reply).contains("without permission"),
		"permission was given, so the plan is what is wrong"
	);
}

#[test]
fn bad_flags_stop_the_server_before_it_starts() {
	for flags in [vec!["--allow-destructive"], vec!["--bogus"]] {
		let output = Command::new(env!("CARGO_BIN_EXE_remade-launcher-mcp"))
			.args(&flags)
			.output()
			.unwrap();

		assert_eq!(output.status.code(), Some(2), "{flags:?}");
	}
}
