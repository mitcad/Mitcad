# SPDX-License-Identifier: MIT
# Move to... on an occurrence: the parent picker and cancellation
# (mitcad#219): modal Qt Widgets.
target_sources(test_app PRIVATE tests/move_occurrence.cpp framework/MoveOccurrenceDialog.cpp)
