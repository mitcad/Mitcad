# SPDX-License-Identifier: MIT
# The Rib's and Web's Draft Angle (mitcad#2102): the input after Thickness
# and `draft_angle` only when not 0, on a command state.
target_sources(test_app PRIVATE tests/rib_draft.cpp commands/RibDraft.cpp commands/RibDraft.hpp
               framework/Command.cpp framework/Command.hpp)
