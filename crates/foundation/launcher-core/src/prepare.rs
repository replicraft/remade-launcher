//! Turning a wish ("install this") into a plan a person can approve.
//!
//! Preparation reads the network and the verified catalog but changes
//! nothing on disk. The result is either a sealed [`Plan`] with an
//! explanation, a short list to choose from, or a plain-language reason that
//! nothing can be installed.

use launcher_catalog::{
	Freshness, InstallScope, PackageFormat, Project, Variant, VerificationLevel,
};
use launcher_model::{ErrorCode, HostTarget, InstallationId, LauncherError};
use launcher_providers::{
	GithubProvider, ReleaseObservation, RepositoryIdentity, Transport, parse_source_url,
};
use launcher_resolver::{
	Candidate, Consent, CustomOption, EvidenceLevel, InstallBinding, NoCompatible, Operation,
	Origin, Outcome, Plan, SourceBinding, custom_candidates, resolve_curated,
};

use time::OffsetDateTime;

use crate::engine::{Engine, now_unix};
use crate::lifecycle::InstallMeta;

/// A plan ready for the user to approve, with everything the screen needs.
#[derive(Clone, Debug)]
pub struct Ready {
	pub plan: Plan,
	pub candidate: Candidate,
	pub meta: InstallMeta,
	/// `owner/repository` to show beside the approve control.
	pub source_display: String,
	/// True for a source Remade has not reviewed.
	pub unreviewed: bool,
	/// Things the user should know that are not errors.
	pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum Preparation {
	Ready(Box<Ready>),
	/// Several packages fit. The user must pick one; none is chosen for them.
	Choose(Vec<Candidate>),
	/// Nothing can be installed right now, and why.
	Unavailable(String),
}

/// A user-added repository, before any asset is chosen.
#[derive(Clone, Debug)]
pub struct CustomPreparation {
	pub identity: RepositoryIdentity,
	pub release_id: u64,
	pub tag: String,
	pub prerelease: bool,
	pub options: Vec<CustomOption>,
	/// False when paging stopped early, so a newer release may exist.
	pub discovery_complete: bool,
}

impl Engine {
	/// Plans an install of a catalog app for this device.
	pub async fn prepare_install(&self, slug: &str) -> Result<Preparation, LauncherError> {
		let provider = GithubProvider::new(self.http.clone());

		self.prepare_curated(&provider, slug, None, HostTarget::current(), now_unix())
			.await
	}

	/// Plans an update of an installed catalog app, keeping its package type.
	pub async fn prepare_update(
		&self,
		installation: InstallationId,
	) -> Result<Preparation, LauncherError> {
		let provider = GithubProvider::new(self.http.clone());

		self.prepare_update_with(&provider, installation, HostTarget::current(), now_unix())
			.await
	}

	pub(crate) async fn prepare_update_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		installation: InstallationId,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<Preparation, LauncherError> {
		let record = self
			.database
			.installation(installation)
			.await?
			.ok_or_else(|| {
				LauncherError::new(
					ErrorCode::InstallationNotFound,
					"That app is not installed.",
				)
			})?;

		let Some(slug) = record
			.catalog_slug
			.clone()
			.filter(|_| record.origin == "curated")
		else {
			return Ok(Preparation::Unavailable(
				"This app was added from a repository you chose. Install its newer release from the source again."
					.into(),
			));
		};

		let preparation = self
			.prepare_curated(
				provider,
				&slug,
				Some((installation, &record.variant_id)),
				host,
				now,
			)
			.await?;

		// Say "up to date" instead of offering the version already installed.
		if let Preparation::Ready(ready) = &preparation
			&& ready.candidate.release.id == record.release_id
			&& ready.candidate.asset.id == record.asset_id
		{
			return Ok(Preparation::Unavailable(format!(
				"{} {} is up to date.",
				record.display_name, record.release_tag
			)));
		}

		Ok(preparation)
	}

	/// The shared curated path. `update` carries the installation and its
	/// package type when this is an update.
	pub(crate) async fn prepare_curated<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		slug: &str,
		update: Option<(InstallationId, &str)>,
		host: Option<HostTarget>,
		now: i64,
	) -> Result<Preparation, LauncherError> {
		let state = self.catalog().state();
		let unavailable = |why: &str| Ok(Preparation::Unavailable(why.to_owned()));

		let Some(catalog) = state.catalog.clone() else {
			return unavailable("There is no verified catalog yet, so apps cannot be installed.");
		};

		let clock = OffsetDateTime::from_unix_timestamp(now).unwrap_or(OffsetDateTime::UNIX_EPOCH);

		if catalog.freshness(clock) == Freshness::Expired {
			return unavailable(
				"The catalog has expired, so new installs and updates are on hold until it is renewed.",
			);
		}

		let Some(project) = catalog.project(slug) else {
			return unavailable("That app is not in the catalog.");
		};

		let Some(host) = host else {
			return unavailable("This device type is not recognized by the catalog.");
		};

		// Pin the repository by its numeric IDs, so a renamed, transferred,
		// or recreated repository cannot pass as the one the catalog approved.
		let identity = provider
			.repository_by_id(parse_id(&project.source.repository_id)?)
			.await?;

		if identity.repository_id.to_string() != project.source.repository_id
			|| identity.owner_id.to_string() != project.source.owner_id
		{
			return Err(LauncherError::new(
				ErrorCode::SourceIdentityChanged,
				"The repository no longer matches the one Remade approved, so nothing was installed.",
			));
		}

		let observation = provider.releases(identity.clone()).await?;
		let supported = self.catalog().config().supported_capabilities;
		let resolution = resolve_curated(project, &observation, host, supported);

		let mut notes = Vec::new();

		if identity.display_name() != project.source.display_name() {
			notes.push(format!(
				"The repository has been renamed from {} to {}. Its identity still matches.",
				project.source.display_name(),
				identity.display_name()
			));
		}

		if !resolution.discovery_complete {
			notes.push("Only the most recent releases were checked.".into());
		}

		let wanted = update.map(|(_, variant)| variant);

		let candidate = match resolution.outcome {
			Outcome::Candidate(candidate) => *candidate,
			Outcome::Ambiguous(mut candidates) => {
				candidates.retain(|candidate| {
					wanted.is_none_or(|variant| candidate.variant_id == variant)
				});

				match candidates.len() {
					0 => {
						return unavailable(
							"A new version exists, but not in the package type you have installed.",
						);
					}
					1 => candidates.remove(0),
					_ => return Ok(Preparation::Choose(candidates)),
				}
			}
			Outcome::NoCompatible(reason) => {
				return unavailable(&explain(&reason, resolution.discovery_complete));
			}
			Outcome::Blocked(state) => {
				let why = project
					.status
					.reason
					.as_deref()
					.unwrap_or("No reason was given.");

				return unavailable(&format!(
					"Remade has {} this app. {why}",
					status_word(state)
				));
			}
		};

		if let Some(variant) = wanted
			&& candidate.variant_id != variant
		{
			return unavailable(
				"A new version exists, but not in the package type you have installed.",
			);
		}

		let variant = project
			.variants
			.iter()
			.find(|variant| variant.id == candidate.variant_id)
			.ok_or_else(internal_mismatch)?;

		let install = match install_binding(project, variant, &candidate) {
			Ok(install) => install,
			Err(why) => return unavailable(&why),
		};

		let operation = if update.is_some() {
			Operation::Update
		} else {
			Operation::Install
		};
		let origin = Origin::Curated {
			feed_id: catalog.feed_id().to_owned(),
			revision: catalog.revision(),
			catalog_digest: hex::encode(catalog.digest()),
			slug: slug.to_owned(),
		};
		let source = SourceBinding {
			repository_id: identity.repository_id,
			owner_id: identity.owner_id,
			display: identity.display_name(),
		};
		let plan = Plan::for_candidate(operation, origin, source, &candidate, install, now)
			.map_err(|_| internal_mismatch())?;

		Ok(Preparation::Ready(Box::new(Ready {
			plan,
			candidate,
			meta: InstallMeta {
				display_name: project.display.name.clone(),
				catalog_slug: Some(slug.to_owned()),
			},
			source_display: identity.display_name(),
			unreviewed: false,
			notes,
		})))
	}

	/// Looks up a repository the user pasted. Nothing is installable from the
	/// result until the user picks a file and acknowledges the source.
	pub async fn prepare_custom(&self, url: &str) -> Result<CustomPreparation, LauncherError> {
		let provider = GithubProvider::new(self.http.clone());

		self.prepare_custom_with(&provider, url, HostTarget::current())
			.await
	}

	pub(crate) async fn prepare_custom_with<T: Transport>(
		&self,
		provider: &GithubProvider<T>,
		url: &str,
		host: Option<HostTarget>,
	) -> Result<CustomPreparation, LauncherError> {
		let source = parse_source_url(url)?;
		let identity = provider
			.repository_by_name(&source.owner, &source.repository)
			.await?;
		let observation: ReleaseObservation = provider.releases(identity.clone()).await?;

		let release = match &source.tag {
			Some(tag) => observation
				.releases
				.iter()
				.find(|release| &release.tag == tag),
			// With no tag given, offer the newest stable release.
			None => observation
				.releases
				.iter()
				.find(|release| !release.prerelease),
		}
		.ok_or_else(|| {
			LauncherError::new(
				ErrorCode::ProviderNotFound,
				"No matching release was found in that repository.",
			)
		})?;

		let host = host.ok_or_else(|| {
			LauncherError::new(
				ErrorCode::PackageUnsupported,
				"This device type is not recognized.",
			)
		})?;

		Ok(CustomPreparation {
			identity,
			release_id: release.id,
			tag: release.tag.clone(),
			prerelease: release.prerelease,
			options: custom_candidates(release, host),
			discovery_complete: observation.complete,
		})
	}
}

/// Builds the plan for one file the user chose from a custom repository.
pub fn custom_plan(
	preparation: &CustomPreparation,
	asset_id: u64,
	now: i64,
) -> Result<Ready, LauncherError> {
	let option = preparation
		.options
		.iter()
		.find(|option| option.asset.id == asset_id)
		.ok_or_else(|| {
			LauncherError::new(
				ErrorCode::PackageUnsupported,
				"That file is not one of the choices.",
			)
		})?;

	let format = match option.guess.format {
		Some(PackageFormat::Zip) if option.managed => "zip",
		Some(PackageFormat::TarGz) if option.managed => "tar.gz",
		_ => {
			return Err(LauncherError::new(
				ErrorCode::PackageUnsupported,
				"The launcher can only manage zip and tar.gz packages from your own sources for now. Open the release page to download this file yourself.",
			));
		}
	};

	// The host's checksum, when there is one, is still the host's claim: it
	// proves the bytes are what GitHub holds, not who made them.
	let (evidence, expected) = match &option.asset.host_sha256 {
		Some(digest) => (EvidenceLevel::HostDigest, Some(digest.clone())),
		None => (EvidenceLevel::LocalObserved, None),
	};

	let candidate = Candidate {
		release: launcher_resolver::ReleaseRef {
			id: preparation.release_id,
			tag: preparation.tag.clone(),
			page_url: String::new(),
			prerelease: preparation.prerelease,
			order: launcher_resolver::OrderBasis::ProviderOrder,
		},
		variant_id: "custom".into(),
		asset: option.asset.clone(),
		explanation: vec![format!(
			"You chose {} from {}.",
			option.asset.name,
			preparation.identity.display_name()
		)],
		skipped_newer: Vec::new(),
	};

	let source_display = preparation.identity.display_name();
	let plan = Plan::for_candidate(
		Operation::Install,
		Origin::Custom,
		SourceBinding {
			repository_id: preparation.identity.repository_id,
			owner_id: preparation.identity.owner_id,
			display: source_display.clone(),
		},
		&candidate,
		InstallBinding {
			variant_id: "custom".into(),
			adapter: "portable-v1".into(),
			scope: "user".into(),
			format: format.into(),
			entry_point: None,
			strip_components: 0,
			evidence,
			expected_sha256: expected,
		},
		now,
	)
	.map_err(|_| internal_mismatch())?;

	Ok(Ready {
		plan,
		candidate,
		meta: InstallMeta {
			display_name: preparation.identity.name.clone(),
			catalog_slug: None,
		},
		source_display,
		unreviewed: true,
		notes: vec!["Remade has not reviewed this source. A checksum can show the file is intact; it cannot show who made it or that it is safe.".into()],
	})
}

impl Engine {
	/// Approves and runs a prepared plan.
	pub async fn install(
		&self,
		ready: &Ready,
		consent: &Consent,
	) -> Result<InstallationId, LauncherError> {
		let now = now_unix();
		let approved = ready
			.plan
			.clone()
			.approve(consent, now)
			.map_err(approval_error)?;

		self.lifecycle().install(&approved, &ready.meta, now).await
	}

	/// Approves and runs a prepared update.
	pub async fn update(
		&self,
		installation: InstallationId,
		ready: &Ready,
		consent: &Consent,
	) -> Result<InstallationId, LauncherError> {
		let now = now_unix();
		let approved = ready
			.plan
			.clone()
			.approve(consent, now)
			.map_err(approval_error)?;

		self.lifecycle()
			.update(installation, &approved, &ready.meta, now)
			.await
	}
}

fn approval_error(error: launcher_resolver::PlanError) -> LauncherError {
	LauncherError::new(
		ErrorCode::PackageUnsupported,
		"The install was not approved. Review it and try again.",
	)
	.with_source(error)
}

fn internal_mismatch() -> LauncherError {
	LauncherError::new(
		ErrorCode::InternalInvariant,
		"The launcher could not build an install plan.",
	)
}

fn parse_id(text: &str) -> Result<u64, LauncherError> {
	text.parse().map_err(|_| internal_mismatch())
}

fn status_word(state: launcher_catalog::State) -> &'static str {
	match state {
		launcher_catalog::State::Paused => "paused",
		_ => "blocked",
	}
}

fn explain(reason: &NoCompatible, complete: bool) -> String {
	let tail = if complete {
		""
	} else {
		" Only the most recent releases were checked."
	};

	match reason {
		NoCompatible::NoVariantForDevice => "This app has no package for this device.".to_owned(),
		NoCompatible::NeedsNewerLauncher => {
			"This app needs a feature this version of the launcher lacks. Update the launcher."
				.to_owned()
		}
		NoCompatible::NoEligibleRelease { checked } => {
			format!(
				"No release of this app is eligible under its release rules ({checked} checked).{tail}"
			)
		}
		NoCompatible::NoMatchingAsset { checked } => {
			format!("No compatible build found in the {checked} releases checked.{tail}")
		}
		NoCompatible::InvalidRule => {
			"This app's catalog entry uses a rule the launcher cannot read.".to_owned()
		}
	}
}

/// The install binding for `variant`, or a plain reason it cannot be offered.
fn install_binding(
	project: &Project,
	variant: &Variant,
	candidate: &Candidate,
) -> Result<InstallBinding, String> {
	let _ = project;

	let (evidence, expected) = match variant.verification.minimum {
		VerificationLevel::PublisherSignature => {
			return Err("This app requires the publisher's own signature to be checked, which this version of the launcher cannot do yet.".into());
		}
		VerificationLevel::PinnedSha256 => {
			let pinned = variant
				.verification
				.sha256
				.clone()
				.ok_or("The catalog's checksum for this app is missing.")?;

			// A host digest that disagrees with the signed pin is a red flag
			// worth stopping on before downloading anything.
			if candidate
				.asset
				.host_sha256
				.as_ref()
				.is_some_and(|host| *host != pinned)
			{
				return Err("GitHub's checksum for this file does not match the one in the signed catalog, so it was not offered.".into());
			}

			(EvidenceLevel::PinnedSha256, Some(pinned))
		}
		VerificationLevel::HostDigest => {
			let digest = candidate.asset.host_sha256.clone().ok_or(
				"GitHub reports no checksum for this file, and this app's policy requires one.",
			)?;

			(EvidenceLevel::HostDigest, Some(digest))
		}
	};

	let format = match variant.format {
		PackageFormat::Zip => "zip",
		PackageFormat::TarGz => "tar.gz",
		other => {
			return Err(format!(
				"{} packages cannot be installed by the launcher yet.",
				other.label()
			));
		}
	};

	let scope = match variant.install.scope {
		InstallScope::User => "user",
		_ => {
			return Err(
				"This app needs a system-wide install, which this version does not support.".into(),
			);
		}
	};

	Ok(InstallBinding {
		variant_id: variant.id.clone(),
		adapter: variant.install.adapter.clone(),
		scope: scope.into(),
		format: format.into(),
		entry_point: variant
			.install
			.entry_point
			.clone()
			.or_else(|| variant.install.bundle_locator.clone()),
		strip_components: variant.install.strip_components.unwrap_or(0),
		evidence,
		expected_sha256: expected,
	})
}
