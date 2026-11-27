# SPDX-License-Identifier: MIT
# Minimum Distance (mitcad#2519): the picks of a selection, the definition,
# the query, the result area's words, the log line and the overlay's line:
# plain Qt Core and Gui.
target_sources(test_app PRIVATE tests/minimum_distance.cpp commands/MinimumDistance.cpp
               commands/MinimumDistance.hpp commands/PrintAnalysis.cpp commands/PrintAnalysis.hpp)
