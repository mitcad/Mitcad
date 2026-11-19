# SPDX-License-Identifier: MIT
# A group's default buttons removed from the toolbar and shown again, and
# the tab's groups reordered, in the command layout's user layer
# (mitcad#1991): plain Qt Core.
target_sources(test_app PRIVATE tests/toolbar_buttons.cpp framework/CommandLayout.cpp framework/CommandLayout.hpp)
