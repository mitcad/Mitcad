# SPDX-License-Identifier: MIT
# The browser's keys (mitcad#1840): AltGr's characters typed, Control's
# shortcuts not, Option's characters on macOS not system keys.
target_sources(test_app PRIVATE tests/browser_keys.cpp webbrowser/BrowserKeys.cpp webbrowser/BrowserKeys.hpp)
