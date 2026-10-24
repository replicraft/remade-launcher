//! Conservative guessing for user-added sources.
//!
//! A custom repository has no signed policy, so the launcher can only read
//! filenames. Tokens are matched at word boundaries, `darwin` is never taken
//! for Windows because it contains "win", and an asset that looks like a
//! source archive, checksum, signature, or debug symbol is excluded. A guess
//! never installs anything by itself: it only orders choices for a person.

use launcher_catalog::PackageFormat;
use launcher_model::{Arch, HostTarget, Os};
use launcher_providers::Release;
use serde::Serialize;

use crate::resolve::AssetRef;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Guess {
	pub os: Option<Os>,
	pub arch: Option<Arch>,
	pub format: Option<PackageFormat>,
	/// Why the file should not be offered as an app, if it should not.
	pub excluded: Option<&'static str>,
}

pub fn classify_asset(name: &str) -> Guess {
	let lower = name.to_lowercase();

	// `x86_64` would otherwise split into `x86` and `64`.
	let normalized = lower.replace("x86_64", "x64").replace("x86-64", "x64");
	let tokens: Vec<&str> = normalized
		.split(|c: char| !c.is_ascii_alphanumeric())
		.filter(|t| !t.is_empty())
		.collect();
	let has = |wanted: &[&str]| tokens.iter().any(|token| wanted.contains(token));

	Guess {
		os: guess_os(&has),
		arch: guess_arch(&has),
		format: guess_format(&lower),
		excluded: excluded_reason(&lower, &has),
	}
}

fn guess_os(has: &dyn Fn(&[&str]) -> bool) -> Option<Os> {
	let found: Vec<Os> = [
		(Os::Windows, &["windows", "win", "win32", "win64"][..]),
		(Os::Macos, &["macos", "darwin", "osx", "mac", "apple"][..]),
		(Os::Linux, &["linux"][..]),
		(Os::Freebsd, &["freebsd"][..]),
	]
	.into_iter()
	.filter(|(_, words)| has(words))
	.map(|(os, _)| os)
	.collect();

	// Two systems named in one file is a multi-platform bundle: unknown.
	match found.as_slice() {
		[only] => Some(*only),
		_ => None,
	}
}

fn guess_arch(has: &dyn Fn(&[&str]) -> bool) -> Option<Arch> {
	let found: Vec<Arch> = [
		(Arch::X86_64, &["x64", "amd64"][..]),
		(Arch::Aarch64, &["arm64", "aarch64"][..]),
		(Arch::Universal, &["universal", "universal2"][..]),
		(Arch::X86, &["x86", "i386", "i686", "ia32"][..]),
		(Arch::Armv7, &["armv7", "armv7l", "armhf"][..]),
		(Arch::Armv6, &["armv6", "armv6l"][..]),
	]
	.into_iter()
	.filter(|(_, words)| has(words))
	.map(|(arch, _)| arch)
	.collect();

	match found.as_slice() {
		[only] => Some(*only),
		_ => None,
	}
}

fn guess_format(lower: &str) -> Option<PackageFormat> {
	const SUFFIXES: &[(&str, PackageFormat)] = &[
		(".tar.gz", PackageFormat::TarGz),
		(".tgz", PackageFormat::TarGz),
		(".tar.xz", PackageFormat::TarXz),
		(".tar.zst", PackageFormat::TarZst),
		(".appimage", PackageFormat::AppImage),
		(".zip", PackageFormat::Zip),
		(".7z", PackageFormat::SevenZip),
		(".dmg", PackageFormat::Dmg),
		(".msi", PackageFormat::Msi),
		(".exe", PackageFormat::ExeInstaller),
		(".deb", PackageFormat::Deb),
		(".rpm", PackageFormat::Rpm),
		(".pkg", PackageFormat::Pkg),
	];

	SUFFIXES
		.iter()
		.find(|(suffix, _)| lower.ends_with(suffix))
		.map(|(_, format)| *format)
}

fn excluded_reason(lower: &str, has: &dyn Fn(&[&str]) -> bool) -> Option<&'static str> {
	const CHECKSUM: &[&str] = &[".sha256", ".sha512", ".sha1", ".md5", ".sum", ".sha256sum"];
	const SIGNATURE: &[&str] = &[".sig", ".asc", ".minisig", ".pem", ".crt"];

	if CHECKSUM.iter().any(|suffix| lower.ends_with(suffix))
		|| has(&["checksums", "sha256sums", "sha512sums"])
	{
		Some("a checksum file")
	} else if SIGNATURE.iter().any(|suffix| lower.ends_with(suffix)) {
		Some("a signature file")
	} else if has(&["source", "src"]) {
		Some("source code")
	} else if has(&["debug", "dbg", "symbols", "pdb"])
		|| lower.ends_with(".pdb")
		|| lower.ends_with(".dsym.zip")
	{
		Some("debug symbols")
	} else if has(&["sbom", "sdk", "headers", "docs"])
		|| lower.ends_with(".sbom")
		|| lower.ends_with(".txt")
	{
		Some("not an application package")
	} else {
		None
	}
}

/// How a file fits the current device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Fit {
	/// Named for this device's system and architecture.
	Matches,
	/// The name does not say enough to tell.
	Unknown,
	/// Named for a different system or architecture.
	Mismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomOption {
	pub asset: AssetRef,
	pub guess: Guess,
	pub fit: Fit,
	/// Whether Remade can manage the full lifecycle of this format.
	pub managed: bool,
}

/// Lists the downloadable files of `release`, best fits first, without
/// choosing for the user.
pub fn custom_candidates(release: &Release, host: HostTarget) -> Vec<CustomOption> {
	let mut options: Vec<CustomOption> = release
		.assets
		.iter()
		.map(|asset| (asset, classify_asset(&asset.name)))
		.filter(|(_, guess)| guess.excluded.is_none())
		.map(|(asset, guess)| CustomOption {
			asset: AssetRef {
				id: asset.id,
				name: asset.name.clone(),
				size: asset.size,
				download_url: asset.download_url.clone(),
				host_sha256: asset.host_sha256.clone(),
			},
			fit: fit(guess, host),
			managed: guess.format.is_some_and(is_managed_format),
			guess,
		})
		.collect();

	options.sort_by_key(|option| {
		let rank = match option.fit {
			Fit::Matches => 0,
			Fit::Unknown => 1,
			Fit::Mismatch => 2,
		};

		(rank, !option.managed)
	});

	options
}

fn fit(guess: Guess, host: HostTarget) -> Fit {
	match (guess.os, guess.arch) {
		(Some(os), Some(arch)) if host.runs(os, arch) => Fit::Matches,
		(Some(os), _) if os != host.os => Fit::Mismatch,
		(_, Some(arch)) if arch != host.arch && arch != Arch::Universal => Fit::Mismatch,
		_ => Fit::Unknown,
	}
}

/// Formats the launcher can install, update, and remove on its own.
fn is_managed_format(format: PackageFormat) -> bool {
	matches!(
		format,
		PackageFormat::Zip | PackageFormat::TarGz | PackageFormat::AppImage
	)
}

#[cfg(test)]
mod tests {
	use launcher_providers::Asset;

	use super::*;

	fn guess(name: &str) -> Guess {
		classify_asset(name)
	}

	#[test]
	fn reads_common_names() {
		let g = guess("Tool-1.2.0-linux-x86_64.tar.gz");
		assert_eq!(
			(g.os, g.arch, g.format),
			(
				Some(Os::Linux),
				Some(Arch::X86_64),
				Some(PackageFormat::TarGz)
			)
		);

		let g = guess("tool_1.2.0_darwin_arm64.zip");
		assert_eq!((g.os, g.arch), (Some(Os::Macos), Some(Arch::Aarch64)));

		let g = guess("Tool-Setup-win64-x64.exe");
		assert_eq!(
			(g.os, g.arch, g.format),
			(
				Some(Os::Windows),
				Some(Arch::X86_64),
				Some(PackageFormat::ExeInstaller)
			)
		);
	}

	#[test]
	fn darwin_is_not_windows() {
		assert_eq!(guess("tool-darwin-amd64.zip").os, Some(Os::Macos));
		assert_eq!(guess("tool-darwin-amd64.zip").arch, Some(Arch::X86_64));
	}

	#[test]
	fn x86_is_not_arm_and_32bit_is_not_64bit() {
		assert_eq!(guess("tool-linux-i686.tar.gz").arch, Some(Arch::X86));
		assert_eq!(guess("tool-linux-armv7.tar.gz").arch, Some(Arch::Armv7));
		assert_eq!(
			guess("tool-linux.tar.gz").arch,
			None,
			"no architecture token means unknown, not universal"
		);
	}

	#[test]
	fn multi_platform_names_are_unknown() {
		assert_eq!(guess("tool-linux-windows-bundle.zip").os, None);
	}

	#[test]
	fn non_app_files_are_excluded() {
		for (name, why) in [
			("tool-1.0.tar.gz.sha256", "a checksum file"),
			("SHA256SUMS", "a checksum file"),
			("tool-linux.tar.gz.sig", "a signature file"),
			("tool-1.0-source.tar.gz", "source code"),
			("tool-1.0-src.zip", "source code"),
			("tool-win-x64-debug.zip", "debug symbols"),
			("tool.pdb", "debug symbols"),
			("README.txt", "not an application package"),
		] {
			assert_eq!(guess(name).excluded, Some(why), "{name}");
		}

		assert_eq!(guess("tool-linux-x64.tar.gz").excluded, None);
	}

	fn asset(id: u64, name: &str) -> Asset {
		Asset {
			id,
			name: name.into(),
			size: 1,
			content_type: String::new(),
			host_sha256: None,
			download_url: format!("https://github.com/o/r/releases/download/v1/{name}"),
		}
	}

	fn release(names: &[&str]) -> Release {
		Release {
			id: 1,
			tag: "v1".into(),
			name: None,
			prerelease: false,
			published_at: None,
			notes: None,
			page_url: String::new(),
			assets: names
				.iter()
				.enumerate()
				.map(|(i, n)| asset(i as u64, n))
				.collect(),
		}
	}

	const MAC: HostTarget = HostTarget {
		os: Os::Macos,
		arch: Arch::Aarch64,
	};

	#[test]
	fn candidates_are_ordered_by_fit_and_never_pre_chosen() {
		let release = release(&[
			"tool-windows-x64.zip",
			"tool-macos-arm64.zip",
			"tool-1.0-source.tar.gz",
			"tool.dmg",
			"tool-macos-x64.zip",
		]);

		let options = custom_candidates(&release, MAC);
		let names: Vec<_> = options.iter().map(|o| o.asset.name.as_str()).collect();

		assert_eq!(names[0], "tool-macos-arm64.zip");
		assert_eq!(options[0].fit, Fit::Matches);
		assert!(!names.contains(&"tool-1.0-source.tar.gz"));
		assert_eq!(options.last().unwrap().fit, Fit::Mismatch);
		assert_eq!(
			options
				.iter()
				.find(|o| o.asset.name == "tool.dmg")
				.unwrap()
				.fit,
			Fit::Unknown
		);
		assert!(
			!options
				.iter()
				.find(|o| o.asset.name == "tool.dmg")
				.unwrap()
				.managed
		);
	}

	#[test]
	fn an_irrelevant_extra_asset_does_not_change_an_unambiguous_pick() {
		let base = custom_candidates(&release(&["tool-macos-arm64.zip"]), MAC);
		let more = custom_candidates(
			&release(&["tool-macos-arm64.zip", "notes.txt", "tool-linux-x64.tar.gz"]),
			MAC,
		);

		assert_eq!(base[0].asset.name, more[0].asset.name);
	}
}
