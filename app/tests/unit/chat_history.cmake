# SPDX-License-Identifier: MIT
# The history files (keys, the end read, moves, clearing) and the chat's
# settings: plain Qt Core.
target_sources(test_app PRIVATE tests/chat_history.cpp agent/ChatHistoryStore.cpp agent/ChatHistoryStore.hpp
               agent/ChatSettings.cpp agent/ChatSettings.hpp)
