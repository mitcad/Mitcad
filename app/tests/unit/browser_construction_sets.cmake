# SPDX-License-Identifier: MIT
# The Browser's construction sets (mitcad#2514): a component's sets before
# its Bodies folder, their items' own rows under them and out of their
# usual folders, the active set in bold, built from a snapshot
# (BrowserPanel, whose sources browser_roles.cmake gives test_app; the
# snapshot's lookups, DocumentSnapshot from document_snapshot.cmake).
target_sources(test_app PRIVATE tests/browser_construction_sets.cpp)
