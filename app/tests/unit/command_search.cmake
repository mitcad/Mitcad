# SPDX-License-Identifier: MIT
# The list's order, its hint and the row selected: plain Qt Core.
target_sources(test_app PRIVATE tests/command_search.cpp framework/CommandSearchRank.cpp
               framework/CommandSearchRank.hpp)
