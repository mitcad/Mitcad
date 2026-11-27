# SPDX-License-Identifier: MIT
# Continuity Analysis (mitcad#2521): the query of the picks, the result
# area's words, the log line and the edges drawn by their classes: plain Qt
# and OCCT.
target_sources(test_app PRIVATE tests/continuity_analysis.cpp commands/ContinuityAnalysis.cpp
               commands/ContinuityAnalysis.hpp)
