; Branding for the Kae Mods installer.
; The Tauri NSIS template includes this file BEFORE it declares the MUI pages, so these defines apply to them.
; (Hooks such as NSIS_HOOK_POSTINSTALL could be added here later; none are needed now.)

; Graphite welcome/finish pages and header, matching the app.
!define MUI_BGCOLOR "17181A"
!define MUI_TEXTCOLOR "F5F5F6"

; Progress log in white on the same graphite background.
!define MUI_INSTFILESPAGE_COLORS "F5F5F6 17181A"

