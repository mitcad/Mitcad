# SPDX-License-Identifier: MIT
# The tools' definitions and the commands they make: plain Qt Core.
target_sources(test_app PRIVATE tests/drawing_tools.cpp drawings/DrawingTools.cpp drawings/DrawingTools.hpp
                                drawings/DrawingTemplates.cpp drawings/DrawingTemplates.hpp)
