# SPDX-License-Identifier: MIT
# The active component's highlight in the Browser (mitcad#2618): the
# preference, its settings transfer group, the tint's contrast with the rows'
# names in both themes and a panel's active headers tinted in the Docked, the
# Floating Card and the Floating Rows layout. Qt Widgets on the offscreen
# platform; the preferences and the settings transfer come with
# tests/unit/browser_control_places.cmake, the panel, the chrome style and
# the panels' row style with tests/unit/browser_roles.cmake.
target_sources(test_app PRIVATE tests/browser_active_highlight.cpp)
