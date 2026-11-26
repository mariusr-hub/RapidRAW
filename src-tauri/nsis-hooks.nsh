!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "${UNINSTKEY}" "DisplayVersion" "1.7.1.2"
!macroend
