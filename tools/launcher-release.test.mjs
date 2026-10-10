import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {generateKeyPairSync,verify,createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
test('Launcher publication signs exact installer bytes and exports only public configuration',async()=>{
 const dir=await fs.mkdtemp(path.join(os.tmpdir(),'studhub-release-'));
 try{
  const {privateKey,publicKey}=generateKeyPairSync('ed25519');
  await fs.writeFile(path.join(dir,'key.pem'),privateKey.export({type:'pkcs8',format:'pem'}));await fs.writeFile(path.join(dir,'Setup.exe'),'installer bytes');
  execFileSync(process.execPath,[path.resolve('tools/launcher-release.mjs'),'--exe',path.join(dir,'Setup.exe'),'--version','0.4.8','--url','https://cdn.example/Setup.exe','--update-url','https://updates.example/launcher.json','--key',path.join(dir,'key.pem'),'--out',path.join(dir,'release')]);
  const env=JSON.parse(await fs.readFile(path.join(dir,'release/launcher-manifest.json'),'utf8'));
  function sort(v){return v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sort(v[k])])):v}
  assert.ok(verify(null,Buffer.from(JSON.stringify(sort(env.payload))),publicKey,Buffer.from(env.signature,'base64')));
  assert.equal(env.payload.artifacts['windows-amd64'].sha256,createHash('sha256').update('installer bytes').digest('hex'));
  assert.equal(env.payload.artifacts['windows-amd64'].installer,'nsis');
  const d=JSON.parse(await fs.readFile(path.join(dir,'release/deployment.json'),'utf8'));assert.equal(d.launcher_public_key,publicKey.export({type:'spki',format:'der'}).subarray(-32).toString('base64'));assert.deepEqual(d.trusted_origins,['https://cdn.example','https://updates.example']);
  assert.deepEqual((await fs.readdir(path.join(dir,'release'))).sort(),['deployment.json','launcher-manifest.json']);
 }finally{await fs.rm(dir,{recursive:true,force:true})}
});
