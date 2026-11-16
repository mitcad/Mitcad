# SPDX-License-Identifier: MIT
# The lifetimes by level, an error that stays until it is closed or the
# next command starts, a hint that does not push a problem away, the
# stub of the latest message and the history newest first: plain Qt Core.
target_sources(test_app PRIVATE tests/message_log.cpp framework/MessageLog.cpp framework/MessageLog.hpp)
