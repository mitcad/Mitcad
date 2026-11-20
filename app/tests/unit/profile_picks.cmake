# SPDX-License-Identifier: MIT
# What Extrude's and Revolve's Profiles take with a click (mitcad#868), and
# the profile the profile commands take as they start (mitcad#2087): plain
# Qt Core and the command context's interface.
target_sources(test_app PRIVATE tests/profile_picks.cpp commands/ProfilePicks.cpp commands/ProfilePicks.hpp
               framework/Command.cpp framework/Command.hpp)
