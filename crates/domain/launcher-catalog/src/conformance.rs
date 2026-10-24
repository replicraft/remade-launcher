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
