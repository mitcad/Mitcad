# SPDX-License-Identifier: MIT
# A rounding wider than the face beside its edge whose far edge goes on
# tangentially into a round face (GitHub #560, mitcad#2208): the ball
# takes the narrow face away and rolls on over the round face, with and
# without a cut corner it ends on, with the cut past the narrow face, and
# after another fillet; one valid solid without a note, the volume added
# exact.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_narrow_face_tangent_beyond.cpp)
add_test(NAME geometry.fillet_narrow_face_tangent_beyond COMMAND test_geometry_fillet_f_n fillet_narrow_face_tangent_beyond)
set_tests_properties(geometry.fillet_narrow_face_tangent_beyond PROPERTIES TIMEOUT 300)
