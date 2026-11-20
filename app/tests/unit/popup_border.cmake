# SPDX-License-Identifier: MIT
# The popups' border (mitcad#2081): a menu's, a combo box's drop-down
# list's and a completer's frame under Qt's portable style drawn in the
# PopupBorder role, at least 3:1 against the window on dark and light
# palettes (IconSizeStyle.cpp is in test_app through icon_size_style.cmake,
# Theme.cpp already).
target_sources(test_app PRIVATE tests/popup_border.cpp)
