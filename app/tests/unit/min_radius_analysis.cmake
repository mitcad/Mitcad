# SPDX-License-Identifier: MIT
# Minimum Radius (mitcad#2518): the definition, the query, the result area's
# words, the log line and the overlay's marker and faces under the limit:
# plain Qt Core and Gui.
target_sources(test_app PRIVATE tests/min_radius_analysis.cpp commands/MinRadiusAnalysis.cpp
               commands/MinRadiusAnalysis.hpp)
