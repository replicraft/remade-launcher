# de: Sheets (Installationsprüfung, Hinweise, Deinstallation, Standard wiederherstellen, alles aktualisieren, Notizen) und „Von GitHub hinzufügen“.

sheet-ok = OK
sheet-close = Schließen

sheet-choose-title = Mehrere Pakete passen
sheet-choose-body = Mehr als ein Paket passt gleich gut zu diesem Gerät, und der Launcher rät nicht, welches du möchtest. Das ist eine Lücke im Katalogeintrag, nicht dein Fehler.

sheet-uninstall-title = { $name } deinstallieren?
sheet-uninstall-installed = Installiert { $date }
sheet-uninstall-location = Speicherort
sheet-uninstall-what-happens = Das passiert:
sheet-uninstall-removes = Die vom Launcher installierten Dateien werden entfernt.
sheet-uninstall-keeps = Alles, was du bearbeitet hast, und Daten, die die App selbst gespeichert hat, bleiben erhalten.
sheet-uninstall-report = Danach siehst du genau, was entfernt wurde und was geblieben ist.
sheet-uninstall-confirm = Deinstallieren

sheet-mode-intro = Bevor du das einschaltest, solltest du wissen:
sheet-mode-risk-behavior = Eine neue Version kann das Verhalten einer App ändern oder sie unbrauchbar machen.
sheet-mode-risk-compromised = Wenn das Konto eines Herausgebers kompromittiert wird, könnte eine schädliche Version installiert werden.
sheet-mode-risk-migration = Ein Update kann die Daten einer App so umwandeln, dass ältere Versionen sie nicht mehr lesen können.
sheet-mode-risk-unattended = Updates laufen ohne Rückfrage, und kein Scan kann garantieren, dass Software sicher ist.
sheet-mode-daemon-warning = Dadurch wird außerdem ein Hintergrundprogramm registriert, das bei jeder Anmeldung startet und weiterläuft, nachdem du den Launcher geschlossen hast. Du kannst es hier oder in den Anmeldeobjekten deines Systems ausschalten.
sheet-mode-acknowledge = Ich habe verstanden und möchte automatische Updates
sheet-mode-confirm = Einschalten

sheet-update-all-title = Alles aktualisieren?
sheet-update-all-line = { $name }: { $from } auf { $to }
sheet-update-all-note = Jede App wird einzeln geprüft und ersetzt. Ein Fehler bei einer App stoppt die anderen nicht.
sheet-update-all-confirm = Alle aktualisieren

sheet-notes-empty = Diese Version hat keine Hinweise.

sheet-restore-title = Standardwerte wiederherstellen?
sheet-restore-intro = Damit werden die eigenen Einstellungen des Launchers so zurückgesetzt, wie sie bei einer Neuinstallation sind:
sheet-restore-appearance = Das Erscheinungsbild folgt wieder deinem System.
sheet-restore-updates = Updates werden wieder manuell ausgeführt, mit einem Mindestalter von einer Stunde.
sheet-restore-app-settings = Die Update-Einstellungen jeder App werden gelöscht, einschließlich angehefteter Versionen.
sheet-restore-daemon = Ein Hintergrund-Updater, falls du einen eingeschaltet hast, wird ausgeschaltet.
sheet-restore-note = Deine installierten Apps und ihre Dateien bleiben unberührt. Das lässt sich nicht rückgängig machen.
sheet-restore-confirm = Standardwerte wiederherstellen

sheet-removed-title = { $name } wurde deinstalliert
sheet-removed-count = { $count ->
    [one] { $count } Datei entfernt.
   *[other] { $count } Dateien entfernt.
}
sheet-removed-kept = Einige Dateien wurden behalten, damit nichts verloren geht, was du erstellt hast:

sheet-review-install = Installieren
sheet-review-update = Aktualisieren
sheet-review-install-title = { $name } installieren
sheet-review-update-title = { $name } aktualisieren
sheet-review-source = Quelle
sheet-review-unreviewed = Remade hat diese Quelle nicht geprüft. Installiere nur Software von Menschen, denen du vertraust.
sheet-review-verified = Im geprüften Remade-Katalog aufgeführt.
sheet-review-version = Version
sheet-review-file = Datei
sheet-review-checked-against = Geprüft anhand
sheet-review-skipped = Neuere Version { $tag } übersprungen: { $reason }
sheet-review-acknowledge = Ich habe verstanden, dass diese Quelle nicht von Remade geprüft wurde
sheet-evidence-host-digest = der Prüfsumme, die GitHub für die Datei meldet. Sie zeigt, dass die Datei unversehrt ist, nicht, wer sie erstellt hat
sheet-evidence-pinned = einer Prüfsumme, die im signierten Remade-Katalog festgelegt ist
sheet-evidence-signature = der eigenen Signatur des Herausgebers
sheet-evidence-local = nichts außer dem Download selbst. Es wurde keine Prüfsumme zum Vergleich veröffentlicht

add-title = Von GitHub hinzufügen
add-intro = Füge den Link zu einem öffentlichen GitHub-Repository oder einer Version ein. Remade hat Apps, die du so hinzufügst, nicht geprüft, und eine Prüfsumme kann weder zeigen, wer eine Datei erstellt hat, noch dass sie sicher ist.
add-url-hint = https://github.com/owner/repository
add-look-up = Suchen
add-release = Version { $tag }
add-partial = Es wurden nur die neuesten Versionen geprüft.
add-no-files = Diese Version hat keine herunterladbaren App-Dateien.
add-review-install = Installation prüfen
add-download-only = Nur Download
add-fit-matches = Für dieses Gerät benannt
add-fit-unknown = Der Name verrät nicht, für welches Gerät
add-fit-mismatch = Für ein anderes Gerät benannt
