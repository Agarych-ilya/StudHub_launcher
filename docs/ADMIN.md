# Изменения 0.4.3

Microsoft OAuth удалён. Поля microsoft_client_id и redirect_uri больше не используются. Telegram-бот остаётся отдельным. Вход по нику, UUID Java OfflinePlayer. Сервер должен разрешать такой режим входа. Для NeoForge закреплена версия 21.1.250. Сначала используйте PLAYER-GUIDE.md.

Ниже историческая справка 0.4.2 по CDN/API; описание Microsoft-входа устарело.

# Подключение клиента 0.4.2

Telegram-бот отделён от клиента. Для игры нужны официальный Minecraft Java аккаунт, разрешённый Microsoft application Client ID, подписанный manifest модпака, HTTPS CDN и реальный адрес Minecraft-сервера. Публичный API StudHub необязателен: используется только GET /server/status (online, players, max_players, version) и GET /news (массив title, body, date), без Telegram Bearer token и без запросов /profile.

Заполните deployment.example.json и сохраните как deployment.json. trusted_origins содержит точные HTTPS origins manifest/файлов; перенаправления запрещены. Публичные ключи Ed25519 — base64 от 32 байт. Секреты бота и закрытые ключи в файл не добавляйте.

Microsoft: собственный public desktop client с разрешённым callback http://localhost:8231/callback и доступом Minecraft Services API. PKCE/state, refresh, лицензия и официальный профиль проверяются клиентом. Токены хранятся в Windows Credential Manager/macOS Keychain/Linux kernel keyring, без plaintext fallback. Для устойчивых Linux-сессий требуется миграция на persistent Secret Service; kernel keyring может потерять сессию после перезагрузки.

Папка server сохранена как отдельный компонент прежнего проекта и не связана с авторизацией лаунчера. Код бота не менялся. Настройка владельца — OWNER-LOGIN.md.

## CDN и модпак

На стороне администратора:

```text
modpack/
  mods/
  config/
  defaultconfigs/
  resourcepacks/
  shaderpacks/
  servers.dat
```

Скопируйте mod-ids.example.json и впишите реальные пути TaCZ (для NeoForge 1.21.1) и StudHub Core. Rust дополнительно проверяет заявленный modId в JAR; несовместимые Fabric-файлы будут отклонены.

```sh
node tools/manifest.mjs keygen --out pack-private.pem
node tools/manifest.mjs pack --root ./modpack --cdn https://YOUR_CDN/modpack --version YOUR_VERSION --neoforge 21.1.256 --key pack-private.pem --ids mod-ids.json --out manifest.json
```

Это команды с явно маркированными YOUR_* значениями, не deployable адреса. Закрытый ключ храните только у администратора; публичный ключ команда keygen выводит для deployment.json. На Windows дополнительно ограничьте ACL файла ключа: mode=0600 не заменяет NTFS ACL.

Сначала загрузите файлы CDN, затем атомарно опубликуйте manifest.json. Подпись покрывает canonical JSON payload: ключи объектов сортируются, UTF-8, без пробелов. Не меняйте подписанный payload вручную.

Устаревшие управляемые файлы удаляются только если их локальный хеш совпадает с предыдущим управляемым хешем. Изменённые устаревшие файлы сохраняются как пользовательские. Существующий неуправляемый файл с именем нового управляемого файла не перезаписывается: игрок получает объяснение.

Запретите несовместимые дополнения через incompatible_user_mods в payload. Дубликаты modId, Fabric-only и повреждённые JAR блокируют запуск с именем файла. Это базовая проверка; все зависимости каждого стороннего NeoForge-мода автоматически не разрешаются.

## Обновления самого клиента

Отдельный ключ и launcher_update_url. Payload:

```json
{"version":"0.4.1","artifacts":{"windows-amd64":{"url":"https://YOUR_CDN/StudHubLauncher-Setup.exe","size":0,"sha256":"REPLACE_WITH_REAL_SHA256"}}}
```

Пример схемы, не рабочая публикация. Подпишите node tools/manifest.mjs sign --input app-update.json --key app-private.pem --out launcher-manifest.json. Платформы: windows-amd64, darwin-arm64, darwin-x86_64, linux-x86_64. Клиент проверяет подпись и SHA-256, скачивает установщик и открывает его папку. Игрок запускает установщик после закрытия клиента.

Для публичного выпуска настройте Windows Authenticode и macOS Developer ID/notarization в CI. Текущий установщик не подписан сертификатом издателя.
