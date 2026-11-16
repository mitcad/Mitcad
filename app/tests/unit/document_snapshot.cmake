# SPDX-License-Identifier: MIT
# The document snapshot's feature lookup by uid (mitcad#833): read() from a
# model's answers, the command context's interface.
target_sources(test_app PRIVATE tests/document_snapshot.cpp browser/DocumentSnapshot.cpp
               browser/DocumentSnapshot.hpp)
