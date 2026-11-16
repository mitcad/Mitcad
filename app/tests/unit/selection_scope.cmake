# SPDX-License-Identifier: MIT
# Sketch selection filters' three states, Shift and edge sight (mitcad#217):
# plain Qt Core.
target_sources(test_app PRIVATE tests/selection_scope.cpp framework/SelectionScope.cpp
               framework/SelectionScope.hpp)
