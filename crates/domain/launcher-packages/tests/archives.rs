//! Adversarial and ordinary archives, built on the fly.

#![allow(clippy::unwrap_used)]

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use launcher_packages::{EntryKind, Format, Limits, OwnedKind, PackageError, extract, inspect};
use zip::write::SimpleFileOptions;

struct Workspace {
	_directory: tempfile::TempDir,
	root: PathBuf,
}

fn workspace() -> Workspace {
	let directory = tempfile::tempdir().unwrap();
	let root = directory.path().to_path_buf();

	Workspace {
		_directory: directory,
		root,
	}
}

type ZipEntry<'a> = (&'a str, &'a [u8], u32);

fn write_zip(path: &Path, entries: &[ZipEntry<'_>]) {
	let mut writer = zip::ZipWriter::new(File::create(path).unwrap());

	for (name, data, mode) in entries {
		let options = SimpleFileOptions::default().unix_permissions(*mode);

		if name.ends_with('/') {
			writer.add_directory(*name, options).unwrap();
		} else {
			writer.start_file(*name, options).unwrap();
			writer.write_all(data).unwrap();
		}
	}

	writer.finish().unwrap();
}

/// Builds a tar.gz, writing names verbatim so unsafe ones can be tested.
fn write_tar_gz(path: &Path, entries: &[(&str, tar::EntryType, &[u8], u32, &str)]) {
	let encoder =
		flate2::write::GzEncoder::new(File::create(path).unwrap(), flate2::Compression::default());
	let mut builder = tar::Builder::new(encoder);

	for (name, kind, data, mode, link) in entries {
		let mut header = tar::Header::new_gnu();

		header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
		header.set_entry_type(*kind);
		header.set_size(data.len() as u64);
		header.set_mode(*mode);

		if !link.is_empty() {
			header.as_old_mut().linkname[..link.len()].copy_from_slice(link.as_bytes());
		}

		header.set_cksum();
		builder.append(&header, *data).unwrap();
	}

	builder.into_inner().unwrap().finish().unwrap();
}

fn inspect_zip(
	entries: &[ZipEntry<'_>],
	strip: u8,
) -> Result<launcher_packages::Inventory, PackageError> {
	let space = workspace();
	let archive = space.root.join("a.zip");
	write_zip(&archive, entries);

	inspect(&archive, Format::Zip, strip, Limits::default())
}

#[test]
fn an_ordinary_zip_round_trips_with_hashes_and_permissions() {
	let space = workspace();
	let archive = space.root.join("app.zip");
	write_zip(
		&archive,
		&[
			("app-1.0/", b"", 0o755),
			("app-1.0/bin/", b"", 0o755),
			("app-1.0/bin/run", b"#!/bin/sh\necho hi\n", 0o755),
			("app-1.0/data.txt", b"hello", 0o644),
		],
	);

	let inventory = inspect(&archive, Format::Zip, 1, Limits::default()).unwrap();

	assert!(inventory.has_file("bin/run"));
	assert!(inventory.has_path("bin"));
	assert_eq!(inventory.total_bytes, 18 + 5);

	let destination = space.root.join("out");
	let owned = extract(&archive, &inventory, &destination).unwrap();

	assert_eq!(
		std::fs::read_to_string(destination.join("data.txt")).unwrap(),
		"hello"
	);

	let data = owned.iter().find(|file| file.path == "data.txt").unwrap();
	assert_eq!(
		data.sha256,
		"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
	);
	assert_eq!(data.size, 5);
	assert!(
		owned
			.iter()
			.any(|file| file.path == "bin" && file.kind == OwnedKind::Directory)
	);

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;

		let mode = |name: &str| {
			std::fs::metadata(destination.join(name))
				.unwrap()
				.permissions()
				.mode() & 0o777
		};

		assert_eq!(mode("bin/run"), 0o755);
		assert_eq!(mode("data.txt"), 0o644);
	}
}

#[test]
fn an_ordinary_tar_gz_round_trips() {
	let space = workspace();
	let archive = space.root.join("app.tar.gz");
	write_tar_gz(
		&archive,
		&[
			("app/", tar::EntryType::Directory, b"", 0o755, ""),
			("app/run", tar::EntryType::Regular, b"binary", 0o755, ""),
			("app/readme", tar::EntryType::Regular, b"read me", 0o644, ""),
		],
	);

	let inventory = inspect(&archive, Format::TarGz, 1, Limits::default()).unwrap();
	let destination = space.root.join("out");
	extract(&archive, &inventory, &destination).unwrap();

	assert_eq!(std::fs::read(destination.join("run")).unwrap(), b"binary");
	assert_eq!(
		std::fs::read(destination.join("readme")).unwrap(),
		b"read me"
	);
}

#[test]
fn zip_with_escaping_or_absolute_paths_is_refused() {
	for name in [
		"../evil",
		"a/../../evil",
		"/etc/passwd",
		"C:/win",
		"a\\b",
		"nul",
		"x/aux.txt",
	] {
		let error = inspect_zip(&[(name, b"x", 0o644)], 0).unwrap_err();

		assert!(
			matches!(error, PackageError::UnsafePath(_)),
			"{name}: {error:?}"
		);
	}
}

#[test]
fn tar_with_escaping_paths_is_refused() {
	for name in ["../evil", "a/../../evil", "/abs/path"] {
		let space = workspace();
		let archive = space.root.join("a.tar.gz");
		write_tar_gz(
			&archive,
			&[(name, tar::EntryType::Regular, b"x", 0o644, "")],
		);

		let error = inspect(&archive, Format::TarGz, 0, Limits::default()).unwrap_err();

		assert!(
			matches!(error, PackageError::UnsafePath(_)),
			"{name}: {error:?}"
		);
	}
}

#[test]
fn special_files_and_hard_links_are_refused() {
	for kind in [
		tar::EntryType::Link,
		tar::EntryType::Char,
		tar::EntryType::Block,
		tar::EntryType::Fifo,
	] {
		let space = workspace();
		let archive = space.root.join("a.tar.gz");
		write_tar_gz(&archive, &[("thing", kind, b"", 0o644, "target")]);

		let error = inspect(&archive, Format::TarGz, 0, Limits::default()).unwrap_err();

		assert!(
			matches!(error, PackageError::Unsupported(_)),
			"{kind:?}: {error:?}"
		);
	}
}

#[cfg(unix)]
#[test]
fn symlinks_must_stay_inside_the_app() {
	let space = workspace();
	let archive = space.root.join("a.tar.gz");

	write_tar_gz(
		&archive,
		&[(
			"lib/a",
			tar::EntryType::Symlink,
			b"",
			0o777,
			"../escape/../../x",
		)],
	);
	assert!(matches!(
		inspect(&archive, Format::TarGz, 0, Limits::default()),
		Err(PackageError::UnsafePath(_))
	));

	write_tar_gz(
		&archive,
		&[("lib/a", tar::EntryType::Symlink, b"", 0o777, "/etc/passwd")],
	);
	assert!(inspect(&archive, Format::TarGz, 0, Limits::default()).is_err());

	write_tar_gz(
		&archive,
		&[
			("lib/real", tar::EntryType::Regular, b"x", 0o644, ""),
			("lib/link", tar::EntryType::Symlink, b"", 0o777, "real"),
		],
	);

	let inventory = inspect(&archive, Format::TarGz, 0, Limits::default()).unwrap();
	let destination = space.root.join("out");
	extract(&archive, &inventory, &destination).unwrap();

	assert_eq!(
		std::fs::read_to_string(destination.join("lib/link")).unwrap(),
		"x"
	);
}

#[cfg(unix)]
#[test]
fn a_symlink_entry_cannot_redirect_a_later_write_outside_the_destination() {
	let space = workspace();
	let archive = space.root.join("a.tar.gz");
	let outside = space.root.join("outside");
	std::fs::create_dir(&outside).unwrap();

	// The link's target looks contained to a lexical check only if it is
	// relative; this one is refused at inspection. Writing through it is
	// also impossible because links are created after all files.
	write_tar_gz(
		&archive,
		&[
			("dir/link", tar::EntryType::Symlink, b"", 0o777, "../.."),
			(
				"dir/link/payload",
				tar::EntryType::Regular,
				b"pwned",
				0o644,
				"",
			),
		],
	);

	assert!(inspect(&archive, Format::TarGz, 0, Limits::default()).is_err());
	assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

#[test]
fn names_that_collide_by_case_are_refused() {
	let error = inspect_zip(&[("App.txt", b"a", 0o644), ("app.TXT", b"b", 0o644)], 0).unwrap_err();

	assert!(matches!(error, PackageError::Duplicate(_)), "{error:?}");
}

#[test]
fn a_file_cannot_also_be_a_directory() {
	let error = inspect_zip(&[("a", b"x", 0o644), ("a/b", b"y", 0o644)], 0).unwrap_err();

	assert!(matches!(error, PackageError::Duplicate(_)), "{error:?}");
}

#[test]
fn limits_are_enforced() {
	let space = workspace();
	let archive = space.root.join("a.zip");
	write_zip(
		&archive,
		&[("a", &[0u8; 100], 0o644), ("b", &[0u8; 100], 0o644)],
	);

	let small_total = Limits {
		max_total_bytes: 150,
		..Limits::default()
	};
	assert!(matches!(
		inspect(&archive, Format::Zip, 0, small_total),
		Err(PackageError::TooLarge)
	));

	let small_file = Limits {
		max_file_bytes: 50,
		..Limits::default()
	};
	assert!(matches!(
		inspect(&archive, Format::Zip, 0, small_file),
		Err(PackageError::TooLarge)
	));

	let few = Limits {
		max_entries: 1,
		..Limits::default()
	};
	assert!(matches!(
		inspect(&archive, Format::Zip, 0, few),
		Err(PackageError::TooManyEntries)
	));
}

#[test]
fn a_suffix_cannot_change_the_format() {
	let space = workspace();
	let archive = space.root.join("really-a-zip.tar.gz");
	write_zip(&archive, &[("a", b"x", 0o644)]);

	assert!(matches!(
		inspect(&archive, Format::TarGz, 0, Limits::default()),
		Err(PackageError::FormatMismatch)
	));
	assert!(inspect(&archive, Format::Zip, 0, Limits::default()).is_ok());

	let junk = space.root.join("junk.zip");
	std::fs::write(&junk, b"not an archive at all").unwrap();
	assert!(matches!(
		inspect(&junk, Format::Zip, 0, Limits::default()),
		Err(PackageError::FormatMismatch)
	));
}

#[test]
fn extraction_refuses_a_non_empty_destination_and_never_overwrites() {
	let space = workspace();
	let archive = space.root.join("a.zip");
	write_zip(&archive, &[("keep.txt", b"new", 0o644)]);
	let inventory = inspect(&archive, Format::Zip, 0, Limits::default()).unwrap();

	let destination = space.root.join("out");
	std::fs::create_dir(&destination).unwrap();
	std::fs::write(destination.join("keep.txt"), "mine").unwrap();

	assert!(extract(&archive, &inventory, &destination).is_err());
	assert_eq!(
		std::fs::read_to_string(destination.join("keep.txt")).unwrap(),
		"mine"
	);
}

#[test]
fn stripping_can_remove_the_top_level_directory_entirely() {
	let inventory = inspect_zip(&[("top/", b"", 0o755), ("top/f", b"x", 0o644)], 1).unwrap();

	assert_eq!(inventory.entries.len(), 1);
	assert_eq!(inventory.entries[0].path, "f");
	assert_eq!(inventory.entries[0].kind, EntryKind::File);
}

#[test]
fn many_files_extract_in_parallel_correctly() {
	let space = workspace();
	let archive = space.root.join("many.zip");
	let names: Vec<String> = (0..600)
		.map(|i| format!("d{}/file{i}.txt", i % 7))
		.collect();
	let contents: Vec<Vec<u8>> = (0..600)
		.map(|i| format!("content {i}").into_bytes())
		.collect();
	let entries: Vec<ZipEntry<'_>> = names
		.iter()
		.zip(&contents)
		.map(|(name, data)| (name.as_str(), data.as_slice(), 0o644))
		.collect();

	write_zip(&archive, &entries);

	let inventory = inspect(&archive, Format::Zip, 0, Limits::default()).unwrap();
	let destination = space.root.join("out");
	let owned = extract(&archive, &inventory, &destination).unwrap();

	assert_eq!(
		owned.iter().filter(|f| f.kind == OwnedKind::File).count(),
		600
	);

	for (name, data) in names.iter().zip(&contents) {
		assert_eq!(
			&std::fs::read(destination.join(name)).unwrap(),
			data,
			"{name}"
		);
	}
}

#[test]
fn a_highly_compressed_archive_is_refused_as_a_bomb() {
	let space = workspace();
	let archive = space.root.join("bomb.zip");
	let zeros = vec![0u8; 80 * 1024 * 1024];

	write_zip(&archive, &[("zeros.bin", &zeros, 0o644)]);

	assert!(matches!(
		inspect(&archive, Format::Zip, 0, Limits::default()),
		Err(PackageError::CompressionBomb)
	));
}
