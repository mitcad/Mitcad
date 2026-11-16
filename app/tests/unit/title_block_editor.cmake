# SPDX-License-Identifier: MIT
# The editor's lines, fields and images, their snaps, moves and deletes
# and the block it gives back: plain Qt Core (the sheet's view paints
# it).
target_sources(test_app PRIVATE tests/title_block_editor.cpp drawings/TitleBlockEditor.cpp
                                drawings/TitleBlockEditor.hpp)
