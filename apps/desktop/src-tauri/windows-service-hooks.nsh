; The service executable must never run from a user-writable installation.
!macro NSIS_HOOK_PREINSTALL
  StrLen $R1 "$PROGRAMFILES64\"
  StrCpy $R0 "$INSTDIR" $R1
  StrCmp $R0 "$PROGRAMFILES64\" +2 0
    StrCpy $INSTDIR "$PROGRAMFILES64\FarSail"
  IfFileExists "$INSTDIR\farsail-system-host.exe" 0 farsail_service_preinstall_done
  nsExec::ExecToStack '"$INSTDIR\farsail-system-host.exe" --stop'
  Pop $R0
  Pop $R1
  StrCmp $R0 "0" farsail_service_preinstall_done
    MessageBox MB_ICONSTOP "Unable to stop the FarSail desktop service. Installation stopped."
    Abort
  farsail_service_preinstall_done:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack '"$INSTDIR\farsail-system-host.exe" --install'
  Pop $R0
  Pop $R1
  StrCmp $R0 "0" farsail_service_postinstall_done
    MessageBox MB_ICONSTOP "Unable to install the FarSail desktop service. Run the installer again as administrator."
    Abort
  farsail_service_postinstall_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack '"$INSTDIR\farsail-system-host.exe" --uninstall'
  Pop $R0
  Pop $R1
  StrCmp $R0 "0" farsail_service_preuninstall_done
    MessageBox MB_ICONSTOP "Unable to remove the FarSail desktop service. Uninstall stopped."
    Abort
  farsail_service_preuninstall_done:
!macroend
