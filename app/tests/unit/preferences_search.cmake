# SPDX-License-Identifier: MIT
# Preferences' search (mitcad#2074): the index made from a test page's
# widgets, the counts, the looks and the field's keys, Qt Widgets on the
# offscreen platform (the theme's SearchMatch from Theme.cpp, already in
# test_app).
target_sources(test_app PRIVATE tests/preferences_search.cpp framework/PreferencesSearch.cpp
               framework/PreferencesSearch.hpp)
