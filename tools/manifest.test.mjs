import {test} from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {createPublicKey,verify,createHash} from 'node:crypto';
test('Administrator CLI hashes and signs actual files; keygen refuses overwrite',async()=>{
  const dir=await fs.mkdtemp(path.join(os.tmpdir(),'studhub-manifest-test-'));
  try{
    const mods=path.join(dir,'pack/mods');await fs.mkdir(mods,{recursive:true});
    await fs.writeFile(path.join(mods,'tacz.jar'),'EXPLICIT TEST FIXTURE, NOT A GAME MOD');
    await fs.writeFile(path.join(mods,'core.jar'),'EXPLICIT CORE TEST FIXTURE');
    const ids=path.join(dir,'ids.json'),key=path.join(dir,'private.pem'),out=path.join(dir,'manifest.json');
    await fs.writeFile(ids,JSON.stringify({'mods/tacz.jar':'tacz','mods/core.jar':'studhub_core'}));
    const cli=path.join(import.meta.dirname,'manifest.mjs');
    execFileSync(process.execPath,[cli,'keygen','--out',key]);
    assert.throws(()=>execFileSync(process.execPath,[cli,'keygen','--out',key],{stdio:'ignore'}));
    execFileSync(process.execPath,[cli,'pack','--root',path.join(dir,'pack'),'--cdn','https://cdn.example.org/modpack','--version','TEST-FIXTURE','--neoforge','21.1.256','--key',key,'--ids',ids,'--out',out]);
    const manifest=JSON.parse(await fs.readFile(out,'utf8'));
    const canonical=v=>Array.isArray(v)?v.map(canonical):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,canonical(v[k])])):v;
    assert.equal(verify(null,Buffer.from(JSON.stringify(canonical(manifest.payload))),createPublicKey(await fs.readFile(key)),Buffer.from(manifest.signature,'base64')),true);
    const tacz=manifest.payload.files.find(f=>f.mod_id==='tacz');
    assert.equal(tacz.sha256,createHash('sha256').update(await fs.readFile(path.join(mods,'tacz.jar'))).digest('hex'));
    assert.equal(manifest.payload.files.length,2);
  }finally{await fs.rm(dir,{recursive:true,force:true})}
});
