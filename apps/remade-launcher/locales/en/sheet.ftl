# en: sheet (install review, messages, uninstall, restore defaults, update all, notes) and Add from GitHub.

sheet-ok = OK
sheet-close = Close

sheet-choose-title = Several packages fit
sheet-choose-body = More than one package fits this device equally well, and the launcher will not guess which you want. This is a gap in the catalog entry, not something you did.

sheet-uninstall-title = Uninstall { $name }?
sheet-uninstall-installed = Installed { $date }
sheet-uninstall-location = Where it lives
sheet-uninstall-what-happens = What happens:
sheet-uninstall-removes = The files the launcher installed are removed.
sheet-uninstall-keeps = Anything you edited, and data the app saved itself, is kept.
sheet-uninstall-report = Afterwards you see exactly what was removed and what stayed.
sheet-uninstall-confirm = Uninstall

sheet-mode-intro = Before you turn this on, know that:
sheet-mode-risk-behavior = A new version can change how an app behaves, or stop it working.
sheet-mode-risk-compromised = If a publisher's account is compromised, a harmful release could be installed.
sheet-mode-risk-migration = An update can migrate an app's data in a way older versions cannot read.
sheet-mode-risk-unattended = Updates run without asking, and no scan can promise software is safe.
sheet-mode-daemon-warning = This also registers a background program that starts each time you sign in, and keeps running after you close the Launcher. You can turn it off here or in your system's login items.
sheet-mode-acknowledge = I understand, and I want automatic updates
sheet-mode-confirm = Turn on

sheet-update-all-title = Update everything?
sheet-update-all-line = { $name }: { $from } to { $to }
sheet-update-all-note = Each app is checked and replaced one at a time. A failure in one does not stop the others.
sheet-update-all-confirm = Update all

sheet-notes-empty = This release has no notes.

sheet-restore-title = Restore defaults?
sheet-restore-intro = This puts the launcher's own settings back the way a new install has them:
sheet-restore-appearance = Appearance follows your system again.
sheet-restore-updates = Updates go back to manual, with a one-hour minimum age.
sheet-restore-app-settings = Every app's own update settings are cleared, including pins.
sheet-restore-daemon = A background updater, if you turned one on, is switched off.
sheet-restore-note = Your installed apps and their files are not touched. This cannot be undone.
sheet-restore-confirm = Restore defaults

sheet-removed-title = { $name } was uninstalled
sheet-removed-count = { $count ->
    [one] { $count } file removed.
   *[other] { $count } files removed.
}
sheet-removed-kept = Some files were kept so nothing you made is lost:

sheet-review-install = Install
sheet-review-update = Update
sheet-review-install-title = Install { $name }
sheet-review-update-title = Update { $name }
sheet-review-source = Source
sheet-review-unreviewed = Remade has not reviewed this source. Only install software from people you trust.
sheet-review-verified = Listed in the verified Remade catalog.
sheet-review-version = Version
sheet-review-file = File
sheet-review-checked-against = Checked against
sheet-review-skipped = Newer release { $tag } skipped: { $reason }
sheet-review-acknowledge = I understand this source is not reviewed by Remade
sheet-evidence-host-digest = the checksum GitHub reports for the file. It shows the file is intact, not who made it
sheet-evidence-pinned = a checksum pinned in the signed Remade catalog
sheet-evidence-signature = the publisher's own signature
sheet-evidence-local = nothing beyond the download itself. No checksum was published to compare

add-title = Add from GitHub
add-intro = Paste a public GitHub repository or release link. Remade has not reviewed apps you add this way, and a checksum cannot show who made a file or that it is safe.
add-url-hint = https://github.com/owner/repository
add-look-up = Look up
add-release = Release { $tag }
add-partial = Only the most recent releases were checked.
add-no-files = This release has no downloadable app files.
add-review-install = Review install
add-download-only = Download only
add-fit-matches = Named for this device
add-fit-unknown = Name does not say which device
add-fit-mismatch = Named for another device
