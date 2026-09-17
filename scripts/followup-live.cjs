// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
// Windows live acceptance: imports the existing external env session into a new
// disposable DPAPI profile. Never reads the source cookie or alters MCP config.
'use strict';
const {spawn, spawnSync} = require('node:child_process');
const {createInterface} = require('node:readline');
const {randomUUID} = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');

const binary = path.resolve(process.argv[2] || 'target/debug/bugboard-mcp.exe');
assert.equal(process.platform, 'win32', 'DPAPI acceptance requires Windows');
assert(process.env.BUGBOARD_SESSION_ENV, 'Set the existing external BUGBOARD_SESSION_ENV');
const profile = `acceptance-${randomUUID()}`;
const root = fs.mkdtempSync(path.join(os.tmpdir(), 'bugboard-acceptance-'));
const env = {...process.env, BUGBOARD_PROFILE:profile,
  BUGBOARD_SESSION_STORE:'dpapi', BUGBOARD_SESSION_ROOT:path.join(root, 'protected-sessions'),
  BUGBOARD_CACHE_DIR:path.join(root, 'cache')};
delete env.BUGBOARD_COOKIE;

function auth(operation, input) {
  const args = ['auth', operation, '--profile', profile];
  if (input !== undefined) args.push('--stdin');
  const r = spawnSync(binary, args, {env, input, encoding:'utf8', windowsHide:true, timeout:90000});
  // Never echo subprocess output on assertion errors, even though the CLI is
  // designed to return only fixed status codes.
  assert(!r.error, 'auth subprocess failed');
  return {status:r.status, output:r.stdout};
}
async function connect() {
  const child = spawn(binary, ['--stdio'], {env, stdio:['pipe','pipe','pipe'],windowsHide:true});
  child.stderr.resume();
  const pending = new Map(); let sequence = 0;
  const lines = createInterface({input:child.stdout});
  lines.on('line', line => {
    let r; try { r=JSON.parse(line); } catch { return; }
    const waiter=pending.get(r.id);
    if(waiter) {clearTimeout(waiter.timer); pending.delete(r.id); waiter.resolve(r);}
  });
  function request(method, params) {
    return new Promise((resolve,reject)=>{
      const id=++sequence;
      const timer=setTimeout(()=>{pending.delete(id);reject(new Error('MCP timeout'));},90000);
      pending.set(id,{resolve,reject,timer});
      child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');
    });
  }
  child.on('error',()=>{for(const p of pending.values()){clearTimeout(p.timer);p.reject(new Error('MCP launch failed'));}pending.clear();});
  const close=()=>{lines.close();child.stdin.end();child.kill();for(const p of pending.values())clearTimeout(p.timer);};
  try {
    const init=await request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'followup-acceptance',version:'1'}});
    assert(!init.error, 'MCP initialize failed');
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized',params:{}})+'\n');
  } catch(e) {close();throw e;}
  return {close, call:async(name,args={},expectedError)=>{
    const r=await request('tools/call',{name,arguments:args});
    assert(!r.error, `MCP ${name} protocol failure`);
    const value=r.result.structuredContent || JSON.parse(r.result.content.find(c=>c.type==='text').text);
    if(expectedError) {assert.equal(r.result.isError,true);assert.equal(value.error?.code,expectedError);}
    else assert(!r.result.isError, `MCP ${name} failed`);
    return value;
  }};
}
function pass(name){console.log(JSON.stringify({pass:name}));}
(async()=>{
  let client; let imported=false;
  try {
    assert.equal(auth('import-env').status,0,'Verified legacy import failed'); imported=true;
    assert.equal(auth('status').status,0,'Stored session restart failed');
    pass('verified legacy migration and DPAPI restart');
    const record=path.join(env.BUGBOARD_SESSION_ROOT,profile+'.dpapi');
    const original=fs.readFileSync(record);
    assert.notEqual(auth('import','').status,0,'Empty input must cancel');
    assert(fs.readFileSync(record).equals(original),'Cancel replaced the working session');
    assert.notEqual(auth('import','invalid-header-without-equals').status,0,'Invalid input must fail');
    assert(fs.readFileSync(record).equals(original),'Failed validation replaced the working session');
    pass('cancel and rejection preserve stored session');
    const corrupt=Buffer.from(original);corrupt[corrupt.length-1]^=1;
    fs.writeFileSync(record,corrupt);
    try {
      assert.notEqual(auth('status').status,0);
      client=await connect();
      await client.call('bugboard_auth_status',{},'protected_session_error');
      client.close();client=undefined;
    } finally {fs.writeFileSync(record,original);}
    pass('corrupt protected profile fails closed despite valid legacy source');
    client=await connect();
    assert.equal((await client.call('bugboard_auth_status')).authenticated,true);
    const args={query:'Бухгалтерия',limit:50,metadata_only:true};
    const first=await client.call('project_list',args);
    assert.equal(first.cache.source,'server');assert.equal(first.cache.persistent,true);
    assert(first.projects.some(p=>p.project_code==='bp3'));
    assert.equal((await client.call('project_list',args)).cache.source,'memory');
    client.close();client=await connect();
    const disk=await client.call('project_list',args);
    assert.equal(disk.cache.source,'disk');assert.equal(disk.cache.stale,false);
    assert.equal(disk.coverage.complete,false);
    assert(disk.projects.every(p=>p.project_handle===null));
    const legacy=await client.call('project_list',{query:args.query,limit:50});
    assert(legacy.projects.every(p=>typeof p.project_handle==='string'));
    assert.equal((await client.call('project_list',{...args,force_refresh:true})).cache.source,'server');
    pass('memory cache, disk restart, live handles and forced refresh');
    for(const code of ['bp3','erp2']) {
      const r=await client.call('bug_search',{query:'НДС',mode:'text',project_code:code,limit:2});
      assert(r.count>0 && r.bugs.every(b=>b.project.project_code===code));
    }
    pass('BP and ERP searches using protected session');
    client.close();client=undefined;
    assert.equal(auth('import-env').status,0);
    client=await connect();
    assert.equal((await client.call('project_list',args)).cache.source,'server');
    pass('credential renewal gets an isolated cache generation');
  } finally {
    client?.close();
    if(imported) assert.equal(auth('delete').status,0,'Temporary profile cleanup failed');
  }
})().catch(()=>{console.error(JSON.stringify({status:'failed',note:'Inspect the named acceptance stage; raw process output suppressed'}));process.exitCode=1;});
