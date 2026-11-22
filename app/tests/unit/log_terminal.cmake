# SPDX-License-Identifier: MIT
# The log ring's stderr in a terminal (mitcad#2213): plain Qt Core and the
# log ring (mcp/LogRing.cpp is in test_app already, debug_log.cmake).
target_sources(test_app PRIVATE tests/log_terminal.cpp)
