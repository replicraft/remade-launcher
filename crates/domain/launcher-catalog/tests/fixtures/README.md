# Test-only contract fixtures

These fixtures are synthetic and must never be trusted or published as the production Remade catalog. Repository names, IDs, versions and paths are illustrative. All signature private keys were ephemeral during generation and are absent from the bundle. Only public test keys and signed bytes are included.

## Layout

`valid/launcher-catalog.json` and `.sig` are a matching positive pair. Additional signature envelopes test dual-key overlap, an unknown key alongside a valid signature, and an invalid signature alongside a different valid trusted signature. `empty-catalog.json` tests an empty approved-project map. `installation-receipt.json` is a synthetic local-state example.

`cases.json` supplies expected outcomes and a fixed verification context for 26 cases. `cases/` includes each exact payload and envelope. Some negative payloads are deliberately validly signed so parsing, policy, or semantic failure can be tested after authentication. Never treat a mathematically valid test signature as permission to use these data in production.

All temporal tests use **2030-01-15T00:00:00Z**, not the machine's wall clock. Fixture issuance/expiration are artificial test values, not a planned release date. The test feed is `test.remade.launcher-catalog`, deliberately distinct from production.

## Canonicalization scope

`canonicalization-vectors.json` covers empty objects, key ordering, Unicode UTF-16 ordering, JSON escapes, non-normalization of Unicode, nested data and interoperable integer boundaries. It uses a JCS-compatible integer/string subset with no floating-point values. The included fixture corpus is not the entire RFC 8785 conformance suite. Production implementations must use a reviewed full implementation and standard conformance vectors where their data types permit more cases.

The pack generation compared those vectors and the catalog to an independent Node serialization/order implementation. It also verified a positive Ed25519 signature with OpenSSL independently of the Python cryptography API. The audit reruns positive/negative verification using only the public registry; these checks are evidence about the files, not the future Rust verifier.

## Expected consumer behavior

Enforce envelope limits and unique IDs, then verify exact bytes with locally trusted keys, then strict parse/schema/semantic/hash/freshness/revision checks. Unknown well-formed keys are ignored if another active trusted signature validates. No known valid signature means rejection. A malformed envelope is not rescued by one good signature. A valid signature over bad policy does not make the policy acceptable.

The same-revision cases use the whole-manifest byte digest, not just the projects hash. The invalid-newline case shows why detached signatures cannot be verified over a reserialized object. Empty projects is a legitimate signed empty catalog, not a signal to admit every project.

## Acceptance evidence boundary

The generated [contract validation results](../data/contract-validation-results.json) record the pack checks. Actual native-client conformance, persistence interruption, resource exhaustion, helper trust, and OS installation tests remain planned in the test strategy. Public builds must reject these keys regardless of whether the fixture signatures verify.
