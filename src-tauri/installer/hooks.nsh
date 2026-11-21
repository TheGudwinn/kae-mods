; Branding for the Kae Mods installer.
; The Tauri NSIS template includes this file BEFORE it declares the MUI pages, so these defines apply to them.
; (Hooks such as NSIS_HOOK_POSTINSTALL could be added here later; none are needed now.)

; Dark welcome/finish pages and header, matching the app.
!define MUI_BGCOLOR "080918"
!define MUI_TEXTCOLOR "EEECF8"

; Progress log in the app's violet on the same dark background.
!define MUI_INSTFILESPAGE_COLORS "A193FF 080918"

