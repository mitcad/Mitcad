# SPDX-License-Identifier: MIT
# The Browser rows' Activate radio button and Show eye places (mitcad#2223):
# the two preferences, their settings transfer group and a panel's rows laid
# out with Classic and Right. Qt Widgets on the offscreen platform; the
# panel, the card and the chrome style it needs come with
# tests/unit/browser_roles.cmake.
target_sources(test_app PRIVATE tests/browser_control_places.cpp browser/BrowserPreferences.cpp
               browser/BrowserPreferences.hpp framework/SettingsTransfer.cpp framework/SettingsTransfer.hpp)
