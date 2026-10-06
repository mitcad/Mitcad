; SPDX-License-Identifier: MIT
; The .mitcad file type for the NSIS installer (cmake/Packaging.cmake). The
; commands live in a file of their own: CPack mangles double quotes in its
; CPACK_NSIS_EXTRA_*_COMMANDS variables.

!macro MitcadFileTypeRegister
  WriteRegStr SHCTX "Software\Classes\.mitcad" "" "Mitcad.Project"
  WriteRegStr SHCTX "Software\Classes\Mitcad.Project" "" "Mitcad project"
  WriteRegStr SHCTX "Software\Classes\Mitcad.Project\shell\open\command" "" \
    '"$INSTDIR\bin\mitcad.exe" --open "%1"'
  ; Tell the shell that file associations changed.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro MitcadFileTypeUnregister
  DeleteRegKey SHCTX "Software\Classes\.mitcad"
  DeleteRegKey SHCTX "Software\Classes\Mitcad.Project"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
