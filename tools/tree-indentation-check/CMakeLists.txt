# SPDX-License-Identifier: MIT
# Keeps every list and tree on the app's one compact indentation
# (docs/ui-conventions.md, "Lists and trees"; mitcad#1214): no QTreeView or
# QTreeWidget in app/ made without panel::setCompactIndentation, no
# indentation of its own.
add_executable(mitcad-check-tree-indentation check-tree-indentation.cpp)
mitcad_utf8_code_page(mitcad-check-tree-indentation)
target_link_libraries(mitcad-check-tree-indentation PRIVATE mitcad_warnings)

add_test(NAME repo.tree-indentation COMMAND mitcad-check-tree-indentation "${PROJECT_SOURCE_DIR}/app")
# The scan finds the views planted in it without the helper, and only those.
add_test(NAME repo.tree-indentation-self-test COMMAND mitcad-check-tree-indentation --self-test)
