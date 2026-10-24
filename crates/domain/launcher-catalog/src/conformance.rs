//! Runs the shared conformance corpus from `tests/fixtures`.
//!
//! The same fixtures are meant for the website's publisher, so both sides
//! agree on what each file means. Test-only keys sign them, which is why this
//! module is compiled for tests alone.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::registry::KeyRegistry;
use crate::timestamp;
use crate::verify::{Change, FreshnessPolicy, HighWater, VerifyContext, verify};

fn fixtures() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn read(relative: &str) -> Vec<u8> {
	std::fs::read(fixtures().join(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

fn registry() -> KeyRegistry {
	let json = String::from_utf8(read("test-public-key-registry.json")).unwrap();

	KeyRegistry::from_test_fixture(&json).unwrap()
}

#[derive(Deserialize)]
struct Cases {
	cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
	id: String,
	catalog: String,
	signature: String,
	expected: String,
	context: CaseContext,
}

#[derive(Deserialize)]
struct CaseContext {
	feed_id: String,
	now: String,
	highest_revision: Option<u64>,
	highest_digest: Option<String>,
	supported_capabilities: Vec<String>,
}

fn run(case: &Case) -> Result<Change, String> {
	let registry = registry();
	let manifest = read(&case.catalog);
	let envelope = read(&case.signature);

	let supported: Vec<&str> = case
		.context
		.supported_capabilities
		.iter()
		.map(String::as_str)
		.collect();
	let high_water = case.context.highest_revision.map(|revision| HighWater {
		revision,
		digest: case
			.context
			.highest_digest
			.as_ref()
			.map(|hex| hex::decode(hex).unwrap().try_into().unwrap()),
	});

	let context = VerifyContext {
		registry: &registry,
		feed_id: &case.context.feed_id,
		now: timestamp::parse(&case.context.now).unwrap(),
		high_water,
		supported_capabilities: &supported,
		freshness: FreshnessPolicy::RequireCurrent,
	};

	verify(&manifest, &envelope, &context)
		.map(|outcome| outcome.change)
		.map_err(|error| error.kind().as_str().to_owned())
}

#[test]
fn every_case_in_the_corpus_behaves_as_documented() {
	let cases: Cases = serde_json::from_slice(&read("cases.json")).unwrap();

	assert!(cases.cases.len() >= 26, "the corpus shrank");

	let mut failures = Vec::new();

	for case in &cases.cases {
		let outcome = run(case);
		let actual = match &outcome {
			Ok(_) => "accept",
			Err(kind) => kind.as_str(),
		};

		if actual != case.expected {
			failures.push(format!(
				"{}: expected {}, got {actual}",
				case.id, case.expected
			));
		}
	}

	assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn an_identical_revision_is_reported_as_unchanged() {
	let cases: Cases = serde_json::from_slice(&read("cases.json")).unwrap();
	let case = cases
		.cases
		.iter()
		.find(|case| case.id == "valid-identical-revision")
		.unwrap();

	assert_eq!(run(case), Ok(Change::Unchanged));
}

#[test]
fn a_newer_revision_is_reported_as_new() {
	let cases: Cases = serde_json::from_slice(&read("cases.json")).unwrap();
	let case = cases
		.cases
		.iter()
		.find(|case| case.id == "valid-new-revision")
		.unwrap();

	assert_eq!(run(case), Ok(Change::New));
}

#[test]
fn production_trust_rejects_every_fixture_even_though_signatures_are_valid() {
	let production = KeyRegistry::production();
	let manifest = read("valid/launcher-catalog.json");
	let envelope = read("valid/launcher-catalog.sig");

	let context = VerifyContext {
		registry: &production,
		feed_id: "test.remade.launcher-catalog",
		now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
		high_water: None,
		supported_capabilities: &[],
		freshness: FreshnessPolicy::RequireCurrent,
	};

	let error = verify(&manifest, &envelope, &context).err().unwrap();

	assert_eq!(error.kind().as_str(), "no-trusted-signature");
}

#[test]
fn an_expired_cache_can_still_be_reverified_for_browsing() {
	let registry = registry();
	let manifest = read("valid/launcher-catalog.json");
	let envelope = read("valid/launcher-catalog.sig");

	let context = VerifyContext {
		registry: &registry,
		feed_id: "test.remade.launcher-catalog",
		now: timestamp::parse("2031-06-01T00:00:00Z").unwrap(),
		high_water: None,
		supported_capabilities: &[],
		freshness: FreshnessPolicy::AllowExpired,
	};

	let outcome = verify(&manifest, &envelope, &context).unwrap();

	assert_eq!(
		outcome.catalog.freshness(context.now),
		crate::Freshness::Expired
	);
	assert_eq!(outcome.catalog.project_count(), 1);
}

#[test]
fn manifest_digest_matches_the_whole_byte_digest() {
	let cases: Cases = serde_json::from_slice(&read("cases.json")).unwrap();
	let case = cases
		.cases
		.iter()
		.find(|case| case.id == "valid-identical-revision")
		.unwrap();

	let expected = hex::encode(Sha256::digest(read(&case.catalog)));

	assert_eq!(Some(expected), case.context.highest_digest);
}

#[derive(Deserialize)]
struct Vectors {
	vectors: Vec<Vector>,
}

#[derive(Deserialize)]
struct Vector {
	id: String,
	input: serde_json::Value,
	canonical_utf8: String,
	sha256: String,
}

#[test]
fn canonicalization_matches_the_shared_vectors() {
	let vectors: Vectors = serde_json::from_slice(&read("canonicalization-vectors.json")).unwrap();

	for vector in vectors.vectors {
		let canonical = serde_json_canonicalizer::to_vec(&vector.input).unwrap();

		assert_eq!(
			String::from_utf8(canonical.clone()).unwrap(),
			vector.canonical_utf8,
			"{}",
			vector.id
		);
		assert_eq!(
			hex::encode(Sha256::digest(canonical)),
			vector.sha256,
			"{}",
			vector.id
		);
	}
}

/// Measures verification of a maximum-size catalog. Run it in release mode:
///
/// ```sh
/// cargo test --release -p launcher-catalog catalog_of_ten_thousand -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a timing measurement, not a correctness check"]
fn verifying_a_catalog_of_ten_thousand_projects_is_measured() {
	use base64::Engine;
	use base64::engine::general_purpose::STANDARD;
	use ed25519_dalek::{Signer, SigningKey};

	let signing = SigningKey::from_bytes(&[7u8; 32]);
	let public = STANDARD.encode(signing.verifying_key().to_bytes());
	let registry = KeyRegistry::from_test_fixture(&format!(
		r#"{{"schema_version":1,"feed_id":"perf.feed","test_only":true,"keys":[
			{{"key_id":"perf","algorithm":"ed25519","role":"catalog","status":"active","public_key":"{public}"}}]}}"#
	))
	.unwrap();

	let projects: serde_json::Map<String, serde_json::Value> = (0..10_000)
		.map(|index| {
			let variant = |os: &str, arch: &str| {
				serde_json::json!({
					"id": format!("{os}-{arch}"), "os": os, "arch": arch, "abi": "native", "runtimes": [],
					"format": "tar.gz",
					"assets": {"include": [format!("app{index}-*-{os}-{arch}.tar.gz")], "exclude": ["*debug*"], "case_sensitive": true},
					"install": {"adapter": "portable-v1", "scope": "user", "entry_point": "bin/app", "required_capabilities": ["safe-archive-v1"]},
					"verification": {"minimum": "host-digest"},
					"required_capabilities": []
				})
			};

			(
				format!("app-{index}"),
				serde_json::json!({
					"display": {"name": format!("Application {index}"), "summary": "A synthetic catalog entry used to measure verification.",
						"kind": "desktop", "categories": ["utilities"], "links": {}},
					"source": {"provider": "github", "repository_id": (index + 1).to_string(), "owner_id": "2",
						"owner": "owner", "repository": format!("app-{index}")},
					"status": {"state": "active"},
					"release_policy": {"channel": "stable", "version_strategy": "semver", "tag_include": ["v*"],
						"tag_exclude": [], "tracking": "upstream"},
					"variants": [variant("linux", "x86_64"), variant("macos", "aarch64"), variant("windows", "x86_64")]
				}),
			)
		})
		.collect();

	let projects = serde_json::Value::Object(projects);
	let hash = hex::encode(Sha256::digest(
		serde_json_canonicalizer::to_vec(&projects).unwrap(),
	));
	let manifest = serde_json::to_vec(&serde_json::json!({
		"schema_version": 1, "feed_id": "perf.feed", "revision": 1,
		"issued_at": "2030-01-01T00:00:00Z", "expires_at": "2030-01-31T00:00:00Z",
		"required_capabilities": [], "projects": projects, "hash": hash
	}))
	.unwrap();
	let signature = STANDARD.encode(signing.sign(&manifest).to_bytes());
	let envelope = format!(
		r#"{{"version":1,"signatures":[{{"key_id":"perf","algorithm":"ed25519","signature":"{signature}"}}]}}"#
	);

	let context = VerifyContext {
		registry: &registry,
		feed_id: "perf.feed",
		now: timestamp::parse("2030-01-15T00:00:00Z").unwrap(),
		high_water: None,
		supported_capabilities: &["safe-archive-v1"],
		freshness: FreshnessPolicy::RequireCurrent,
	};

	// Where the time goes, stage by stage.
	{
		use serde::Deserialize;

		let stage = |label: &str, started: std::time::Instant| {
			println!(
				"  {label}: {:.0} ms",
				started.elapsed().as_secs_f64() * 1000.0
			);
		};

		let t = std::time::Instant::now();
		let value = crate::strict_json::parse(&manifest).unwrap();
		stage("strict parse to a value tree", t);

		let t = std::time::Instant::now();
		let typed = crate::manifest::Manifest::deserialize(&value).unwrap();
		stage("typed deserialize", t);

		let t = std::time::Instant::now();
		crate::semantic::validate(&typed).unwrap();
		stage("semantic validation (parallel)", t);

		let t = std::time::Instant::now();
		let canonical = crate::verify::canonical_projects(value.get("projects").unwrap()).unwrap();
		stage("canonical form of projects (parallel)", t);

		let t = std::time::Instant::now();
		let _ = Sha256::digest(&canonical);
		let _ = Sha256::digest(&manifest);
		stage("two SHA-256 passes", t);
	}

	let started = std::time::Instant::now();
	let outcome = verify(&manifest, envelope.as_bytes(), &context).unwrap();
	let elapsed = started.elapsed();

	println!(
		"verified {} projects ({:.1} MiB) in {:.0} ms on {} threads",
		outcome.catalog.project_count(),
		manifest.len() as f64 / (1024.0 * 1024.0),
		elapsed.as_secs_f64() * 1000.0,
		rayon::current_num_threads(),
	);

	assert_eq!(outcome.catalog.project_count(), 10_000);
}
