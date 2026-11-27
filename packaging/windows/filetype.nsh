; SPDX-License-Identifier: MIT
; The .mitcad file type, and MitCad under Open with for the files it
; imports, for the NSIS installer (cmake/Packaging.cmake). The
; commands live in a file of their own: CPack mangles double quotes in its
; CPACK_NSIS_EXTRA_*_COMMANDS variables.
;
; In the scope of the installation: SHCTX is HKCU\Software\Classes for an
; installation just for the current user and HKLM\Software\Classes for one
; for all users, as the installer and the uninstaller set the shell context
; (packaging/windows/NSIS.template.in). A registration of the current user
; takes precedence over the one for all users, so a per-user installation
; opens .mitcad files for its user even when another one is installed for
; all users.

!macro MitcadFileTypeRegister
  WriteRegStr SHCTX "Software\Classes\.mitcad" "" "Mitcad.Project"
  WriteRegStr SHCTX "Software\Classes\Mitcad.Project" "" "MitCad design"
  WriteRegStr SHCTX "Software\Classes\Mitcad.Project\shell\open\command" "" \
    '"$INSTDIR\bin\mitcad.exe" --open "%1"'
  ; The files MitCad imports under Open with (below).
  WriteRegStr SHCTX "Software\Classes\Mitcad.Import" "" "Model for MitCad"
  WriteRegStr SHCTX "Software\Classes\Mitcad.Import\shell\open\command" "" \
    '"$INSTDIR\bin\mitcad.exe" --open "%1"'
  !insertmacro MitcadImportExtensions MitcadOpenWithRegister
  ; Explorer's thumbnails of designs (below).
  !insertmacro MitcadThumbnailsRegister
  ; Tell the shell that file associations changed.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro MitcadFileTypeUnregister
  !insertmacro MitcadThumbnailsUnregister
  !insertmacro MitcadThumbnailsMoveAside
  !insertmacro MitcadImportExtensions MitcadOpenWithUnregister
  DeleteRegKey SHCTX "Software\Classes\Mitcad.Import"
  DeleteRegKey SHCTX "Software\Classes\.mitcad"
  DeleteRegKey SHCTX "Software\Classes\Mitcad.Project"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

; The files MitCad imports (mitcad#2527): offered under Explorer's Open
; with, not made theirs. Each extension's OpenWithProgids names the ProgID
; Mitcad.Import, which opens the file as Mitcad.Project's command does
; (app/main.cpp: --open imports a file of another format). The extension's
; own default and the other programs' entries stay as they are: the
; uninstaller removes only its value, and leaves the OpenWithProgids key,
; which other programs may share. The list is every extension of
; app/files/FileFormats.cpp but .mitcad (app.unit desktop_entry_types).
; tools/installer-windows-test.ps1 reads the list below, one
; "!insertmacro ${OPERATION} <extension>" line each, and checks every
; value after the install and the uninstall (mitcad#2555).
!macro MitcadOpenWithRegister EXT
  WriteRegStr SHCTX "Software\Classes\.${EXT}\OpenWithProgids" "Mitcad.Import" ""
!macroend

!macro MitcadOpenWithUnregister EXT
  DeleteRegValue SHCTX "Software\Classes\.${EXT}\OpenWithProgids" "Mitcad.Import"
!macroend

!macro MitcadImportExtensions OPERATION
  !insertmacro ${OPERATION} f3d
  !insertmacro ${OPERATION} f3z
  !insertmacro ${OPERATION} fcstd
  !insertmacro ${OPERATION} step
  !insertmacro ${OPERATION} stp
  !insertmacro ${OPERATION} iges
  !insertmacro ${OPERATION} igs
  !insertmacro ${OPERATION} brep
  !insertmacro ${OPERATION} brp
  !insertmacro ${OPERATION} serp
  !insertmacro ${OPERATION} stl
  !insertmacro ${OPERATION} obj
  !insertmacro ${OPERATION} 3mf
  !insertmacro ${OPERATION} dxf
  !insertmacro ${OPERATION} svg
  !insertmacro ${OPERATION} ipt
  !insertmacro ${OPERATION} iam
  !insertmacro ${OPERATION} ifc
  !insertmacro ${OPERATION} ifczip
  !insertmacro ${OPERATION} rvt
  !insertmacro ${OPERATION} rfa
  !insertmacro ${OPERATION} rte
  !insertmacro ${OPERATION} rft
  !insertmacro ${OPERATION} sc
  !insertmacro ${OPERATION} solidcraft
  !insertmacro ${OPERATION} solvecraft
  !insertmacro ${OPERATION} dwg
  !insertmacro ${OPERATION} 3dm
  !insertmacro ${OPERATION} glb
  !insertmacro ${OPERATION} gltf
!macroend

; Explorer's thumbnails of designs (mitcad#1695): the thumbnail handler
; bin\mitcad-thumbnails.dll (app/thumbnails), an in-process COM server,
; registered for .mitcad in the scope of the installation. Its CLSID is
; the one of app/thumbnails/ThumbnailHandler.hpp (the installer test runs
; app/tests/thumbnail_handler_windows.cpp --registered, which checks that they
; agree); the ShellEx key is IThumbnailProvider's interface id. The
; installer is 32-bit and Software\Classes\CLSID is redirected for 32-bit
; programs, so the keys are written in the 64-bit view, where Explorer
; reads them.
!define MITCAD_THUMBNAIL_CLSID "{3A94966B-4167-4E3A-81FA-E3F4B2233496}"
!define MITCAD_THUMBNAIL_PROVIDER "{e357fccd-a995-4576-b01f-234630154e96}"
!define MITCAD_THUMBNAIL_DLL "mitcad-thumbnails.dll"

!macro MitcadThumbnailsRegister
  SetRegView 64
  WriteRegStr SHCTX "Software\Classes\CLSID\${MITCAD_THUMBNAIL_CLSID}" "" "MitCad design thumbnails"
  WriteRegStr SHCTX "Software\Classes\CLSID\${MITCAD_THUMBNAIL_CLSID}\InprocServer32" "" \
    "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}"
  WriteRegStr SHCTX "Software\Classes\CLSID\${MITCAD_THUMBNAIL_CLSID}\InprocServer32" "ThreadingModel" "Apartment"
  WriteRegStr SHCTX "Software\Classes\.mitcad\ShellEx\${MITCAD_THUMBNAIL_PROVIDER}" "" "${MITCAD_THUMBNAIL_CLSID}"
  SetRegView default
!macroend

!macro MitcadThumbnailsUnregister
  SetRegView 64
  DeleteRegKey SHCTX "Software\Classes\.mitcad\ShellEx\${MITCAD_THUMBNAIL_PROVIDER}"
  DeleteRegKey /ifempty SHCTX "Software\Classes\.mitcad\ShellEx"
  DeleteRegKey SHCTX "Software\Classes\CLSID\${MITCAD_THUMBNAIL_CLSID}"
  SetRegView default
!macroend

; Explorer, or the process it runs thumbnail handlers in, may still hold
; the handler of the installation (no program from bin, so the check for
; running programs does not see it). A loaded DLL cannot be replaced or
; deleted but can be renamed: it is moved aside to .old, deleted when no
; longer loaded (at the next install or uninstall, or on restart where the
; rights allow), so that the new one can be put in place.
!macro MitcadThumbnailsMoveAside
  Delete "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}.old"
  Delete "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}"
  ${If} ${FileExists} "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}"
  ${AndIfNot} ${FileExists} "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}.old"
    Rename "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}" "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}.old"
    Delete /REBOOTOK "$INSTDIR\bin\${MITCAD_THUMBNAIL_DLL}.old"
  ${EndIf}
!macroend
