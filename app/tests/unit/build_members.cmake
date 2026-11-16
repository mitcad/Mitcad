# SPDX-License-Identifier: MIT
# The adders read from build_parts and the command list: plain Qt Core.
target_sources(test_app PRIVATE tests/build_members.cpp browser/BuildMembers.cpp browser/BuildMembers.hpp)
