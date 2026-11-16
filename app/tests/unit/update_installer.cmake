# SPDX-License-Identifier: MIT
# Updates (mitcad#603): the private download folder on every platform, the
# Windows updater's command line and its SHA-256 check of the installer:
# plain Qt Core and plain C++.
target_sources(test_app PRIVATE tests/update_installer.cpp update/UpdateInstaller.cpp update/UpdateInstaller.hpp
               update/UpdaterChecks.hpp)
# The start-up check's comparison of the OCCT build stamp (mitcad#1454):
# plain C++.
target_sources(test_app PRIVATE update/BuildStamp.hpp)
if(WIN32)
  # processHasAdministratorRights (mitcad#921).
  target_link_libraries(test_app PRIVATE advapi32)
endif()
