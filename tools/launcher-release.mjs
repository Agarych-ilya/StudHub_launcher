import fs from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {createHash,createPublicKey,sign} from 'node:crypto';
import path from 'node:path';
const args=process.argv.slice(2),opts={};for(let i=0;i<args.length;i+=2)opts[args[i].replace(/^--/,'')]=args[i+1];
for(const name of ['exe','version','url','update-url','key','out'])if(!opts[name])throw Error('--'+name+' обязателен');
if(!/^\d+\.\d+\.\d+$/.test(opts.version))throw Error('Версия должна иметь вид 0.4.7');
const download=new URL(opts.url),manifest=new URL(opts['update-url']);
if(download.protocol!=='https:'||manifest.protocol!=='https:'||!download.pathname.endsWith('.exe'))throw Error('Нужны HTTPS-адреса manifest и NSIS .exe');
const key=await fs.readFile(opts.key),publicKey=createPublicKey(key).export({type:'spki',format:'der'}).subarray(-32).toString('base64');
const hash=createHash('sha256');for await(const chunk of createReadStream(opts.exe))hash.update(chunk);
const payload={version:opts.version,artifacts:{'windows-amd64':{url:download.href,size:(await fs.stat(opts.exe)).size,sha256:hash.digest('hex'),installer:'nsis'}}};
function canonical(v){if(Array.isArray(v))return v.map(canonical);if(v&&typeof v==='object')return Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])]));return v}
const envelope={payload,signature:sign(null,Buffer.from(JSON.stringify(canonical(payload))),key).toString('base64')};
const deployment={api_url:'',manifest_url:'',trusted_origins:[...new Set([download.origin,manifest.origin])],manifest_public_key:'',server_address:'148.253.208.254:25589',launcher_update_url:manifest.href,launcher_public_key:publicKey,spawn_status_url:opts['spawn-url']||''};
if(deployment.spawn_status_url){const spawn=new URL(deployment.spawn_status_url);if(spawn.protocol!=='https:')throw Error('Счётчик спавна требует HTTPS');deployment.trusted_origins=[...new Set([...deployment.trusted_origins,spawn.origin])]}
await fs.mkdir(opts.out,{recursive:true});
await fs.writeFile(path.join(opts.out,'launcher-manifest.json'),JSON.stringify(envelope,null,2));
await fs.writeFile(path.join(opts.out,'deployment.json'),JSON.stringify(deployment,null,2));
console.log('Созданы подписанный launcher-manifest.json и deployment.json. Закрытый ключ не копируется.');
