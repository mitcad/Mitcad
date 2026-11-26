# SPDX-License-Identifier: MIT
# A command panel's line of a list of values (mitcad#2421): one line, each
# value on a line of its own, each field under its label; the fields'
# widths and their notes' heights. Qt Widgets on the offscreen platform.
target_sources(test_app PRIVATE tests/list_line_layout.cpp framework/ListLineLayout.cpp framework/ListLineLayout.hpp)
