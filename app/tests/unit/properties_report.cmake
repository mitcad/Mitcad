# SPDX-License-Identifier: MIT
# Names, materials, merged rows, the summary, units and the copied text:
# plain Qt Core (framework/LengthUnits.cpp is in test_app already).
target_sources(test_app PRIVATE tests/properties_report.cpp commands/PropertiesReport.cpp
               commands/PropertiesReport.hpp)
