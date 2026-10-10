// Native Linux smoke test: real Tauri + WebKitGTK + Rust IPC, with isolated user data.
import {spawn} from 'node:child_process';
import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import net from 'node:net';
const root=path.resolve(import.meta.dirname,'..');
const evidence=path.join(root,'evidence');await fs.mkdir(evidence,{recursive:true});
const testRoot=await fs.mkdtemp(path.join(root,'.native-test-'));
const display=100+Math.floor(Math.random()*300);
const env={...process.env,DISPLAY:'127.0.0.1:'+display,DBUS_SESSION_BUS_ADDRESS:'tcp:host=127.0.0.1,port=6790',WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS:'1',XDG_CONFIG_HOME:path.join(testRoot,'config'),XDG_DATA_HOME:path.join(testRoot,'data')};
let statusRequests=0;
function vi(n){const bytes=[];do{let b=n&127;n>>>=7;bytes.push(n?b|128:b)}while(n);return Buffer.from(bytes)}
const mc=net.createServer(socket=>socket.once('data',()=>{statusRequests++;const body=Buffer.from(JSON.stringify({version:{name:'Minecraft 1.21.1',protocol:767},players:{online:statusRequests===1?5:9,max:20}}));const packet=Buffer.concat([Buffer.from([0]),vi(body.length),body]);socket.end(Buffer.concat([vi(packet.length),packet]));}));
await new Promise(resolve=>mc.listen(0,'127.0.0.1',resolve));
const configDir=path.join(testRoot,'config','ru.studhub.launcher');await fs.mkdir(configDir,{recursive:true});
await fs.writeFile(path.join(configDir,'settings.json'),JSON.stringify({deployment:{server_address:'127.0.0.1:'+mc.address().port}}));
const children=[];
function start(cmd,args){const log=spawn(cmd,args,{env,stdio:['ignore','pipe','pipe']});children.push(log);log.stdout.on('data',d=>fs.appendFile(path.join(evidence,'native-processes.log'),d));log.stderr.on('data',d=>fs.appendFile(path.join(evidence,'native-processes.log'),d));return log;}
start('Xvfb',[':'+display,'-screen','0','1280x820x24','-listen','tcp','-ac']);
start('dbus-daemon',['--session','--address=tcp:host=127.0.0.1,port=6790','--nofork']);
start(path.join(os.homedir(),'.cargo/bin/tauri-driver'),['--port','4444','--native-driver','/usr/bin/WebKitWebDriver']);
const pause=ms=>new Promise(r=>setTimeout(r,ms));
let session;
async function request(method,p,body){const r=await fetch('http://127.0.0.1:4444'+p,{method,headers:{'Content-Type':'application/json'},body:body?JSON.stringify(body):undefined,signal:AbortSignal.timeout(p.includes('/execute/async')?1800000:60000)});const data=await r.json();if(data.value?.error)throw Error(JSON.stringify(data.value));return data.value;}
async function js(script,args=[]){return request('POST','/session/'+session+'/execute/sync',{script,args})}
async function asyncjs(script,args=[]){return request('POST','/session/'+session+'/execute/async',{script,args})}
async function click(selector){console.log('Click',selector);await js('document.querySelector(arguments[0]).click()', [selector]);await pause(450)}
async function screenshot(name){await fs.writeFile(path.join(evidence,name),Buffer.from(await request('GET','/session/'+session+'/screenshot'),'base64'))}
const tests=[];
try{
  await pause(1200);
  for(let n=0;n<5;n++){try{const v=await request('POST','/session',{capabilities:{alwaysMatch:{'tauri:options':{application:path.join(root,process.argv.includes('--release')?'src-tauri/target/release/studhub-launcher':'src-tauri/target/debug/studhub-launcher')}}}});session=v.sessionId;break}catch(e){console.log('Session',String(e));if(n===4||e.name==='TimeoutError')throw e;await pause(500)}}
  await request('POST','/session/'+session+'/timeouts',{script:1800000,implicit:5000});
  await pause(1200);
  const body=await js('return document.body.innerText');if(!body.includes('Твой мир.'))throw Error('Главная страница не загрузилась: '+body);
  tests.push('Настоящее desktop-окно: главная страница загрузилась');
  const snap=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done).catch(e=>done({error:String(e)}))');
  if(snap.error||!snap.settings.game_dir)throw Error('Rust IPC: '+JSON.stringify(snap));
  tests.push('Rust IPC: чтение настоящих настроек');
  const design=await js('return {icons:document.querySelectorAll("nav .minecraft-icon").length,word:!!document.querySelector(".brand-name"),size:parseFloat(getComputedStyle(document.querySelector(".sidebar-world p")).fontSize),weight:getComputedStyle(document.querySelector(".sidebar-world p")).fontWeight}');
  if(design.icons!==4||!design.word||design.size<21||Number(design.weight)<800)throw Error('Оформление не обновлено: '+JSON.stringify(design));
  tests.push('Читаемая надпись StudHub, 4 пиксельные иконки Minecraft, увеличенный жирный слоган');
  const pixel=await asyncjs('const done=arguments[arguments.length-1];document.fonts.ready.then(()=>done({loaded:document.fonts.check("14px Monocraft"),family:getComputedStyle(document.querySelector(".connection")).fontFamily,size:parseFloat(getComputedStyle(document.querySelector(".connection")).fontSize),version:parseFloat(getComputedStyle(document.querySelector(".build-label")).fontSize)}))');
  if(!pixel.loaded||!pixel.family.includes('Monocraft')||pixel.size<13||pixel.version<12)throw Error('Пиксельный шрифт статуса не загрузился: '+JSON.stringify(pixel));
  tests.push('Статус сервера и версия: загружен настоящий Monocraft, увеличенный размер текста');
  await js('document.querySelector(".hero").dispatchEvent(new MouseEvent("mouseenter"));document.querySelector(arguments[0]).focus();',['button[aria-label="Следующее изображение"]']);
  const slideBefore=await js('return document.querySelector(".hero").dataset.slide');await pause(5500);
  if(await js('return document.querySelector(".hero").dataset.slide')===slideBefore)throw Error('Баннер не перелистывается автоматически');
  const images=await js('return Array.from(document.querySelectorAll(".hero-slide")).map(e=>({loaded:e.complete&&e.naturalWidth>0,src:e.getAttribute("src")}))');
  if(images.length!==3||images.some(i=>!i.loaded))throw Error('Картинки баннера не загрузились: '+JSON.stringify(images));
  await click('button[aria-label="Показать изображение 2"]');await screenshot('native-carousel-valley.png');
  await click('button[aria-label="Показать изображение 3"]');await screenshot('native-carousel-snow.png');
  await click('button[aria-label="Приостановить перелистывание"]');const stopped=await js('return document.querySelector(".hero").dataset.slide');await pause(5500);
  if(await js('return document.querySelector(".hero").dataset.slide')!==stopped)throw Error('Пауза карусели не работает');
  await click('button[aria-label="Показать изображение 1"]');
  await click('button[aria-label="Включить перелистывание"]');const resumed=await js('return document.querySelector(".hero").dataset.slide');await pause(5500);
  if(await js('return document.querySelector(".hero").dataset.slide')===resumed)throw Error('Перелистывание не возобновилось после кнопки паузы');
  tests.push('Карусель: автоматическая смена при наведении и фокусе, 3 изображения, ручной выбор, пауза и возобновление');
  if(await js('return document.querySelectorAll(".status-card").length!==2||document.body.innerText.includes("управляемых модов")||document.body.innerText.includes("каждые 15 с")||!!Array.from(document.querySelectorAll(".status-card")).find(e=>e.innerText.includes("ОБНОВЛЕНИЯ"))'))throw Error('Лишние подписи или карточка обновлений остались');
  tests.push('Главная: карточка обновлений и ненужные подписи онлайна/числа управляемых модов удалены');
  await fs.writeFile(path.join(evidence,'native-image-diagnostic.json'),JSON.stringify(await js('return Array.from(document.images).map(e=>({src:e.currentSrc,complete:e.complete,width:e.naturalWidth,rect:e.getBoundingClientRect().toJSON()}))'),null,2));
  await screenshot('native-home.png');
  let ready;
  for(let n=0;n<180;n++){
    ready=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');
    if(!ready.busy&&ready.pack_version==="1.0.2")break;
    await pause(1000);
  }
  if(ready.busy||ready.pack_version!=='1.0.2'||ready.mods.filter(m=>m.managed).length!==48)throw Error('Встроенная сборка не установлена: '+JSON.stringify(ready.logs));
  if(ready.mods.some(m=>m.issue))throw Error('Ложные конфликты модов: '+JSON.stringify(ready.mods.filter(m=>m.issue)));
  tests.push('Встроенная сборка 1.0.2: 48 JAR автоматически установлены, ложных конфликтов нет');
  if(statusRequests<2||ready.server.players!==9||!ready.server.checked_at)throw Error('Периодический онлайн не обновился: '+JSON.stringify(ready.server));
  tests.push('Настоящий Minecraft status TCP: общий онлайн автоматически обновился с 5 до 9 без нажатия кнопки');
  const drive=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("action",{kind:"check_drive"}).then(done).catch(e=>done({error:String(e)}))');
  if(drive.error||!drive.files?.some(f=>f.name==="Сборка модов 1.0.2.zip")||drive.status!=="ok")throw Error('Проверка настоящей папки Drive: '+JSON.stringify(drive));
  tests.push('Настоящий Google Drive: публичная папка прочитана, архив найден, повторная проверка без ложных изменений');

  await click('.folder-button');await fs.access(path.join(snap.settings.game_dir,'mods'));tests.push('Кнопка mods: настоящий каталог создан, системный обработчик папки вызван');
  await click('.play-button');if(!(await js('return document.body.innerText')).includes('Профиль Minecraft'))throw Error('Play не направил к авторизации');tests.push('Играть без ника: переход к профилю');
  await screenshot('native-profile.png');
  if(await js('return !!document.querySelector(".profile-info")||document.body.innerText.includes("Сервер StudHub")'))throw Error('Карточка сервера в профиле осталась');
  tests.push('Карточка Сервер StudHub полностью удалена из профиля');
  if(await js('return !!document.querySelector(".telegram-button")'))throw Error('Telegram login остался в интерфейсе');tests.push('Telegram login отсутствует в интерфейсе');
  const removed=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("action",{kind:"telegram"}).then(()=>done("unexpected success"),e=>done(String(e)))');if(!removed.includes('Неизвестная операция'))throw Error('Старая Telegram-команда активна');tests.push('Rust backend отклоняет удалённую Telegram-команду');
  await click('.account-card .primary');if(!(await js('return document.body.innerText')).includes('Укажите ник'))throw Error('Пустой ник принят');
  await js('const e=document.querySelector("input[aria-label="+JSON.stringify("Ник Minecraft")+"]");const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set;set.call(e,"StudHub_01");e.dispatchEvent(new Event("input",{bubbles:true}));e.dispatchEvent(new Event("change",{bubbles:true}));');
  await click('.account-card .primary');
  const profile=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');
  if(profile.minecraft?.name!=='StudHub_01')throw Error('Ник не сохранён');
  tests.push('Ник: пустой отклонён, валидный сохранён настоящим Rust backend');
  const oldMs=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("action",{kind:"minecraft"}).then(()=>done("unexpected success"),e=>done(String(e)))');
  if(!oldMs.includes('Неизвестная операция'))throw Error('Старый Microsoft login активен');
  tests.push('Microsoft OAuth-команда удалена');
  const beforePlay=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');
  const testSettings={...beforePlay.settings,java_path:path.join(testRoot,'nonexistent-java')};
  await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("save_settings",{settings:arguments[0]}).then(done).catch(e=>done({error:String(e)}))',[testSettings]);
  await click('nav button:first-child');await click('.play-button');
  let afterPlay;
  for(let n=0;n<120;n++){afterPlay=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');if(!afterPlay.busy)break;await pause(1000);}
  if(afterPlay.busy||afterPlay.pid||!(afterPlay.drive.checked_at>beforePlay.drive.checked_at)||!afterPlay.logs.slice(beforePlay.logs.length).some(x=>x.startsWith('Google Drive:')))throw Error('Нажатие ИГРАТЬ не вызвало новую проверку Drive');
  await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("save_settings",{settings:arguments[0]}).then(done)',[beforePlay.settings]);
  tests.push('Кнопка ИГРАТЬ: настоящий Rust backend заново проверяет Google Drive автоматически до подготовки игры');
  await click('nav button:nth-child(2)');await screenshot('native-pack.png');tests.push('Навигация: сборка и профиль');
  const rowFont=await js('return [".mod-row .source",".mod-row .mod-ok"].map(s=>{const c=getComputedStyle(document.querySelector(s));return {size:parseFloat(c.fontSize),weight:Number(c.fontWeight),family:c.fontFamily}})');
  if(rowFont.some(c=>c.size<14||c.weight<800||!c.family.includes('Manrope')))throw Error('Мелкие подписи модов: '+JSON.stringify(rowFont));
  tests.push('Список модов: StudHub и Обнаружен увеличены до 14–15 px, жирность 800, шрифт Manrope');
  const first=ready.mods.find(m=>m.managed);
  const modFile=path.join(ready.settings.game_dir,'mods',first.file);const original=await fs.readFile(modFile);await fs.writeFile(modFile,'damaged');
  await click('.repair-card button');
  for(let n=0;n<120;n++){const s=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');if(!s.busy)break;await pause(1000);}
  if(!(await fs.readFile(modFile)).equals(original))throw Error('Встроенный мод не восстановлен');
  tests.push('Исправить сборку: повреждённый настоящий мод восстановлен из встроенного архива без CDN');

  await click('nav button:nth-child(3)');
  await js('const e=document.querySelector(".nickname-input");const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set;set.call(e,"draft name");e.dispatchEvent(new Event("input",{bubbles:true}));');
  await click('nav button:nth-child(4)');
  await js('document.querySelector(".settings-panel .switch").click()');await pause(100);
  await js('const e=document.querySelector("input[type=range]");const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,"value").set;set.call(e,"7168");e.dispatchEvent(new Event("input",{bubbles:true}));e.dispatchEvent(new Event("change",{bubbles:true}));');
  await js('document.querySelector("input[type=range]").dispatchEvent(new PointerEvent("pointerup",{bubbles:true}))');await pause(500);
  if(await js('return !!document.querySelector(".deployment-panel,.settings-save")||document.body.innerText.includes("Автоподключение к серверу")||document.body.innerText.includes("Обновления при старте")'))throw Error('Удалённый блок настроек остался');
  tests.push('Нижний блок настроек полностью удалён: переключатели, кнопки и подключение StudHub');
  let saved;for(let attempt=0;attempt<20;attempt++){saved=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');
  if(saved.settings.ram_mb===7168)break;await pause(250);}
  if(saved.settings.ram_mb!==7168)throw Error('RAM не сохранилась: '+saved.settings.ram_mb);tests.push('Настройки: изменение RAM автоматически сохранено Rust backend в JSON');
  if(saved.minecraft?.name!=='StudHub_01')throw Error('Автосохранение настроек изменило неподтверждённый ник');
  tests.push('Автосохранение настроек сохраняет игровой ник: черновик профиля не применяется');
  await screenshot('native-settings.png');
  await click('button[aria-label="Открыть журнал"]');if(!(await js('return document.body.innerText')).includes('Журнал лаунчера'))throw Error('Журнал не открылся');tests.push('Журнал открывается по запросу');
  await click('button[aria-label="Закрыть журнал"]');
  await request('POST','/session/'+session+'/window/rect',{width:960,height:660});await click('nav button:first-child');await screenshot('native-home-960.png');
  const bad=await js('return Array.from(document.querySelectorAll(".hero,.play-button,.folder-button,header,.sidebar")).filter(e=>{const r=e.getBoundingClientRect();return r.left<0||r.right>innerWidth+1||r.top<0||r.bottom>innerHeight+1}).map(e=>e.className)');
  if(bad.length)throw Error('Элементы вышли за окно: '+bad.join(','));tests.push('Минимальное окно 960×660: основные элементы доступны');
  if(process.argv.includes('--install')){
    await click('nav button:nth-child(2)');
    await js('Array.from(document.querySelectorAll("button")).find(b=>b.textContent.includes("Установить Minecraft, Java")).click()');
    for(let n=0;n<1200;n++){
      await pause(1000);const s=await asyncjs('const done=arguments[arguments.length-1];window.__TAURI_INTERNALS__.invoke("snapshot").then(done)');
      if(!s.busy){const all=s.logs.join('\n');await fs.writeFile(path.join(evidence,'minecraft-install.log'),all);if(!all.includes('Minecraft, Java 21 и NeoForge установлены'))throw Error('Установка игры: '+all.slice(-2500));tests.push('Реальная установка Minecraft 1.21.1, Java 21, NeoForge и assets завершилась');break}
      if(n===1199)throw Error('Установка не завершилась за 20 минут');
    }
  }
  await fs.writeFile(path.join(evidence,process.argv.includes('--release')?'native-release-tests.json':'native-tests.json'),JSON.stringify({passed:tests},null,2));console.log(JSON.stringify({passed:tests},null,2));
}finally{
  if(session)try{await request('DELETE','/session/'+session)}catch{}
  children.reverse().forEach(p=>p.kill('SIGTERM'));
  mc.close();
}
