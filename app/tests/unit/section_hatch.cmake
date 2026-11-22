# SPDX-License-Identifier: MIT
# The hatch over Section Analysis' caps in the section plane (mitcad#2146):
# OCCT's faces, meshing and classification, no view.
target_sources(test_app PRIVATE tests/section_hatch.cpp view/SectionHatch.cpp view/SectionHatch.hpp)
target_link_libraries(test_app PRIVATE TKMesh TKTopAlgo)
