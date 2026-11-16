# SPDX-License-Identifier: MIT
# The debug log's files: rotation, the folder's limits, the header, the
# command line (mitcad#269): plain Qt Core and the log ring.
target_sources(test_app PRIVATE tests/debug_log.cpp framework/DebugLog.cpp framework/DebugLog.hpp
               mcp/LogRing.cpp mcp/LogRing.hpp)
