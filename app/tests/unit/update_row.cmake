# SPDX-License-Identifier: MIT
# Two windows' bars of one row: Qt Widgets on the offscreen platform
# (framework/TestSync.cpp, NoticeLevel.cpp, UiSizes.cpp and Icons.cpp are
# in test_app already).
target_sources(test_app PRIVATE tests/update_row.cpp update/UpdateRow.cpp update/UpdateRow.hpp
               update/UpdateNotice.cpp update/UpdateNotice.hpp)
