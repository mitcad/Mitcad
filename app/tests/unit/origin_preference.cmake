# SPDX-License-Identifier: MIT
# The settings' load and save, and the shapes each style draws: Qt Core
# and OCCT's shape building.
target_sources(test_app PRIVATE tests/origin_preference.cpp view/OriginPreference.cpp view/OriginPreference.hpp)
target_link_libraries(test_app PRIVATE TKTopAlgo TKBRep)
