# SPDX-License-Identifier: MIT
# The visible-text guard's identifiers, rich text and allow-list
# (mitcad#1741), and the selection card's words for sketch items.
target_sources(test_app PRIVATE tests/visible_text.cpp framework/VisibleTextGuard.cpp
                                framework/VisibleTextGuard.hpp)
