# SPDX-License-Identifier: MIT
# An .iam assembly's import choice, progress lines and parts' outcomes
# (mitcad#507): plain Qt Core.
target_sources(test_app PRIVATE tests/iam_import.cpp files/IamImport.cpp files/IamImport.hpp)
