//! The platform an application can be built for.
//!
//! The same vocabulary appears in the signed catalog, so these enums
//! deserialize directly from its wire values.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
	Windows,
	Macos,
	Linux,
	Freebsd,
	Netbsd,
	Openbsd,
	Dragonfly,
	Illumos,
	Solaris,
	Haiku,
}

impl Os {
	pub const fn label(self) -> &'static str {
		match self {
			Self::Windows => "Windows",
			Self::Macos => "macOS",
			Self::Linux => "Linux",
			Self::Freebsd => "FreeBSD",
			Self::Netbsd => "NetBSD",
			Self::Openbsd => "OpenBSD",
			Self::Dragonfly => "DragonFly BSD",
			Self::Illumos => "illumos",
			Self::Solaris => "Solaris",
			Self::Haiku => "Haiku",
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
	X86,
	#[serde(rename = "x86_64")]
	X86_64,
	Aarch64,
	/// A macOS package containing both Intel and Apple silicon code. It is
	/// not a wildcard for other platforms.
	Universal,
	Armv5te,
	Armv6,
	Armv7,
	#[serde(rename = "riscv64gc")]
	Riscv64gc,
	Powerpc,
	Powerpc64,
	Powerpc64le,
	S390x,
	Loongarch64,
	Mips,
	Mipsel,
	Mips64,
	Mips64el,
	Sparc64,
}

impl Arch {
	pub const fn label(self) -> &'static str {
		match self {
			Self::X86 => "x86",
			Self::X86_64 => "x86-64",
			Self::Aarch64 => "ARM64",
			Self::Universal => "Universal",
			Self::Armv5te => "ARMv5",
			Self::Armv6 => "ARMv6",
			Self::Armv7 => "ARMv7",
			Self::Riscv64gc => "RISC-V 64",
			Self::Powerpc => "PowerPC",
			Self::Powerpc64 => "PowerPC 64",
			Self::Powerpc64le => "PowerPC 64 LE",
			Self::S390x => "s390x",
			Self::Loongarch64 => "LoongArch 64",
			Self::Mips => "MIPS",
			Self::Mipsel => "MIPS LE",
			Self::Mips64 => "MIPS64",
			Self::Mips64el => "MIPS64 LE",
			Self::Sparc64 => "SPARC64",
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Abi {
	Msvc,
	Gnu,
	Gnullvm,
	Musl,
	Gnueabi,
	Gnueabihf,
	Musleabi,
	Musleabihf,
	Native,
}

/// The machine the launcher is running on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostTarget {
	pub os: Os,
	pub arch: Arch,
}

impl HostTarget {
	/// The target this build was compiled for, or `None` for a platform the
	/// catalog vocabulary does not name.
	pub fn current() -> Option<Self> {
		Some(Self {
			os: host_os()?,
			arch: host_arch()?,
		})
	}

	/// Whether a package built for `os`/`arch` runs here.
	///
	/// A universal macOS package runs on both Intel and Apple silicon. Intel
	/// packages on Apple silicon (through Rosetta) are deliberately not
	/// treated as compatible, since that would hide a missing native build.
	pub fn runs(&self, os: Os, arch: Arch) -> bool {
		self.os == os && (self.arch == arch || (arch == Arch::Universal && os == Os::Macos))
	}
}

fn host_os() -> Option<Os> {
	Some(match std::env::consts::OS {
		"windows" => Os::Windows,
		"macos" => Os::Macos,
		"linux" => Os::Linux,
		"freebsd" => Os::Freebsd,
		"netbsd" => Os::Netbsd,
		"openbsd" => Os::Openbsd,
		"dragonfly" => Os::Dragonfly,
		"illumos" => Os::Illumos,
		"solaris" => Os::Solaris,
		"haiku" => Os::Haiku,
		_ => return None,
	})
}

fn host_arch() -> Option<Arch> {
	Some(match std::env::consts::ARCH {
		"x86" => Arch::X86,
		"x86_64" => Arch::X86_64,
		"aarch64" => Arch::Aarch64,
		"arm" => Arch::Armv7,
		"riscv64" => Arch::Riscv64gc,
		"powerpc" => Arch::Powerpc,
		"powerpc64" => Arch::Powerpc64,
		"s390x" => Arch::S390x,
		"loongarch64" => Arch::Loongarch64,
		"mips" => Arch::Mips,
		"mips64" => Arch::Mips64,
		"sparc64" => Arch::Sparc64,
		_ => return None,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn catalog_wire_values_deserialize() {
		assert_eq!(serde_json::from_str::<Os>("\"macos\"").unwrap(), Os::Macos);
		assert_eq!(
			serde_json::from_str::<Arch>("\"x86_64\"").unwrap(),
			Arch::X86_64
		);
		assert_eq!(
			serde_json::from_str::<Arch>("\"riscv64gc\"").unwrap(),
			Arch::Riscv64gc
		);
		assert_eq!(
			serde_json::from_str::<Abi>("\"musleabihf\"").unwrap(),
			Abi::Musleabihf
		);
		assert!(serde_json::from_str::<Os>("\"plan9\"").is_err());
	}

	#[test]
	fn universal_runs_only_on_macos() {
		let mac = HostTarget {
			os: Os::Macos,
			arch: Arch::Aarch64,
		};
		let linux = HostTarget {
			os: Os::Linux,
			arch: Arch::Aarch64,
		};

		assert!(mac.runs(Os::Macos, Arch::Universal));
		assert!(!linux.runs(Os::Linux, Arch::Universal));
	}

	#[test]
	fn intel_package_is_not_assumed_to_run_on_apple_silicon() {
		let mac = HostTarget {
			os: Os::Macos,
			arch: Arch::Aarch64,
		};

		assert!(!mac.runs(Os::Macos, Arch::X86_64));
		assert!(mac.runs(Os::Macos, Arch::Aarch64));
	}
}
