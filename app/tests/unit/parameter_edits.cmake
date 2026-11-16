# SPDX-License-Identifier: MIT
# What the table leaves to an open panel, which of its changes the panel
# follows, a rename in the panel's expressions: plain Qt Core.
target_sources(test_app PRIVATE tests/parameter_edits.cpp framework/ParameterEdits.cpp
               framework/ParameterEdits.hpp)
