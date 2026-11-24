# Minecraft-раздел (мультиигровость)
Этап: функционал F1–F12 и дизайн D1–D9 из `docs/product-review-minecraft.md` реализован и собран; запуск Fabric/Forge проверялся вживую раньше, остальное (вход Microsoft, обновление модпака, коды) вживую не проверено.
Контекст: «как CurseForge внутри менеджера»: сборки, моды, версии игры, Java, оффлайн/лицензия, общий каркас для нескольких игр.
Решение:
- Бэкенд `src-tauri/src/minecraft/` (versions, java, launch, loader, catalog, modpack, share, auth), команды `commands/minecraft.rs` (`mc_*`), Discord-статус `presence.rs`, перенос данных со старого идентификатора `legacy.rs`.
- UI `src/features/minecraft/` (Minecraft.tsx — каталог и каркас, Instances.tsx — библиотека и страница сборки, GeneralSettings.tsx — общие настройки), блоки в настройках `features/settings/McSettings.tsx`, `Presence.tsx`.
- Загрузчики: vanilla, Fabric, Quilt, Forge, NeoForge (installer). Каталог: Modrinth без ключа, CurseForge по ключу. Свой формат `.kaepack`, импорт `.mrpack`, короткие коды `KAE1-` (моды по версиям Modrinth).
- Данные в `%APPDATA%\KaeMods` (или выбранная папка). Конец сессии игры и время игры считает фоновый наблюдатель (`watch_session`), событие `mc-exit`.
- Вход Microsoft: device code, `CLIENT_ID` в `auth.rs` (публичный). Нужно одобрение Mojang (форма aka.ms/mce-reviewappid), без него `login_with_xbox` даёт 403.
- Discord: `APP_ID` и `LOGO_URL` в `presence.rs`.
Не сделано: установки Minecraft в странице Downloads (пока полоса прогресса с отменой), метка «рекомендуемая» у версий Forge, живая проверка NeoForge.
Связи: src-tauri/src/minecraft/mod.rs (тесты), src/i18n/*.json (ключи mc.*, presence.*), docs/product-review-minecraft.md
