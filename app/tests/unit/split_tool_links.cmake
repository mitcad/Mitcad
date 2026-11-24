# SPDX-License-Identifier: MIT
# Split Body's tools of other components (mitcad#2299): `tool_links` written
# for a face or body of another component's occurrence, none for the split's
# own component, and a loaded split's picks back where its links saw them;
# plain Qt Core.
target_sources(test_app PRIVATE tests/split_tool_links.cpp commands/SplitToolLinks.cpp commands/SplitToolLinks.hpp
               framework/Selection.cpp framework/Selection.hpp)
