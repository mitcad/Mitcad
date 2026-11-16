# SPDX-License-Identifier: MIT
# The groups, the file written and read back, a file written on Windows
# (tests/data/windows.mitcad-settings: CRLF line ends, Windows paths), a
# newer version refused, the import's plan: plain Qt Core.
target_sources(test_app PRIVATE tests/settings_transfer.cpp framework/SettingsTransfer.cpp
               framework/SettingsTransfer.hpp)
qt_add_resources(test_app settings_transfer_fixture PREFIX /settings-transfer BASE tests/data
                 FILES tests/data/windows.mitcad-settings)
