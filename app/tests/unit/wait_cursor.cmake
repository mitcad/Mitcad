# SPDX-License-Identifier: MIT
# The wait cursor: nested guards, the progress dialog's release, the
# log lines, the idle recovery (mitcad#268): plain Qt Widgets.
target_sources(test_app PRIVATE tests/wait_cursor.cpp framework/WaitCursor.cpp framework/WaitCursor.hpp)
