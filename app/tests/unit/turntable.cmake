# SPDX-License-Identifier: MIT
# Export Turntable (mitcad#2523): the frames' cameras stepping 10 degrees in
# a 36-frame turn, the FFmpeg encoder and arguments, and the video's frame
# count and size read from MP4 and WebM containers.
target_sources(test_app PRIVATE tests/turntable.cpp view/Turntable.cpp view/Turntable.hpp)
target_link_libraries(test_app PRIVATE TKService)
