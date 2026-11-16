# SPDX-License-Identifier: MIT
# The order with stars, the stars' settings, the used parts read from a
# model's answer, the groups' tree and the Supplier filter (mitcad#949):
# plain Qt Core and the command context's interface.
target_sources(test_app PRIVATE tests/part_picker_data.cpp
               library/PartFavourites.cpp library/PartFavourites.hpp
               library/PartGroups.cpp library/PartGroups.hpp
               library/PartSuppliers.cpp library/PartSuppliers.hpp
               library/UsedParts.cpp library/UsedParts.hpp)
