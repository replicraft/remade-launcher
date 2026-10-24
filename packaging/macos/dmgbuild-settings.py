# Layout for the disk image, read by `dmgbuild`.
#
# dmgbuild writes the window layout straight into the image's .DS_Store, so
# unlike Finder scripting it needs no desktop session and no Automation
# permission, which also makes it work in CI.
#
# The build script passes the app and the background through `-D`.

import os

application = defines["app"]  # noqa: F821  (provided by dmgbuild)
background_image = defines["background"]  # noqa: F821
volume_icon = defines["icon"]  # noqa: F821

app_name = os.path.basename(application)

format = "UDZO"
filesystem = "HFS+"

files = [application]
symlinks = {"Applications": "/Applications"}

icon = volume_icon
background = background_image

# Window and icons, in points. The arrow in the background sits between the
# two icons, on the same row.
window_rect = ((200, 120), (660, 400))
icon_size = 112
text_size = 13
icon_locations = {
	app_name: (165, 200),
	"Applications": (495, 200),
}

default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
