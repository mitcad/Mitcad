# SPDX-License-Identifier: MIT
# A `..` path, a symbolic link, a name that differs in case: plain Qt Core.
target_sources(test_app PRIVATE tests/save_component_target.cpp files/SaveComponentTarget.cpp
               files/SaveComponentTarget.hpp)
