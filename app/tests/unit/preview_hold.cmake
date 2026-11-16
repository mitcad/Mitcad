# SPDX-License-Identifier: MIT
# When a held Ctrl hides and shows it, from synthetic events: plain Qt
# Widgets.
target_sources(test_app PRIVATE tests/preview_hold.cpp framework/PreviewHold.cpp framework/PreviewHold.hpp)
