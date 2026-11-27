# SPDX-License-Identifier: MIT
# Parameters in a drawing's texts (mitcad#2483): the references a text has
# and those the design lacks, and Insert Parameter's list: plain Qt Core.
target_sources(test_app PRIVATE tests/parameter_references.cpp drawings/ParameterReferences.cpp
                                drawings/ParameterReferences.hpp)
