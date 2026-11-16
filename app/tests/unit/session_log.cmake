# SPDX-License-Identifier: MIT
# The files' names, the rotation and the folder's limits, the header and
# the lines: plain Qt Core and the log ring (framework/DebugLog.cpp and
# mcp/LogRing.cpp are in test_app already).
target_sources(test_app PRIVATE tests/session_log.cpp framework/SessionLog.cpp framework/SessionLog.hpp)
