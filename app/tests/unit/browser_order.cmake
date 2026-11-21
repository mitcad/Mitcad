# SPDX-License-Identifier: MIT
# Folders of the Browser's component and body rows (mitcad#2103): the
# helpers of browser/BrowserOrder (plain Qt Core over a DocumentSnapshot,
# whose sources document_snapshot.cmake gives test_app) and where dragged
# rows may go (BrowserPanel::dropFor, whose sources browser_roles.cmake
# gives it).
target_sources(test_app PRIVATE tests/browser_order.cpp browser/BrowserOrder.cpp browser/BrowserOrder.hpp)
