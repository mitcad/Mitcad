# SPDX-License-Identifier: MIT
# The Browser's background in the Floating layout (mitcad#419, mitcad#2144):
# its default, a saved choice, Restore Defaults and a card that follows it.
# Qt Widgets on the offscreen platform; the panel, the card and the chrome
# style it needs come with tests/unit/browser_roles.cmake.
target_sources(test_app PRIVATE tests/browser_background.cpp browser/BrowserBackground.cpp
               browser/BrowserBackground.hpp)
