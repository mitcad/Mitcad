# SPDX-License-Identifier: MIT
# The Browser's Selection Sets folder (mitcad#2192): its rows built from a
# snapshot (BrowserPanel, whose sources browser_roles.cmake gives test_app)
# and the members a set takes of a selection and of rows
# (browser/SelectionSets, plain Qt Core over BrowserNode).
target_sources(test_app PRIVATE tests/browser_selection_sets.cpp browser/SelectionSets.cpp
               browser/SelectionSets.hpp)
