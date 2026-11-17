# SPDX-License-Identifier: MIT
# Flex (mitcad#1713): the route's nodes and ids, the end port a click takes,
# the item of the panel and its origin: Qt Core and OCCT's math.
target_sources(test_app PRIVATE tests/flex_route.cpp commands/FlexRoute.cpp commands/FlexRoute.hpp)
