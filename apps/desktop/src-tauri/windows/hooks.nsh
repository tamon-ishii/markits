; Register the installed application for the current user's next Windows login.
!macro NSIS_HOOK_POSTINSTALL
  CreateDirectory "$SMSTARTUP"
  CreateShortcut "$SMSTARTUP\MarkIts.lnk" "$INSTDIR\markits-desktop.exe" "--background"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$SMSTARTUP\MarkIts.lnk"
!macroend
