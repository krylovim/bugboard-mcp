// Copyright 2026 krylovim. Apache-2.0 WITH Commons Clause 1.0; see LICENSE.
// Read-only acceptance test. Pass an external BUGBOARD_SESSION_ENV or explicitly
// select BUGBOARD_SESSION_STORE=dpapi and BUGBOARD_PROFILE, plus a built
// binary path. Does not read, print, copy or persist the cookie itself.
const {spawn} = require('node:child_process');
const {createInterface} = require('node:readline');
const assert = require('node:assert/strict');
const path = require('node:path');
if (!process.env.BUGBOARD_SESSION_ENV && process.env.BUGBOARD_SESSION_STORE !== 'dpapi') {
  throw new Error('Set external BUGBOARD_SESSION_ENV or BUGBOARD_SESSION_STORE=dpapi');
}
const env = {...process.env};
delete env.BUGBOARD_COOKIE;
const child = spawn(path.resolve(process.argv[2] || 'target/debug/bugboard-mcp.exe'), ['--stdio'],
  {env, stdio:['pipe','pipe','pipe'], windowsHide:true});
child.stderr.resume();
const lines = createInterface({input:child.stdout});
const pending = new Map();
let sequence = 0;
lines.on('line', line => {
  const result = JSON.parse(line), waiter = pending.get(result.id);
  if (waiter) { clearTimeout(waiter.timer); pending.delete(result.id); waiter.resolve(result); }
});
function request(method, params) {
  const id = ++sequence;
  return new Promise((resolve,reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Timeout: ${method}`)); },60000);
    pending.set(id,{resolve,reject,timer});
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');
  });
}
child.on('error',() => { for (const p of pending.values()) p.reject(new Error('MCP process failed')); });
async function call(name,args={},expectedError) {
  const r = await request('tools/call',{name,arguments:args});
  assert(!r.error,`${name}: protocol error`);
  const value = r.result.structuredContent ?? JSON.parse(r.result.content.find(c=>c.type==='text').text);
  if (expectedError) { assert.equal(value.error?.code,expectedError); return value; }
  if (r.result.isError) throw new Error(`${name}: ${value.error?.code}; ${JSON.stringify(value.error?.details || {})}`);
  return value;
}
const checks = [];
function pass(name, details={}) { checks.push(name); console.log(JSON.stringify({pass:name,...details})); }
(async()=>{
  try {
    const init = await request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'scoped-search-acceptance',version:'1'}});
    assert(!init.error);
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized',params:{}})+'\n');
    assert.equal((await call('bugboard_auth_status')).authenticated,true);
    pass('existing session authenticated');
    const products = await call('project_list',{query:'Бухгалтерия',limit:50});
    assert(products.count > 1); assert.equal(products.selection,'candidates_only');
    const bp = products.projects.filter(p=>p.project_code==='bp3'); assert.equal(bp.length,1);
    const erps = await call('project_list',{query:'ERP',limit:50});
    // Deliberately require an exact official title; never silently pick the first candidate.
    console.log(JSON.stringify({erpCandidates:erps.projects.map(p=>({code:p.project_code,title:p.title}))}));
    const erp = erps.projects.filter(p=>p.title==='1С:ERP Управление предприятием 2.0');
    assert.equal(erp.length,1,'Expected one exact ERP title; inspect candidates and update fixture');
    pass('ambiguous product returns candidates',{bpCode:bp[0].project_code,erpCode:erp[0].project_code});
    const scoped = code=>call('bug_search',{query:'НДС',project_code:code,mode:'text',limit:3});
    const [a,b] = await Promise.all([scoped('bp3'),scoped(erp[0].project_code)]);
    for (const [r,code] of [[a,'bp3'],[b,erp[0].project_code]]) {
      assert(r.count>0 && r.count<=3);
      assert(r.bugs.every(bug=>bug.project.project_code===code && bug.url));
      assert.equal(r.filter.project.project_code,code); assert.equal(r.filter.server_filtered,true);
      assert.equal(r.coverage.complete,false);
    }
    pass('concurrent BP and ERP scoped searches',{bpNumbers:a.bugs.map(b=>b.number),erpNumbers:b.bugs.map(b=>b.number)});
    const description = await call('bug_search',{query:'10144553',project_code:'bp3',mode:'number',limit:3});
    assert.equal(description.count,1);
    assert(!description.bugs[0].title.toLowerCase().includes('ндс'));
    assert(description.bugs[0].description.toLowerCase().includes('ндс'));
    // The broader limited page is not guaranteed to include this card; query its
    // description more specifically, then check server text results contain it.
    const bodyQuery = description.bugs[0].description.match(/.{0,20}НДС.{0,20}/iu)?.[0].trim();
    assert(bodyQuery);
    const bodyResult = await call('bug_search',{query:bodyQuery,project_code:'bp3',mode:'text',limit:50});
    assert(bodyResult.bugs.some(b=>b.number==='10144553' && b.matched_fields.includes('description')));
    pass('description-only match',{number:'10144553'});
    const limited = await call('bug_search',{query:'НДС',project_handle:bp[0].project_handle,limit:1});
    assert.equal(limited.count,1); assert.equal(limited.has_more,true);
    assert.equal(limited.bugs[0].project.project_code,'bp3');
    const boundary = await call('bug_search',{query:'НДС',project_code:'bp3',mode:'text',limit:50});
    assert(boundary.count<=50 && boundary.bugs.every(b=>b.project.project_code==='bp3'));
    pass('scope before limit and lookahead',{limit50Count:boundary.count,hasMore:boundary.has_more});
    await call('bug_search',{query:'НДС',project_code:'codex-nonexistent-product'},'unknown_project');
    await call('bug_search',{query:'НДС',project_code:'bp3',project_handle:erp[0].project_handle},'invalid_arguments');
    await call('bug_search',{query:'НДС',project_handle:'invalid-handle'},'invalid_reference');
    pass('unknown product and conflicting selectors rejected');
    for (const query of ['codexnomatcha7c43e908b','codex_zz%[x]"\\']) {
      assert.equal((await call('bug_search',{query,project_code:'bp3',mode:'text'})).count,0);
    }
    for (const query of ['%','_','"','\\','[']) {
      const r = await call('bug_search',{query,project_code:'bp3',mode:'text',limit:3});
      assert(r.bugs.every(b=>b.title.includes(query) || b.description.includes(query)),`Not literal: ${query}`);
    }
    const lower = await call('bug_search',{query:'ндс',project_code:'bp3',mode:'text',limit:3});
    assert(lower.count>0 && lower.bugs.every(b=>b.project.project_code==='bp3'));
    pass('empty results special characters and Cyrillic case');
    const duplicates = await call('bug_search',{query:'00-00843958',mode:'number',limit:50});
    assert(duplicates.count>1); assert.equal(duplicates.ambiguous,true);
    assert(new Set(duplicates.bugs.map(b=>b.project.project_code)).size>1);
    await call('bug_get',{bug_number:'00-00843958'},'ambiguous_bug_number');
    const candidate = duplicates.bugs.find(b=>b.project.project_code===erp[0].project_code);
    assert(candidate,'Expected ERP duplicate candidate');
    const exact = await call('bug_search',{query:'00-00843958',project_code:candidate.project.project_code,mode:'number'});
    assert.equal(exact.count,1); assert.equal(exact.ambiguous,false);
    const card = await call('bug_get',{bug_handle:exact.bugs[0].bug_handle});
    assert.equal(card.number,'00-00843958');
    pass('hyphenated duplicate numbers and safe bug_get',{products:duplicates.bugs.map(b=>b.project.project_code)});
    const oldText = await call('bug_search',{query:'ошибка',limit:1});
    assert.equal(oldText.filter.mode,'full_text'); assert.equal(oldText.count,1);
    assert(oldText.bugs[0].project.project_code);
    const oldEmpty = await call('bug_search',{query:'codexnomatcha7c43e908b',limit:3}); assert.equal(oldEmpty.count,0);
    const oldNumber = await call('bug_search',{query:'60021238',limit:3}); assert.equal(oldNumber.bugs[0].number,'60021238');
    const oldGet = await call('bug_get',{bug_number:'60021238'}); assert.equal(oldGet.number,'60021238');
    assert((await call('project_list',{limit:3})).count>0);
    pass('legacy text empty number get and project_list calls');
    console.log(JSON.stringify({status:'PASS',checks:checks.length}));
  } catch(e) { console.error(e.message); process.exitCode=1; }
  finally { child.kill(); lines.close(); for (const p of pending.values()) clearTimeout(p.timer); }
})();
