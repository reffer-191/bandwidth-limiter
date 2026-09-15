; Custom NSIS hooks for Bandwidth Limiter (see tauri.conf.json > bundle.windows.nsis.installerHooks)

!macro NSIS_HOOK_PREINSTALL
  ; A previous copy may still be running with the driver loaded.
  nsExec::ExecToLog 'schtasks /End /TN "Bandwidth Limiter"'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Remove the "start with Windows" logon task created from Settings.
  nsExec::ExecToLog 'schtasks /Delete /F /TN "Bandwidth Limiter"'
  ; Unload the capture driver so WinDivert64.sys can be deleted.
  nsExec::ExecToLog 'sc stop WinDivert'
  nsExec::ExecToLog 'sc delete WinDivert'
!macroend
