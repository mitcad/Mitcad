# SPDX-License-Identifier: MIT
# A nozzle on a dome on a tube whose foot crosses the line where the dome
# runs on tangentially into the tube (GitHub #582, mitcad#2272): the foot
# rounded at four angles about the axis by the kernel's fillet alone and by
# MitCad's, picked by its dome edge and by its tube edge, and a nozzle
# wholly on the dome; one valid closed solid at the exact size without a
# note, the material added the same everywhere (port patch 2272). `--probe`
# prints the foot's vertices.
target_sources(test_geometry_fillet_f_n PRIVATE tests/test_fillet_nozzle_dome_junction.cpp)
add_test(NAME geometry.fillet_nozzle_dome_junction COMMAND test_geometry_fillet_f_n fillet_nozzle_dome_junction)
set_tests_properties(geometry.fillet_nozzle_dome_junction PROPERTIES TIMEOUT 300)
