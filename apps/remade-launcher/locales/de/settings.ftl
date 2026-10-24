# Einstellungen: Sprache und Region.

settings-language = Sprache
    .description = Die Sprache der Oberfläche. „Automatisch“ folgt dem System ({ $system }).
settings-language-auto = Automatisch
settings-time-format = Zeitformat
    .description = „Automatisch“ folgt der Region deines Systems.
time-format-system = System
time-format-12h = 12-Stunden-Format (3:42 PM)
time-format-24h = 24-Stunden-Format (15:42)
settings-date-format = Datumsformat
    .description = „Automatisch“ folgt der Region deines Systems.
date-format-system = System
date-format-mdy = Okt. 24, 2026
date-format-dmy = 24. Okt. 2026
date-format-iso = 2026-10-24
settings-region-example = Beispiel: { $today }   ·   { $earlier }

# Einstellungen: Bereiche und der Rest.

settings-title = Einstellungen
settings-appearance = Darstellung
settings-region-panel = Sprache und Region
settings-updates-panel = Updates
    .description = Wie installierte Apps aktualisiert werden. Der Launcher aktualisiert sich separat, weiter unten.
settings-launcher-panel = Launcher-Updates
    .description = Wie sich der Launcher selbst aktuell hält. Das ändert nichts daran, wie deine Apps aktualisiert werden.
settings-notifications-panel = Benachrichtigungen
settings-network-panel = Netzwerk
settings-storage-panel = Speicher
settings-support-panel = Support
settings-about-panel = Über

settings-theme = Design
theme-system = System
theme-light = Hell
theme-dark = Dunkel
settings-scale = Oberflächengröße
    .description = Macht alles größer oder kleiner. Praktisch auf hochauflösenden Bildschirmen.
settings-start-page = Öffnen mit
    .description = Die Seite, die der Launcher beim Start zeigt.
start-page-auto = Bibliothek, wenn ich Apps habe, sonst Entdecken
start-page-library = Bibliothek
start-page-explore = Entdecken
start-page-last-used = Wo ich aufgehört habe
settings-layout-note = Dein Layout, etwa die eingeklappte Seitenleiste, Raster oder Liste und die Fenstergröße, wird automatisch gemerkt.

update-mode-manual = Manuelle Updates (Standard)
    .description = Sucht nach Updates, wenn du den Launcher öffnest. Installierte Apps zeigen eine Schaltfläche „Aktualisieren“, du entscheidest also, wann installiert wird.
update-mode-on-launch = Automatisch aktualisieren beim Start des Launchers
    .description = Bei jedem Start des Launchers werden passende App-Updates automatisch installiert. Nach dem Schließen läuft nichts weiter und es werden keine Apps im Hintergrund aktualisiert.
update-mode-daemon = Automatisch aktualisieren mit dauerhaft aktivem Hintergrunddienst (gefährlich)
    .description = Startet beim Anmelden einen schlanken Hintergrunddienst. Er prüft beim Start und solange er aktiv ist alle 24 Stunden, außerdem wenn du den Launcher öffnest. Updates werden ohne einzelne Bestätigung installiert. Bösartige, kompromittierte oder fehlerhafte Upstream-Versionen könnten automatisch installiert werden und Software oder Daten beschädigen.
settings-min-age = Standard-Mindestalter einer Version für automatische Updates
    .automatic = Eine Version muss mindestens so alt sein, bevor ein automatisches Update sie installiert. Jede App kann ihren eigenen Wert nutzen.
    .manual = Gilt nur für die automatischen Modi. Ein Update, das du selbst startest, wird nie zurückgehalten.
minimum-age-0 = Keine Verzögerung
minimum-age-3600 = 1 Stunde
minimum-age-21600 = 6 Stunden
minimum-age-43200 = 12 Stunden
minimum-age-86400 = 24 Stunden
minimum-age-259200 = 3 Tage
minimum-age-604800 = 7 Tage
settings-min-age-warning = Ohne Verzögerung wird eine Version sofort nach der Veröffentlichung installiert. Das ist riskanter.
settings-background = Hintergrunddienst für Updates
settings-background-checking = Prüfe …
settings-background-active = Aktiv
settings-background-stopped = Gestoppt
    .detail = Er startet, wenn du dich anmeldest.
settings-background-pending = Wartet auf Freigabe durch das System
    .detail = Gib ihn in den Anmeldeobjekten deines Systems frei.
settings-background-disabled = Vom System deaktiviert
    .detail = Du hast ihn in den Systemeinstellungen ausgeschaltet. Der Launcher schaltet ihn nicht wieder ein.
settings-background-not-installed = Nicht installiert
settings-background-failed = Fehlgeschlagen
settings-background-unavailable = Hier nicht verfügbar
settings-background-last-check = Letzte Prüfung: { $when }
settings-background-last-result = Letztes Ergebnis: { $outcome }
settings-background-next-check = Nächste Prüfung: { $when }
settings-background-stop = Hintergrund-Updates stoppen

launcher-updates-automatic = Automatisch
    .description = Lädt einen neuen Launcher im Hintergrund herunter und wechselt dorthin, wenn du diesen schließt.
launcher-updates-notify = Benachrichtigen
    .description = Sagt Bescheid, wenn ein neuer Launcher verfügbar ist. Den Rest erledigst du.
launcher-updates-off = Aus
    .description = Sucht nie nach einem neuen Launcher.
settings-launcher-last-checked = Zuletzt geprüft: { $when }
settings-launcher-staged = { $version } ist heruntergeladen. Sie startet beim nächsten Öffnen des Launchers.
settings-launcher-available = { $version } ist verfügbar.
settings-launcher-applied = Am { $when } von { $from } auf { $to } gewechselt.
settings-launcher-checking = Prüfe …
settings-launcher-check-now = Jetzt prüfen
settings-launcher-download = Jetzt herunterladen
settings-launcher-roll-back = Zur vorherigen Version zurück

settings-notify-updates = Benachrichtige mich, wenn Updates verfügbar sind
settings-notify-finished = Benachrichtige mich, wenn eine Installation oder ein Update fertig ist
settings-notify-note = Benachrichtigungen erscheinen nur, solange der Launcher im Hintergrund läuft. Dein System fragt beim ersten Mal womöglich nach Erlaubnis.
settings-notify-test = Testbenachrichtigung senden

settings-proxy = Proxy
    .description = Automatisch nutzt die Proxy-Einstellungen deines Systems, einschließlich der üblichen Umgebungsvariablen.
settings-proxy-auto = Automatisch (System)
settings-proxy-none = Kein Proxy
settings-proxy-custom = Eigene Adresse
settings-proxy-hint = http://, https:// oder socks5://. Lass Benutzername und Passwort weg: Sie würden ungeschützt gespeichert.
settings-proxy-use = Diesen Proxy verwenden
settings-proxy-problem-empty = Gib eine Adresse ein, etwa http://proxy.example:8080.
settings-proxy-problem-address = Das ist keine gültige Adresse.
settings-proxy-problem-scheme = Verwende eine Adresse mit http://, https:// oder socks5://.
settings-proxy-problem-credentials = Lass Benutzername und Passwort weg. Sie würden ungeschützt gespeichert.
settings-timeout = Zeitlimit für Anfragen
    .description = Wie lange auf einen Server gewartet wird, bevor aufgegeben wird. Langsame Verbindungen brauchen eventuell länger.
settings-seconds = { $count ->
    [one] { $count } Sekunde
   *[other] { $count } Sekunden
}
settings-github = GitHub-Zugriff
settings-github-intro = GitHub erlaubt jedem 60 Abfragen pro Stunde, mit einem kostenlosen Token 5.000. Der Launcher merkt sich, was er schon gefragt hat, daher kosten die meisten Prüfungen nichts. Füge einen Token nur hinzu, wenn du viele Apps hast und trotzdem an das Limit stößt. Ein Token ohne Berechtigungen genügt, denn alles, was der Launcher liest, ist öffentlich.
settings-token-stored = Ein Token ist im Anmeldespeicher deines Systems gespeichert.
settings-token-remove = Token entfernen
settings-token-environment = Ein Token stammt aus der Einstellung { $name } in deiner Umgebung.
settings-token-unavailable = Der Anmeldespeicher deines Systems ist nicht verfügbar, daher kann ein Token nicht sicher aufbewahrt werden.
settings-token-paste = Token einfügen
settings-token-save = Token speichern
settings-rate-check = Mein Limit prüfen
settings-rate-left = Noch { $remaining } von { $limit } Abfragen übrig. Zurückgesetzt { $resets }. { $note }
settings-rate-token = Dein Token wird verwendet.
settings-rate-anonymous = Nicht angemeldet.

settings-measuring = Messe …
settings-show-folder = Im Dateimanager anzeigen
settings-storage-apps = Deine Apps
    .description = Alles, was der Launcher installiert hat. Deinstalliere eine App, um ihren Platz freizugeben.
settings-storage-leftover = Übrig gebliebene Downloads
    .description = Dateien von abgebrochenen Installationen. Sie können gefahrlos entfernt werden; Laufendes bleibt unberührt.
settings-clear-leftovers = Reste löschen
settings-storage-cache = Gemerkte Abfragen
    .description = Antworten von GitHub, die Prüfungen schneller und günstiger machen. Sie werden bei Bedarf neu geholt.
settings-clear-cache = Gemerkte Abfragen löschen
settings-storage-data = Launcher-Daten
    .description = Die eigenen Daten des Launchers: was installiert ist, Einstellungen und Verlauf.

settings-support-copy = Support-Bericht kopieren
settings-support-note = Der Bericht enthält nur Versionen und Anzahlen. Keine Dateipfade, Links oder App-Namen.

settings-about-by = { $name } von Replicraft
settings-about-version = Version { $version }
settings-about-platform = Plattform
settings-about-license = Lizenz
settings-about-website = Website
settings-about-source = Quellcode auf GitHub ansehen
settings-about-restore = Standardwerte wiederherstellen
