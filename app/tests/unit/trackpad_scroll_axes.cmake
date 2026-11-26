# SPDX-License-Identifier: MIT
# Alt + a two-finger trackpad scroll keeps the fingers' axes on xcb and
# Wayland (GitHub mitcad/Mitcad#381, #662; view/Navigation.hpp).
target_sources(test_app PRIVATE tests/trackpad_scroll_axes.cpp)
