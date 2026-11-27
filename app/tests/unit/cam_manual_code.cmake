# SPDX-License-Identifier: MIT
# A lathe setup's manual G-code in the setup panel (mitcad#2506): the block
# list loaded, moved, removed and written back, and where the Browser lists
# the blocks. Plain Qt Core; BrowserPanel.cpp in test_app calls it too.
target_sources(test_app PRIVATE tests/cam_manual_code.cpp cam/CamManualCode.cpp cam/CamManualCode.hpp
               framework/Command.cpp framework/Command.hpp)
