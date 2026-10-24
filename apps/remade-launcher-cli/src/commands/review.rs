//! The CLI's version of the install review: the same facts the desktop
//! sheet shows, so consent means the same thing in both.

use launcher_core::Ready;
use launcher_resolver::EvidenceLevel;

use crate::output::bytes;

pub(crate) fn print(ready: &Ready, verb: &str) {
	let body = ready.plan.body();

	eprintln!("{verb} {}", ready.meta.display_name);
	eprintln!("  Source:         {}", ready.source_display);

	if ready.unreviewed {
		eprintln!(
			"  Warning:        Remade has not reviewed this source. Only install software from people you trust."
		);
	} else {
		eprintln!("  Reviewed:       listed in the verified Remade catalog");
	}

	eprintln!("  Version:        {}", body.release.tag);
	eprintln!(
		"  File:           {} ({})",
		body.asset.name,
		bytes(body.asset.size)
	);
	eprintln!("  Checked against: {}", evidence(body.install.evidence));

	for line in &ready.candidate.explanation {
		eprintln!("  - {line}");
	}

	for note in &ready.notes {
		eprintln!("  Note: {note}");
	}

	for skipped in &ready.candidate.skipped_newer {
		eprintln!(
			"  Newer release {} skipped: {}",
			skipped.tag, skipped.reason
		);
	}
}

fn evidence(level: EvidenceLevel) -> &'static str {
	match level {
		EvidenceLevel::HostDigest => {
			"the checksum GitHub reports (shows the file is intact, not who made it)"
		}
		EvidenceLevel::PinnedSha256 => "a checksum pinned in the signed catalog",
		EvidenceLevel::PublisherSignature => "the publisher's signature",
		EvidenceLevel::LocalObserved => {
			"nothing beyond the download itself (no checksum was published)"
		}
	}
}
