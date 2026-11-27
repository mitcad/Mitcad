# SPDX-License-Identifier: MIT
# Surface quality analyses (mitcad#2517): the definitions, the query, the
# result area's words, the log lines, the overlay's stripes, colors, curves
# and mirrored faces and the studio picture: plain Qt Core and Gui.
target_sources(test_app PRIVATE tests/surface_analysis.cpp commands/SurfaceAnalysis.cpp
               commands/SurfaceAnalysis.hpp commands/PrintAnalysis.cpp commands/PrintAnalysis.hpp)
