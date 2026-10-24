# Settings: language and region.

settings-language = Language
    .description = The language of the interface. Automatic follows your system ({ $system }).
settings-language-auto = Automatic
settings-time-format = Time format
    .description = Automatic follows your system's region.
time-format-system = System
time-format-12h = 12-hour (3:42 PM)
time-format-24h = 24-hour (15:42)
settings-date-format = Date format
    .description = Automatic follows your system's region.
date-format-system = System
date-format-mdy = Oct 24, 2026
date-format-dmy = 24 Oct 2026
date-format-iso = 2026-10-24
settings-region-example = Example: { $today }   ·   { $earlier }

# Settings: panels and the rest.

settings-title = Settings
settings-appearance = Appearance
settings-region-panel = Language and region
settings-updates-panel = Updates
    .description = How installed apps are updated. The Launcher updates itself separately, below.
settings-launcher-panel = Launcher updates
    .description = How the Launcher keeps itself up to date. This does not change how your apps are updated.
settings-notifications-panel = Notifications
settings-network-panel = Network
settings-storage-panel = Storage
settings-support-panel = Support
settings-about-panel = About

settings-theme = Theme
theme-system = System
theme-light = Light
theme-dark = Dark
settings-scale = Interface scale
    .description = Makes everything larger or smaller. Useful on high-resolution screens.
settings-start-page = Open on
    .description = The page the launcher shows when it starts.
start-page-auto = Library if I have apps, otherwise Explore
start-page-library = Library
start-page-explore = Explore
start-page-last-used = Where I left off
settings-layout-note = Your layout, such as the folded sidebar, grid or list, and window size, is remembered automatically.

update-mode-manual = Manual updates (default)
    .description = Checks for updates when you open the Launcher. Installed apps show an Update button so you decide when to install.
update-mode-on-launch = Auto-update when the Launcher starts
    .description = Each time you start the Launcher, it installs eligible app updates automatically. It does not keep running or update apps in the background after you close it.
update-mode-daemon = Auto-update with an always-active background updater (dangerous)
    .description = Starts a lightweight background updater when you sign in. It checks on startup and every 24 hours while active, and also when you open the Launcher. Updates are installed without individual confirmation. Malicious, compromised, or broken upstream releases could be installed automatically and may damage software or data.
settings-min-age = Default minimum release age for auto-updates
    .automatic = A release must be at least this old before an automatic update installs it. Each app can use its own.
    .manual = Used only by the automatic modes. It never holds back an update you start yourself.
minimum-age-0 = No delay
minimum-age-3600 = 1 hour
minimum-age-21600 = 6 hours
minimum-age-43200 = 12 hours
minimum-age-86400 = 24 hours
minimum-age-259200 = 3 days
minimum-age-604800 = 7 days
settings-min-age-warning = No delay installs a release the moment it is published. That is riskier.
settings-background = Background updater
settings-background-checking = Checking…
settings-background-active = Active
settings-background-stopped = Stopped
    .detail = It will start when you sign in.
settings-background-pending = Pending OS approval
    .detail = Approve it in your system's login items.
settings-background-disabled = Disabled by OS
    .detail = You turned it off in system settings. The Launcher will not turn it back on.
settings-background-not-installed = Not installed
settings-background-failed = Failed
settings-background-unavailable = Unavailable here
settings-background-last-check = Last check: { $when }
settings-background-last-result = Last result: { $outcome }
settings-background-next-check = Next check: { $when }
settings-background-stop = Stop background updates

launcher-updates-automatic = Automatic
    .description = Download a new launcher in the background and switch to it when you close this one.
launcher-updates-notify = Tell me
    .description = Say when a new launcher is available, and leave the rest to you.
launcher-updates-off = Off
    .description = Never look for a new launcher.
settings-launcher-last-checked = Last checked { $when }
settings-launcher-staged = { $version } is downloaded. It starts the next time you open the launcher.
settings-launcher-available = { $version } is available.
settings-launcher-applied = Switched from { $from } to { $to } on { $when }.
settings-launcher-checking = Checking…
settings-launcher-check-now = Check now
settings-launcher-download = Download now
settings-launcher-roll-back = Go back to the previous version

settings-notify-updates = Tell me when updates are available
settings-notify-finished = Tell me when an install or update finishes
settings-notify-note = Notifications appear only while the launcher is in the background. Your system may ask permission the first time.
settings-notify-test = Send a test notification

settings-proxy = Proxy
    .description = Automatic uses your system's proxy settings, including the usual environment variables.
settings-proxy-auto = Automatic (system)
settings-proxy-none = No proxy
settings-proxy-custom = Custom address
settings-proxy-hint = http://, https://, or socks5://. Leave user names and passwords out: they would be saved unprotected.
settings-proxy-use = Use this proxy
settings-proxy-problem-empty = Enter an address, such as http://proxy.example:8080.
settings-proxy-problem-address = That is not a valid address.
settings-proxy-problem-scheme = Use an http://, https://, or socks5:// address.
settings-proxy-problem-credentials = Leave the user name and password out. They would be saved unprotected.
settings-timeout = Request timeout
    .description = How long to wait for a server before giving up. Slow connections may need longer.
settings-seconds = { $count ->
    [one] { $count } second
   *[other] { $count } seconds
}
settings-github = GitHub access
settings-github-intro = GitHub allows 60 lookups an hour to anyone, and 5,000 with a free token. The launcher remembers what it has already asked, so most checks cost nothing. Add a token only if you keep a lot of apps and still hit the limit. A token with no permissions is enough, because everything the launcher reads is public.
settings-token-stored = A token is saved in your system's credential store.
settings-token-remove = Remove token
settings-token-environment = A token comes from the { $name } setting in your environment.
settings-token-unavailable = Your system's credential store is not available, so a token cannot be kept safely.
settings-token-paste = Paste a token
settings-token-save = Save token
settings-rate-check = Check my limit
settings-rate-left = { $remaining } of { $limit } lookups left. Resets { $resets }. { $note }
settings-rate-token = Using your token.
settings-rate-anonymous = Not signed in.

settings-measuring = Measuring…
settings-show-folder = Show in file manager
settings-storage-apps = Your apps
    .description = Everything the launcher has installed. Uninstall an app to free its space.
settings-storage-leftover = Leftover downloads
    .description = Files from installs that were interrupted. Safe to remove; nothing running is touched.
settings-clear-leftovers = Clear leftovers
settings-storage-cache = Remembered lookups
    .description = Answers from GitHub kept to make checks faster and cheaper. They are refilled as needed.
settings-clear-cache = Clear remembered lookups
settings-storage-data = Launcher data
    .description = The launcher's own records: what is installed, settings, and history.

settings-support-copy = Copy support report
settings-support-note = The report lists versions and counts only. It has no file paths, links, or app names.

settings-about-by = { $name } by Replicraft
settings-about-version = Version { $version }
settings-about-platform = Platform
settings-about-license = License
settings-about-website = Website
settings-about-source = View the source on GitHub
settings-about-restore = Restore defaults
