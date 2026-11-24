# SPDX-License-Identifier: MIT
# The import progress's timeline item (mitcad#2331): the worker's lines of
# a design with several components read, the number shown never going back,
# the item's name and component in the text: plain Qt Core.
target_sources(test_app PRIVATE tests/import_item_progress.cpp files/ImportItemProgress.cpp
                                files/ImportItemProgress.hpp)
