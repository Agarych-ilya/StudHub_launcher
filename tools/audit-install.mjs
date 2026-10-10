import fs from 'node:fs/promises';
import {createReadStream} from 'node:fs';
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import path from 'node:path';
const root=path.resolve(process.argv[2]);
async function hash(file,algorithm){const h=createHash(algorithm);for await(const b of createReadStream(file))h.update(b);return h.digest('hex')}
const v=JSON.parse(await fs.readFile(path.join(root,'versions/1.21.1/1.21.1.json'),'utf8'));
const clientHash=await hash(path.join(root,'versions/1.21.1/1.21.1.jar'),'sha1');
if(clientHash!==v.downloads.client.sha1)throw Error('Minecraft client hash mismatch');
const index=JSON.parse(await fs.readFile(path.join(root,'assets/indexes',v.assetIndex.id+'.json'),'utf8'));
let verified=0;for(const a of Object.values(index.objects)){
  if(await hash(path.join(root,'assets/objects',a.hash.slice(0,2),a.hash),'sha1')!==a.hash)throw Error('Asset hash mismatch: '+a.hash);verified++;
}
const completed=JSON.parse(await fs.readFile(path.join(root,'.studhub/neoforge-21.1.256.complete.json'),'utf8'));
if(Object.keys(completed).length!==5)throw Error('Incomplete NeoForge');
for(const [file,expected] of Object.entries(completed))if(await hash(path.join(root,file),'sha256')!==expected)throw Error('NeoForge output hash mismatch: '+file);
const java=path.join(root,'runtime/java21',process.platform==='win32'?'bin/java.exe':process.platform==='darwin'?'jre.bundle/Contents/Home/bin/java':'bin/java');
let javaVersion;
try{execFileSync(java,['-version'],{windowsHide:true,stdio:['ignore','pipe','pipe']})}catch(e){throw Error('Java failed: '+e)}
const j=await new Promise((resolve,reject)=>{import('node:child_process').then(({execFile})=>execFile(java,['-version'],{windowsHide:true},(e,out,err)=>e?reject(e):resolve(err||out)))});
javaVersion=j.trim();
if(!javaVersion.includes('version "21'))throw Error('Java 21 is required');
console.log(JSON.stringify({minecraft:'1.21.1',client_sha1:clientHash,assets_verified:verified,neoforge:'21.1.256',verified_neoforge_outputs:completed,java:javaVersion},null,2));
