# Имя исполняемого файла
Этап: файл приложения называется `Kae Mods.exe`.
Контекст: имя продукта уже было Kae Mods, а файл назывался deadlock-mm.exe.
Решение: `mainBinaryName` в tauri.conf.json (переименовывает только `tauri build`; `cargo build` даёт старое имя). Имя crate, базы `deadlock_mm` и старый id не менять: на них завязаны данные.
Связи: src-tauri/tauri.conf.json, .github/workflows/release.yml
