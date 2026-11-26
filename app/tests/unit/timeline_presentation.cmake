# SPDX-License-Identifier: MIT
# The timeline's History list (mitcad#2427): its preference (Preferences ›
# Display › Timeline), its settings transfer group and the bodies a History
# row's light bulb shows and hides (DocumentSnapshot::bodiesByFeature). The
# preferences, the settings transfer and the snapshot come with
# tests/unit/browser_control_places.cmake and tests/unit/document_snapshot.cmake.
target_sources(test_app PRIVATE tests/timeline_presentation.cpp)
