# SPDX-License-Identifier: MIT
# A pop-up list's highlighted row as the accent's inset band, not the
# system's square highlight (mitcad#2084): Qt Widgets on the offscreen
# platform. IconSizeStyle (icon_size_style.cmake), the band's painter in
# WindowStyle (browser_roles.cmake) and the theme's colours (Theme.cpp) are
# in test_app already.
target_sources(test_app PRIVATE tests/popup_list_rows.cpp)
