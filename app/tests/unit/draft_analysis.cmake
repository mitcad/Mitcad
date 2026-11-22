# SPDX-License-Identifier: MIT
# Draft Analysis (mitcad#2166): the definition, the query, the result area's
# words, the log line and the overlay's three classes: plain Qt Core and Gui.
target_sources(test_app PRIVATE tests/draft_analysis.cpp commands/DraftAnalysis.cpp
               commands/DraftAnalysis.hpp)
