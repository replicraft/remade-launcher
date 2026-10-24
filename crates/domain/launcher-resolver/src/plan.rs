//! Immutable installation plans and approval.
//!
//! A [`Plan`] freezes one decision: which repository, which release, which
//! exact asset, which adapter, with what evidence. Its digest covers all of
//! it, so changing any field produces a different plan and invalidates
//! approval. [`ApprovedPlan`] can only be built from a plan plus a matching
//! [`Consent`], which is how the interface proves the user saw the source.

use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::resolve::Candidate;

pub const PLAN_SCHEMA_VERSION: u32 = 1;

/// How long an unapproved plan stays valid.
pub const PLAN_LIFETIME_SECONDS: i64 = 15 * 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
	Install,
	Update,
	Uninstall,
}

/// Where the authority to install came from. The two paths are separate and
/// neither can turn into the other.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Origin {
	/// Allowed by a verified Remade catalog.
	Curated {
		feed_id: String,
		revision: u64,
		catalog_digest: String,
		slug: String,
	},
	/// A repository the user added themselves. Not reviewed by Remade, and
	/// matching a curated name does not make it curated.
	Custom,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceBinding {
	pub repository_id: u64,
	pub owner_id: u64,
	/// `owner/repository`, shown beside the approve button.
	pub display: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ReleaseBinding {
	pub id: u64,
	pub tag: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AssetBinding {
	pub id: u64,
	pub name: String,
	pub size: u64,
	pub download_url: String,
}

/// The strongest evidence the package will be checked against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceLevel {
	/// The digest GitHub reports for the file. Proves the bytes match what
	/// the host holds, not who made them.
	HostDigest,
	/// A SHA-256 pinned in the signed catalog.
	PinnedSha256,
	/// The publisher's own signature.
	PublisherSignature,
	/// Bytes observed locally for a user-added source. This is never
	/// publisher verification.
	LocalObserved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InstallBinding {
	pub variant_id: String,
	pub adapter: String,
	pub scope: String,
	pub format: String,
	pub entry_point: Option<String>,
	pub strip_components: u8,
	pub evidence: EvidenceLevel,
	/// Lowercase hex SHA-256 the download must equal, when one is known
	/// before downloading.
	pub expected_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanBody {
	pub schema_version: u32,
	pub operation: Operation,
	pub origin: Origin,
	pub source: SourceBinding,
	pub release: ReleaseBinding,
	pub asset: AssetBinding,
	pub install: InstallBinding,
	/// Unix seconds.
	pub created_at: i64,
	pub expires_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
	body: PlanBody,
	digest: [u8; 32],
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PlanError {
	#[error("the plan expired before it was approved")]
	Expired,
	#[error("the source shown to the user does not match the plan")]
	SourceMismatch,
	#[error("a source Remade has not reviewed needs explicit acknowledgement")]
	UnreviewedNotAcknowledged,
	#[error("the plan could not be serialized")]
	Unserializable,
}

impl Plan {
	/// Seals `body`, computing its digest over the RFC 8785 canonical form.
	pub fn seal(body: PlanBody) -> Result<Self, PlanError> {
		let value = serde_json::to_value(&body).map_err(|_| PlanError::Unserializable)?;
		let canonical =
			serde_json_canonicalizer::to_vec(&value).map_err(|_| PlanError::Unserializable)?;

		Ok(Self {
			digest: Sha256::digest(canonical).into(),
			body,
		})
	}

	/// A plan for `candidate`, from whichever authority `origin` names.
	pub fn for_candidate(
		operation: Operation,
		origin: Origin,
		source: SourceBinding,
		candidate: &Candidate,
		install: InstallBinding,
		now: i64,
	) -> Result<Self, PlanError> {
		Self::seal(PlanBody {
			schema_version: PLAN_SCHEMA_VERSION,
			operation,
			origin,
			source,
			release: ReleaseBinding {
				id: candidate.release.id,
				tag: candidate.release.tag.clone(),
			},
			asset: AssetBinding {
				id: candidate.asset.id,
				name: candidate.asset.name.clone(),
				size: candidate.asset.size,
				download_url: candidate.asset.download_url.clone(),
			},
			install,
			created_at: now,
			expires_at: now + PLAN_LIFETIME_SECONDS,
		})
	}

	pub fn body(&self) -> &PlanBody {
		&self.body
	}

	pub fn digest(&self) -> [u8; 32] {
		self.digest
	}

	pub fn digest_hex(&self) -> String {
		hex::encode(self.digest)
	}

	/// Binds a user's consent to this exact plan.
	pub fn approve(self, consent: &Consent, now: i64) -> Result<ApprovedPlan, PlanError> {
		if now >= self.body.expires_at {
			return Err(PlanError::Expired);
		}

		if consent.source_shown != self.body.source.display {
			return Err(PlanError::SourceMismatch);
		}

		if self.body.origin == Origin::Custom && !consent.acknowledged_unreviewed {
			return Err(PlanError::UnreviewedNotAcknowledged);
		}

		Ok(ApprovedPlan {
			digest: self.digest,
			plan: self,
			approved_at: now,
		})
	}
}

/// What the interface reports having shown the user.
#[derive(Clone, Debug)]
pub struct Consent {
	/// The `owner/repository` text displayed beside the approve control.
	pub source_shown: String,
	/// The user confirmed the source is not reviewed by Remade.
	pub acknowledged_unreviewed: bool,
}

#[derive(Clone, Debug)]
pub struct ApprovedPlan {
	plan: Plan,
	digest: [u8; 32],
	approved_at: i64,
}

impl ApprovedPlan {
	pub fn plan(&self) -> &Plan {
		&self.plan
	}

	pub fn approved_at(&self) -> i64 {
		self.approved_at
	}

	/// True only for the identical plan. A refreshed observation that changes
	/// the asset, destination, adapter, or evidence produces a new digest.
	pub fn covers(&self, plan: &Plan) -> bool {
		self.digest == plan.digest
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn body() -> PlanBody {
		PlanBody {
			schema_version: PLAN_SCHEMA_VERSION,
			operation: Operation::Install,
			origin: Origin::Custom,
			source: SourceBinding {
				repository_id: 1,
				owner_id: 2,
				display: "owner/repo".into(),
			},
			release: ReleaseBinding {
				id: 10,
				tag: "v1".into(),
			},
			asset: AssetBinding {
				id: 100,
				name: "app.zip".into(),
				size: 5,
				download_url: "https://github.com/owner/repo/releases/download/v1/app.zip".into(),
			},
			install: InstallBinding {
				variant_id: "custom".into(),
				adapter: "portable-v1".into(),
				scope: "user".into(),
				format: "zip".into(),
				entry_point: None,
				strip_components: 0,
				evidence: EvidenceLevel::LocalObserved,
				expected_sha256: None,
			},
			created_at: 1_000,
			expires_at: 1_000 + PLAN_LIFETIME_SECONDS,
		}
	}

	fn consent(acknowledged: bool) -> Consent {
		Consent {
			source_shown: "owner/repo".into(),
			acknowledged_unreviewed: acknowledged,
		}
	}

	#[test]
	fn the_digest_is_stable_and_covers_every_field() {
		let plan = Plan::seal(body()).unwrap();

		assert_eq!(plan.digest(), Plan::seal(body()).unwrap().digest());

		let mut changed = body();
		changed.asset.id = 101;
		assert_ne!(plan.digest(), Plan::seal(changed).unwrap().digest());

		let mut changed = body();
		changed.install.adapter = "other".into();
		assert_ne!(plan.digest(), Plan::seal(changed).unwrap().digest());

		let mut changed = body();
		changed.install.evidence = EvidenceLevel::HostDigest;
		assert_ne!(plan.digest(), Plan::seal(changed).unwrap().digest());
	}

	#[test]
	fn an_unreviewed_source_needs_acknowledgement() {
		let plan = Plan::seal(body()).unwrap();

		assert_eq!(
			plan.clone().approve(&consent(false), 1_001).unwrap_err(),
			PlanError::UnreviewedNotAcknowledged
		);
		assert!(plan.approve(&consent(true), 1_001).is_ok());
	}

	#[test]
	fn approval_requires_the_source_the_user_saw() {
		let plan = Plan::seal(body()).unwrap();
		let wrong = Consent {
			source_shown: "someone/else".into(),
			acknowledged_unreviewed: true,
		};

		assert_eq!(
			plan.approve(&wrong, 1_001).unwrap_err(),
			PlanError::SourceMismatch
		);
	}

	#[test]
	fn an_expired_plan_cannot_be_approved() {
		let plan = Plan::seal(body()).unwrap();

		assert_eq!(
			plan.approve(&consent(true), 1_000 + PLAN_LIFETIME_SECONDS)
				.unwrap_err(),
			PlanError::Expired
		);
	}

	#[test]
	fn approval_covers_only_the_identical_plan() {
		let plan = Plan::seal(body()).unwrap();
		let approved = plan.clone().approve(&consent(true), 1_001).unwrap();

		assert!(approved.covers(&plan));

		let mut swapped = body();
		swapped.asset.download_url =
			"https://github.com/owner/repo/releases/download/v1/evil.zip".into();

		assert!(!approved.covers(&Plan::seal(swapped).unwrap()));
	}
}
