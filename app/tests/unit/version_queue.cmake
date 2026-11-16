# SPDX-License-Identifier: MIT
# The queue's order, a command run aside, the state's words: plain Qt Core.
target_sources(test_app PRIVATE tests/version_queue.cpp files/VersionQueue.cpp files/VersionQueue.hpp
               files/SyncState.cpp files/SyncState.hpp)
