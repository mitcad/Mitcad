# SPDX-License-Identifier: MIT
# The Open, Import and Insert Mesh filters against fileKind() (mitcad#2527):
# plain Qt Core.
target_sources(test_app PRIVATE tests/file_formats.cpp files/FileFormats.cpp files/FileFormats.hpp)
