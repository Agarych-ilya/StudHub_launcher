import fs from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import path from 'node:path';
import {generateKeyPairSync,createHash,sign} from 'node:crypto';
const args=process.argv.slice(2);const command=args.shift();
const opts={};for(let i=0;i<args.length;i+=2)opts[args[i].replace(/^--/,'')]=args[i+1];
function canonical(v){if(Array.isArray(v))return v.map(canonical);if(v&&typeof v==='object')return Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])]));return v}
function signed(payload,key){return {payload,signature:sign(null,Buffer.from(JSON.stringify(canonical(payload))),key).toString('base64')}}
if(command==='keygen'){
  if(!opts.out)throw Error('--out private-key.pem обязателен');
  const {privateKey,publicKey}=generateKeyPairSync('ed25519');
  await fs.writeFile(opts.out,privateKey.export({type:'pkcs8',format:'pem'}),{mode:0o600,flag:'wx'});
  console.log('Public key (в deployment.json): '+publicKey.export({type:'spki',format:'der'}).subarray(-32).toString('base64'));
}else if(command==='pack'){
  for(const name of ['root','cdn','version','neoforge','key','ids','out'])if(!opts[name])throw Error('--'+name+' обязателен');
  if(!/^21\.1\.\d+$/.test(opts.neoforge))throw Error('Нужен NeoForge 21.1.x');
  const base=new URL(opts.cdn);if(base.protocol!=='https:')throw Error('CDN должен использовать HTTPS');
  const root=path.resolve(opts.root),ids=JSON.parse(await fs.readFile(opts.ids,'utf8')),files=[];
  async function walk(dir){
    for(const e of await fs.readdir(path.join(root,dir),{withFileTypes:true})){
      if(e.isSymbolicLink())throw Error('Символические ссылки запрещены: '+e.name);
      const name=path.posix.join(dir,e.name);
      if(e.isDirectory())await walk(name);else if(e.isFile())await add(name);
    }
  }
  async function add(name){
    const full=path.join(root,name),hash=createHash('sha256');for await(const chunk of createReadStream(full))hash.update(chunk);
    const entry={path:name,url:opts.cdn.replace(/\/$/,'')+'/'+name.split('/').map(encodeURIComponent).join('/'),size:(await fs.stat(full)).size,sha256:hash.digest('hex')};
    if(ids[name])entry.mod_id=ids[name];files.push(entry);
  }
  for(const dir of ['mods','config','defaultconfigs','resourcepacks','shaderpacks'])try{await walk(dir)}catch(e){if(e.code!=='ENOENT')throw e}
  try{await fs.access(path.join(root,'servers.dat'));await add('servers.dat')}catch(e){if(e.code!=='ENOENT')throw e}
  if(!files.some(f=>f.mod_id==='tacz')||!files.some(f=>f.mod_id==='studhub_core'))throw Error('Укажите настоящие TaCZ и StudHub Core в --ids');
  files.sort((a,b)=>a.path.localeCompare(b.path));
  const payload={version:opts.version,minecraft:'1.21.1',neoforge:opts.neoforge,files,incompatible_user_mods:opts.incompatible?opts.incompatible.split(','):[]};
  await fs.writeFile(opts.out,JSON.stringify(signed(payload,await fs.readFile(opts.key)),null,2));
  console.log('Подписано файлов: '+files.length);
}else if(command==='sign'){
  const payload=JSON.parse(await fs.readFile(opts.input,'utf8'));await fs.writeFile(opts.out,JSON.stringify(signed(payload,await fs.readFile(opts.key)),null,2));
}else throw Error('Команды: keygen --out; pack --root --cdn --version --neoforge --key --ids --out; sign --input --key --out');
