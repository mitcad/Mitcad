# SPDX-License-Identifier: MIT
# Solid-mode profile groups share the actual selection and an independent hover.
target_sources(test_app PRIVATE tests/autoselect.cpp framework/AutoselectPanel.cpp framework/AutoselectPanel.hpp)
