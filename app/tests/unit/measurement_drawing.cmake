# SPDX-License-Identifier: MIT
# The query's entries to dimensions, their layout through a projection and
# at a picture's scale, painting, the values in words, a render camera's
# projection: plain Qt Gui and OCCT's points.
target_sources(test_app PRIVATE tests/measurement_drawing.cpp view/MeasurementDrawing.cpp
               view/MeasurementDrawing.hpp)
