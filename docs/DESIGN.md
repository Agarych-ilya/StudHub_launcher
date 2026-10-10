# Интерфейс

Тёмный navy, золотой акцент и голубые оттенки исходной академической шапочки. Manrope с кириллицей, локально включённый OFL-шрифт. Hero — художественный фон, а не изображение вашего реального сервера. Статистика/новости/аккаунты читаются только из backend.

Фон public/world.png создан встроенным imagegen как растровый проектный ассет. Финальный prompt:

> Cinematic voxel Minecraft-like university outpost and forested mountains at blue hour, block-built stone tower and wooden roofs with small warm golden window lights on the right. Cubic pine forest, layered mountain silhouettes, atmospheric navy sky, cobalt shadows and restrained gold highlights. Left half: dark quiet sky for interface text. Premium detailed voxel environment, crisp pixel surfaces. No text, logos, UI, people or watermark.

Это ассет приложения, не отдельный макет. В evidence/native-*.png находятся скриншоты настоящего Tauri-окна Linux.

## Иконка 0.4.1

Существующий растровый знак отредактирован встроенным imagegen. Prompt: удалить только внешний чёрный фон, сохранить voxel-шапочку, синие/голубые контуры, золотую кисточку и кубики; настоящий RGBA alpha, чуть увеличить предмет с безопасным отступом. Оригинальная непрозрачная аватарка заменена по запросу владельца. Tauri icon CLI создал PNG/ICO/ICNS; public/app-icon.png — прозрачная 128px версия для WebView.
