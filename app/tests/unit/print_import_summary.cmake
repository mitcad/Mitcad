# SPDX-License-Identifier: MIT
# The import's line and report (plain Qt Core); the key table's layout
# and values, and the settings form's rows (Qt Widgets).
target_sources(test_app PRIVATE tests/print_import_summary.cpp tests/print_settings_form.cpp
               print/PrintImportSummary.cpp print/PrintImportSummary.hpp print/PrintKeys.cpp
               print/PrintKeys.hpp print/SettingField.cpp print/SettingField.hpp print/SettingsForm.cpp
               print/SettingsForm.hpp)
