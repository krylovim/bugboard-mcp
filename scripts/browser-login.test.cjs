// Added by krylovim, 2026. Licensed under the repository LICENSE.
'use strict';
const test = require('node:test');
const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { spawn } = require('node:child_process');
const { BUGBOARD_URL, parseArgs, privateEnv, cookieHeader, importCookie, runLogin } = require('./browser-login.cjs');

const options = { mcp: process.execPath, profile: 'pilot', channel: 'msedge', timeout: 30 };
const cookie = (changes = {}) => ({ name: 'session', value: 'synthetic-only', domain: 'bugboard.1c.ru',
  path: '/', expires: -1, secure: true, ...changes });

function fakeBrowser() {
  const page = Object.assign(new EventEmitter(), { goto: async (url) => assert.equal(url, BUGBOARD_URL) });
  const context = Object.assign(new EventEmitter(), {
    newPage: async () => page,
    cookies: async (url) => { assert.equal(url, BUGBOARD_URL); return [cookie()]; },
    close: async () => { context.closed = true; context.emit('close'); },
  });
  const browser = Object.assign(new EventEmitter(), {
    newContext: async (settings) => { assert.equal(settings.acceptDownloads, false); return context; },
    close: async () => { browser.closed = true; browser.emit('disconnected'); },
  });
  const chromium = { launch: async (settings) => {
    assert.equal(settings.headless, false);
    assert.equal(settings.channel, 'msedge');
    assert.equal(settings.userDataDir, undefined);
    return browser;
  } };
  return { page, context, browser, chromium };
}

test('only fixed-origin applicable cookies cross the pipe; rejects injection', () => {
  assert.equal(cookieHeader([cookie(), cookie({ domain: 'evil1c.ru' }), cookie({ path: '/other' }),
    cookie({ expires: 1 }), cookie({ partitionKey: 'https://other.test' })]), 'session=synthetic-only');
  assert.throws(() => cookieHeader([cookie({ value: 'x\r\nAuthorization: bad' })]), { code: 'invalid_cookie' });
  assert.throws(() => cookieHeader([cookie({ name: 'a;b' })]), { code: 'invalid_cookie' });
  assert.throws(() => cookieHeader([]), { code: 'no_session' });
});

test('validates options and strips logging/legacy secret environment', () => {
  assert.equal(parseArgs(['--mcp', process.execPath, '--profile', 'pilot']).profile, 'pilot');
  assert.equal(parseArgs(['--mcp', process.execPath]).channel, 'chrome');
  assert.throws(() => parseArgs(['--mcp', 'relative.exe']), { code: 'invalid_arguments' });
  assert.throws(() => parseArgs(['--mcp', process.execPath, '--profile', '../escape']), { code: 'invalid_arguments' });
  assert.throws(() => parseArgs(['--mcp', process.execPath, '--profile', 'CON']), { code: 'invalid_arguments' });
  assert.equal(parseArgs(['--mcp', process.execPath, '--profile', 'Work']).profile, 'work');
  assert.throws(() => parseArgs(['--mcp', process.execPath, '--timeout', 'Infinity']), { code: 'invalid_arguments' });
  assert.deepEqual(privateEnv({ DEBUG: 'pw:*', PWDEBUG: '1', BUGBOARD_COOKIE: 'synthetic-only',
    BUGBOARD_SESSION_ENV: 'old.env', NODE_OPTIONS: '--inspect', BUGBOARD_SESSION_ROOT: 'protected' }),
  { BUGBOARD_SESSION_ROOT: 'protected' });
});

test('quoted cookie values survive verbatim; unrelated malformed cookies cannot block a session', () => {
  assert.equal(cookieHeader([cookie({ value: '"synthetic-only"' })]), 'session="synthetic-only"');
  const signed = 's:synthetic.payload-with_underscore.signature+base64/value==';
  assert.equal(cookieHeader([cookie({ value: signed })]), `session=${signed}`);
  const statuses = [];
  assert.equal(cookieHeader([cookie(), cookie({ name: 'consent', value: '{"enabled":true}' }),
    cookie({ name: 'invalid;name', value: 'ignored' }), cookie({ name: 'inject', value: 'x\r\nInjected: bad' }),
    cookie({ name: 'separator', value: 'x; session=bad' })], Date.now() / 1000, (s) => statuses.push(s)),
  'session=synthetic-only');
  assert.deepEqual(statuses, ['cookie_value_omitted', 'cookie_name_omitted']);
  assert.throws(() => cookieHeader([cookie({ value: 'a'.repeat(33 * 1024) })]), { code: 'invalid_cookie_size' });
});

test('valid session plus JSON consent cookie reaches live import gate', async () => {
  const fake = fakeBrowser();
  fake.context.cookies = async () => [cookie(), cookie({ name: 'consent', value: '{"enabled":true}' })];
  const statuses = [];
  let imports = 0;
  await runLogin(options, { chromium: fake.chromium, emit: (s) => statuses.push(s), confirm: async () => {},
    importCookie: async (_options, value) => { assert.equal(value, 'session=synthetic-only'); imports++; },
  });
  assert.equal(imports, 1);
  assert.deepEqual(statuses, ['cookie_value_omitted', 'saved']);
});

test('real anonymous child pipe carries secret without argv/env/output', async () => {
  const transport = (binary, args, settings) => {
    assert.deepEqual(args, ['auth', 'import', '--stdin', '--profile', 'pilot']);
    assert.equal(settings.env.BUGBOARD_COOKIE, undefined);
    assert.deepEqual(settings.stdio, ['pipe', 'ignore', 'ignore']);
    const script = "let input='';process.stdin.on('data',x=>input+=x);process.stdin.on('end',()=>{process.stdout.write(input);process.stderr.write(input);process.exitCode=input==='session=synthetic-only'?0:1;});";
    return spawn(binary, ['-e', script], settings);
  };
  await importCookie(options, 'session=synthetic-only', transport);
  await assert.rejects(importCookie(options, 'wrong', transport), { code: 'import_failed' });
});

test('success saves once, closes browser and a subsequent launch has fresh context', async () => {
  const contexts = [];
  for (let i = 0; i < 2; i++) {
    const fake = fakeBrowser();
    contexts.push(fake.context);
    let imports = 0;
    const statuses = [];
    await runLogin(options, { chromium: fake.chromium, confirm: async () => {}, emit: (s) => statuses.push(s),
      importCookie: async (_options, value) => { assert.equal(value, 'session=synthetic-only'); imports++; } });
    assert.equal(imports, 1);
    assert.deepEqual(statuses, ['saved']);
    assert.ok(fake.browser.closed && fake.context.closed);
  }
  assert.notEqual(contexts[0], contexts[1]);
});

test('invalid session can retry; cancellation does not invoke save', async () => {
  const fake = fakeBrowser();
  const controller = new AbortController();
  let confirms = 0;
  let imports = 0;
  const statuses = [];
  await assert.rejects(runLogin(options, { chromium: fake.chromium, controller,
    confirm: async () => { if (++confirms === 2) controller.abort('cancelled'); },
    importCookie: async () => { imports++; throw Object.assign(new Error('secret must not print'), { code: 'import_failed' }); },
    emit: (s) => statuses.push(s),
  }), { code: 'cancelled' });
  assert.equal(imports, 1);
  assert.deepEqual(statuses, ['import_failed']);
  assert.ok(fake.browser.closed);
});

test('window close and timeout before confirmation preserve storage', async () => {
  for (const reason of ['cancelled', 'timeout']) {
    const fake = fakeBrowser();
    const controller = new AbortController();
    let imports = 0;
    await assert.rejects(runLogin(options, { chromium: fake.chromium, controller,
      confirm: async () => { reason === 'cancelled' ? fake.page.emit('close') : controller.abort('timeout'); },
      importCookie: async () => { imports++; }, emit: () => {},
    }), { code: reason });
    assert.equal(imports, 0);
    assert.ok(fake.browser.closed && fake.context.closed);
  }
});

test('launch exceptions are redacted and identify the failing stage', async () => {
  await assert.rejects(runLogin(options, {
    chromium: { launch: async () => { throw new Error('session=synthetic-sensitive'); } },
  }), { message: 'browser_launch_failed', code: 'browser_launch_failed' });
});

test('slow navigation keeps owned window open for confirmation; never retries login navigation', async () => {
  const fake = fakeBrowser();
  let navigations = 0;
  fake.page.goto = async (_url, settings) => {
    navigations++;
    assert.equal(settings.waitUntil, 'commit');
    throw Object.assign(new Error('sensitive-redirect-must-not-print'), { name: 'TimeoutError' });
  };
  const statuses = [];
  const chromium = { launch: async (settings) => { assert.equal(settings.channel, 'chrome'); assert.equal(settings.headless, false); return fake.browser; } };
  await runLogin({ ...options, channel: 'chrome' }, { chromium, emit: (s) => statuses.push(s),
    confirm: async () => { assert.equal(fake.browser.closed, undefined); assert.equal(fake.context.closed, undefined); },
    importCookie: async () => {},
  });
  assert.equal(navigations, 1);
  assert.deepEqual(statuses, ['navigation_waiting', 'saved']);
  assert.ok(fake.browser.closed);
});

test('non-timeout navigation and context failures have safe distinct stages', async () => {
  const navigation = fakeBrowser();
  navigation.page.goto = async () => { throw new Error('unsafe-url-and-cookie'); };
  await assert.rejects(runLogin(options, { chromium: navigation.chromium }), { code: 'navigation_failed', message: 'navigation_failed' });
  const context = fakeBrowser();
  context.browser.newContext = async () => { throw new Error('unsafe-details'); };
  await assert.rejects(runLogin(options, { chromium: context.chromium }), { code: 'browser_context_failed', message: 'browser_context_failed' });
});

test('optional installed Edge smoke: isolated cookie API, close and restart',
  { skip: process.env.BUGBOARD_BROWSER_SMOKE !== '1' }, async () => {
    const { chromium } = require('playwright');
    for (let run = 0; run < 2; run++) {
      const browser = await chromium.launch({ channel: 'msedge', headless: true, env: privateEnv() });
      try {
        const context = await browser.newContext();
        assert.deepEqual(await context.cookies(BUGBOARD_URL), []);
        await context.addCookies([{ name: 'test_only', value: 'synthetic-only', url: BUGBOARD_URL }]);
        assert.equal(cookieHeader(await context.cookies(BUGBOARD_URL)), 'test_only=synthetic-only');
        await context.close();
      } finally { await browser.close(); }
    }
  });
