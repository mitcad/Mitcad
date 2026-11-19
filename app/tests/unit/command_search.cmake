# SPDX-License-Identifier: MIT
# The list's order, its hint and the row selected: plain Qt Core (the
# place line reads the command layout, mitcad#1983).
target_sources(test_app PRIVATE tests/command_search.cpp framework/CommandSearchRank.cpp
               framework/CommandSearchRank.hpp framework/CommandLayout.cpp framework/CommandLayout.hpp)
