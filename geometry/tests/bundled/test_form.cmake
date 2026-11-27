# SPDX-License-Identifier: MIT
# Form bodies (mitcad#1757): six bulged bicubic patches of a cube sewn into
# a valid solid with its faces named, a fillet on one of its edges and a
# hole through it; five of them a sheet; a missing one and a broken patch
# refused.
target_sources(test_geometry_sheet PRIVATE tests/test_form.cpp)
add_test(NAME geometry.form COMMAND test_geometry_sheet form)
