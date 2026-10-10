# Проверка сборки StudHub 1.0.2

Проверен архив «Сборка модов 1.0.2.zip», предоставленный пользователем: 48 JAR-файлов, 187 653 771 байт. Minecraft 1.21.1, NeoForge 21.1.250.

Все исходные JAR сохраняются без изменений. Лаунчер использует SHA-256 архива и каждого файла; установка и восстановление берут файлы из встроенного архива. Конфиги и дополнительные ресурсы в ZIP отсутствуют.

## Зависимости и загрузчик

При рекурсивной проверке META-INF/jarjar/metadata.json все объявленные обязательные modId найдены либо в отдельных модах, либо во вложенных JAR. Flywheel, Ponder, Simulated/Aeronautics, Apollib, Mezz Config и XaeroLib встроены. Это проверка присутствия зависимостей, а не доказательство их полной совместимости во время исполнения.

Kotlin for Forge — контейнер языкового провайдера и библиотек; отсутствие mods.toml у наружного JAR допустимо. Его вложенные JAR содержат описание kotlinforforge.

StudHub Core в архиве отсутствует. Лаунчер больше не требует этот мод, поскольку предоставленный пользователем состав сборки является источником истины.

## Обнаруженная проблема

В jei-1.21.1-neoforge-19.57.0.449.jar обязательная зависимость minecraft имеет versionRange = "[1.21, 1.21.1)". Правая круглая скобка исключает 1.21.1. Таким образом, заявленные метаданные JEI не разрешают целевую версию игры. Это может остановить загрузку NeoForge.

В этой поставке JEI не подменён и его JAR не исправлялся. Для устранения нужен корректный совместимый JAR JEI от автора/из вашей рабочей клиентской сборки. Отключение проверки версий зависимостей не реализовано.

Запуск игры с этим комплектом до главного меню и подключение к StudHub не объявлены проверенными.

## Состав архива

| Файл | Основной modId | Версия |
|---|---|---|
| aeronautical_diesel-1.1.0.jar | aeronautical_diesel | 1.1.0 |
| aeroworks-1.5.0.jar | aeroworks | 1.5.0 |
| alltheleaks-1.1.13%2B1.21.1-neoforge.jar | alltheleaks | 1.1.13+1.21.1-neoforge |
| architectury-13.0.11-neoforge.jar | architectury | 13.0.11 |
| c2me-neoforge-mc1.21.1-0.4.0-alpha.0.122.jar | c2me | 0.4.0-alpha.0.122+1.21.1 |
| cbcatfix-1.1.0.jar | cbcatfix | 1.1.0 |
| cbc_at_Neoforge_1.21.1_0.1.4c.jar | cbc_at | 0.1.4c-1.21.1 |
| Chunky-NeoForge-1.4.23.jar | chunky | 1.4.23 |
| copycats-3.0.9%2Bmc.1.21.1-neoforge.jar | copycats | 3.0.9+mc.1.21.1-neoforge |
| create-1.21.1-6.0.10.jar | create | 6.0.10 |
| create-aeronautics-bundled-1.21.1-1.3.2.jar | aeronautics_bundled | 1.3.2 |
| create-track-map-2.1+mc1.21.1-neoforge.jar | createtrackmap | 2.1 |
| createaddition-1.7.1.jar | createaddition | 1.7.1 |
| createbigcannons-5.11.7+mc.1.21.1.jar | createbigcannons | 5.11.7 |
| createdieselgenerators-1.21.1-1.3.15.jar | createdieselgenerators | 1.21.1-1.3.15 |
| createjeicompat-1.0.3.jar | createjeicompat | 1.0.3 |
| create_aero_radar-0.1.1-1.21.1.jar | create_aero_radar | 0.1.1-1.21.1 |
| create_connected-1.3.3-mc1.21.1.jar | create_connected | 1.3.3-mc1.21.1 |
| create_radar-0.4.9.4-1.21.1.jar | create_radar | 0.4.9.4-1.21.1 |
| create_train_physics-0.2.0-beta.2.jar | create_train_physics | 0.2.0-beta.2 |
| croptopia-neoforge-1.21.1-4.2.4.jar | croptopia | 4.2.4 |
| crowns-1.21.1-2.2.5.jar | crowns | 2.2.5 |
| curios-neoforge-9.5.1+1.21.1.jar | curios | 9.5.1+1.21.1 |
| EpheroLib-1.21.1-NEO-FORGE-1.2.0.jar | epherolib | 1.2.0 |
| ferritecore-7.0.3-neoforge.jar | ferritecore | 7.0.3 |
| ForgeConfigAPIPort-v21.1.6-1.21.1-NeoForge.jar | forgeconfigapiport | 21.1.6 |
| formicapi-1.21.1-2.4.1.jar | formicapi | 2.4.1 |
| geckolib-neoforge-1.21.1-4.9.3.jar | geckolib | 4.9.3 |
| jei-1.21.1-neoforge-19.57.0.449.jar | jei | 19.57.0.449 |
| jeioptimizer-1.21.1-1.2.0-19.56.jar | jeioptimizer | 1.2.0-19.56 |
| kotlinforforge-5.12.0-all.jar |  |  |
| lithostitched-1.8.0-neoforge-21.1.jar | lithostitched | 1.8.0 |
| ntgl-1.21.1-3.2.0.jar | ntgl | 3.2.0 |
| ParCool-1.21.1-4.0.0.6.jar | parcool | 4.0.0.6 |
| player-animation-lib-forge-2.0.4+1.21.1.jar | playeranimator | 2.0.4+1.21.1 |
| powergrid-mc1.21.1-0.6.2.jar | powergrid | 0.6.2 |
| ritchiesprojectilelib-2.1.2-mc.1.21.1-neoforge.jar | ritchiesprojectilelib | 2.1.2 |
| sable-neoforge-1.21.1-2.0.5.jar | sable | 2.0.5 |
| sablexaeromaps-1.21.1-1.4.0.jar | sablexaeromaps | 1.4.0 |
| sable_optimizer-1.0.jar | sable_optimizer | 1.0 |
| sodium-neoforge-0.8.13+mc1.21.1.jar | sodium | 0.8.13+mc1.21.1 |
| tacz-neoforge-1.21.1-1.1.8-hotfix-r6.jar | tacz | 1.1.8-hotfix-r6 |
| tacz-pack-upgrader-2.1.3.jar | taczpackupgrader | 2.1.3 |
| taczruntimecompat-1.0.1.jar | taczruntimecompat | 1.0.0 |
| Terralith_1.21.x_v2.6.2.jar | terralith | 2.6.2 |
| voicechat-neoforge-1.21.1-2.6.24.jar | voicechat, voicechat_api | 1.21.1-2.6.24, 2.6.24 |
| xaerominimap-neoforge-1.21.1-26.5.0.jar | xaerominimap | 26.5.0 |
| xaeroworldmap-neoforge-1.21.1-1.46.0.jar | xaeroworldmap | 1.46.0 |
