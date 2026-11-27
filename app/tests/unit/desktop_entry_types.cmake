# SPDX-License-Identifier: MIT
# The packaging's file types against fileKind() (mitcad#2527): the Linux
# desktop entry, its MIME package and the Windows installer's Open with
# list, read from the source tree. Plain Qt Core.
target_sources(test_app PRIVATE tests/desktop_entry_types.cpp framework/DesktopEntry.cpp framework/DesktopEntry.hpp
                                files/FileFormats.cpp files/FileFormats.hpp)
set_source_files_properties(tests/desktop_entry_types.cpp PROPERTIES
  COMPILE_DEFINITIONS "MITCAD_PACKAGING_DIR=\"${PROJECT_SOURCE_DIR}/packaging\"")
