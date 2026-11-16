# SPDX-License-Identifier: MIT
# Rounds superseded, waiting, stopped, off the UI thread: plain Qt Core
# and the geometry's cancel.
target_sources(test_app PRIVATE tests/section_worker.cpp framework/SectionWorker.cpp
               framework/SectionWorker.hpp)
