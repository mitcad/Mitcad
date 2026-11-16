# SPDX-License-Identifier: MIT
# Check again's decisions after the agent's check (mitcad#1155).
target_sources(test_app PRIVATE tests/chat_account_check.cpp agent/AccountCheck.hpp)
# Sign in Again's commands, scripts and terminals per platform (mitcad#1585).
target_sources(test_app PRIVATE agent/SignInCommands.hpp)
