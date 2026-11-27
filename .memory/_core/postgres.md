# Версия встроенного PostgreSQL
Этап: версия закреплена на 18.7.0.
Контекст: без закрепления сборка 0.2.4 взяла 19.0.0, и она не открывала базу версии 18 («embedded postgres error»).
Решение: `POSTGRESQL_VERSION="=18.7.0"` в `.cargo/config.toml`. Смена мажорной версии только с миграцией данных. Build-скрипт не перекачивает уже скачанный архив: после смены версии нужен `cargo clean -p postgresql_embedded` и новый prefix-key кэша в CI.
Связи: .cargo/config.toml, .github/workflows/release.yml, src-tauri/src/db/mod.rs
Базы 19 (свежие установки 0.2.4): `server_for_data` в db/mod.rs берёт уже распакованный сервер из `postgres/bin` по `PG_VERSION`; в сборку 19 не вшита. Проверка на живых серверах: тест `opens_the_database_in_test_dir` (ignored, `KAE_PG_TEST_DIR`).
