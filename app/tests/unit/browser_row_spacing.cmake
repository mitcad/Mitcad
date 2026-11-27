# SPDX-License-Identifier: MIT
# The Browser rows' spacing (mitcad#2513): the preference, its settings
# transfer group, the row height of each spacing and a panel's rows laid out
# again with each in the Floating and the Docked layout. Qt Widgets on the
# offscreen platform; the preferences and the settings transfer come with
# tests/unit/browser_control_places.cmake, the panel, the chrome style and
# the panels' row style with tests/unit/browser_roles.cmake.
target_sources(test_app PRIVATE tests/browser_row_spacing.cpp)
