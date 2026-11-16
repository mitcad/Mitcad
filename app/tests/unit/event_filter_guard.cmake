# SPDX-License-Identifier: MIT
# Event filters on the 3D view when the main window closes (mitcad#226):
# plain Qt Widgets.
target_sources(test_app PRIVATE tests/event_filter_guard.cpp framework/EventFilterGuard.hpp)
