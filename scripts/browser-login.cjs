#!/usr/bin/env node
// Added by krylovim, 2026. Licensed under the repository LICENSE.
'use strict';

// This helper owns its browser. It never connects to a normal browser profile,
// starts a debugging TCP listener, or returns a secret through MCP/tool output.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');

const BUGBOARD_URL = 'https://bugboard.1c.ru/';
const MAX_COOKIE_BYTES = 32 * 1024;
const SAFE = new Set(['invalid_arguments', 'interactive_terminal_required', 'browser_unavailable',
  'browser_module_unavailable', 'browser_launch_failed', 'browser_context_failed', 'navigation_failed',
  'browser_failed', 'cancelled', 'timeout', 'no_session', 'invalid_cookie', 'invalid_cookie_size', 'import_failed']);

function failure(code) { return Object.assign(new Error(code), { code }); }

function parseArgs(args) {
  const options = { profile: 'default', channel: 'chrome', timeout: 600 };
  for (let i = 0; i < args.length; i++) {
    const name = args[i];
    if (name === '--help') return { help: true };
    if (!['--mcp', '--profile', '--channel', '--timeout'].includes(name) || !args[i + 1]) {
      throw failure('invalid_arguments');
    }
    options[name.slice(2)] = args[++i];
  }
  options.timeout = Number(options.timeout);
  if (!options.mcp || !path.isAbsolute(options.mcp) || !/^[a-zA-Z0-9_-]{1,64}$/.test(options.profile)
      || !['msedge', 'chrome', 'chromium'].includes(options.channel)
      || !Number.isInteger(options.timeout) || options.timeout < 30 || options.timeout > 1800) {
    throw failure('invalid_arguments');
  }
  if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i.test(options.profile)) throw failure('invalid_arguments');
  options.profile = options.profile.toLowerCase();
  return options;
}

function privateEnv(source = process.env) {
  // Do not inherit a legacy cookie or Playwright protocol/debug logging.
  const result = { ...source };
  for (const key of Object.keys(result)) {
    if (['DEBUG', 'PWDEBUG', 'DEBUG_COLORS', 'BUGBOARD_COOKIE', 'BUGBOARD_SESSION_ENV',
      'NODE_OPTIONS'].includes(key.toUpperCase())) delete result[key];
  }
  return result;
}

function cookieHeader(cookies, now = Date.now() / 1000, report = () => {}) {
  const selected = [];
  const omitted = new Set();
  for (const cookie of cookies) {
    // cookies(URL) performs URL selection too; retain a second explicit boundary.
    const domain = cookie.domain.replace(/^\./, '').toLowerCase();
    if (!['bugboard.1c.ru', '1c.ru'].includes(domain) || cookie.path !== '/'
        || cookie.partitionKey || (cookie.expires !== -1 && cookie.expires <= now)) continue;
    if (!/^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(cookie.name)) {
      omitted.add('cookie_name_omitted');
      continue;
    }
    // RFC 6265 section 4.1.1 allows optional surrounding DQUOTE. Keep the
    // original value verbatim; never percent-encode, strip quotes, or attempt
    // to repair a cookie. Other website cookies (e.g. raw JSON consent data)
    // may not meet this safe header subset and must not block a valid session.
    // Their omission is safe because live authenticated status is still the
    // mandatory gate before the remaining candidate header can be persisted.
    const inner = cookie.value.startsWith('"') && cookie.value.endsWith('"') && cookie.value.length >= 2
      ? cookie.value.slice(1, -1) : cookie.value;
    if (!/^[\x21\x23-\x2B\x2D-\x3A\x3C-\x5B\x5D-\x7E]*$/.test(inner)) {
      omitted.add('cookie_value_omitted');
      continue;
    }
    selected.push(`${cookie.name}=${cookie.value}`);
  }
  for (const reason of omitted) report(reason); // Codes only, no cookie names/values.
  const value = selected.join('; ');
  if (!value) throw failure(omitted.size ? 'invalid_cookie' : 'no_session');
  if (Buffer.byteLength(value) > MAX_COOKIE_BYTES) throw failure('invalid_cookie_size');
  return value;
}

function importCookie(options, value, spawnChild = spawn) {
  return new Promise((resolve, reject) => {
    // Only the reviewed bugboard executable receives the secret over an anonymous
    // pipe. Child stdout/stderr are discarded even if it misbehaves on failure.
    const child = spawnChild(options.mcp, ['auth', 'import', '--stdin', '--profile', options.profile], {
      stdio: ['pipe', 'ignore', 'ignore'], windowsHide: true, shell: false, env: privateEnv(),
    });
    let finished = false;
    const finish = (ok) => {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      ok ? resolve() : reject(failure('import_failed'));
    };
    const timer = setTimeout(() => { child.kill(); finish(false); }, 90_000);
    child.once('error', () => finish(false));
    child.once('close', (code) => finish(code === 0));
    child.stdin.on('error', () => finish(false));
    child.stdin.end(value, 'utf8');
  });
}

function confirmInTerminal(signal) {
  return new Promise((resolve, reject) => {
    const input = readline.createInterface({ input: process.stdin, output: process.stdout });
    let done = false;
    const finish = (error) => {
      if (done) return;
      done = true;
      signal.removeEventListener('abort', abort);
      input.close();
      error ? reject(error) : resolve();
    };
    const abort = () => finish(failure(signal.reason === 'timeout' ? 'timeout' : 'cancelled'));
    input.once('close', () => finish(failure('cancelled')));
    input.once('SIGINT', abort);
    signal.addEventListener('abort', abort, { once: true });
    if (signal.aborted) return abort();
    input.question('Sign in to Bugboard in the separate window. Then press Enter here; Ctrl+C cancels.\n', () => finish());
  });
}

async function runLogin(options, dependencies = {}) {
  const controller = dependencies.controller || new AbortController();
  const signal = controller.signal;
  const emit = dependencies.emit || ((status) => process.stdout.write(`${JSON.stringify({ status })}\n`));
  let browser;
  let context;
  let importing = false;
  const stop = () => controller.abort('cancelled');
  const timer = setTimeout(() => controller.abort('timeout'), options.timeout * 1000);
  const interrupt = () => { if (!importing) void context?.close().catch(() => {}); };
  signal.addEventListener('abort', interrupt);
  process.once('SIGINT', stop);
  process.once('SIGTERM', stop);
  try {
    let chromium;
    try { chromium = dependencies.chromium || require('playwright').chromium; }
    catch { throw failure('browser_module_unavailable'); }
    try {
      browser = await chromium.launch({
        headless: false, ...(options.channel === 'chromium' ? {} : { channel: options.channel }),
        env: privateEnv(), timeout: 30_000,
      });
    } catch { throw failure('browser_launch_failed'); }
    browser.once('disconnected', stop);
    try { context = await browser.newContext({ acceptDownloads: false }); }
    catch { throw failure('browser_context_failed'); }
    context.once('close', stop);
    let page;
    try { page = await context.newPage(); }
    catch { throw failure('browser_context_failed'); }
    page.once('close', stop);
    // A slow SSO redirect/page load is not a failed browser launch. Wait only
    // for navigation to commit, then let the user finish interactive login.
    // On a timeout leave this owned window open; an automatic goto retry could
    // interrupt an MFA/login page that has already appeared. The overall login
    // deadline still bounds this state, and confirmation validates live auth.
    try {
      await page.goto(BUGBOARD_URL, { waitUntil: 'commit', timeout: Math.min(60_000, options.timeout * 1000) });
    } catch (error) {
      if (error.name !== 'TimeoutError') throw failure('navigation_failed');
      if (!signal.aborted) emit('navigation_waiting');
    }
    while (!signal.aborted) {
      await (dependencies.confirm || confirmInTerminal)(signal);
      if (signal.aborted) break;
      let value;
      try {
        value = cookieHeader(await context.cookies(BUGBOARD_URL), Date.now() / 1000, emit);
        // Once confirmed, finish validation + atomic save before honouring a
        // window-close event. Killing a commit midway would give ambiguous state.
        importing = true;
        await (dependencies.importCookie || importCookie)(options, value);
        emit('saved');
        return 'saved';
      } catch (error) {
        if (!['no_session', 'invalid_cookie', 'invalid_cookie_size', 'import_failed'].includes(error.code)) throw error;
        emit(error.code);
      } finally {
        value = undefined; // Strings cannot be reliably zeroed in JavaScript.
        importing = false;
      }
    }
    throw failure(signal.reason === 'timeout' ? 'timeout' : 'cancelled');
  } catch (error) {
    const code = signal.aborted ? (signal.reason === 'timeout' ? 'timeout' : 'cancelled')
      : SAFE.has(error.code) ? error.code : 'browser_failed';
    throw failure(code); // Never print browser/URL/child exception details.
  } finally {
    clearTimeout(timer);
    process.removeListener('SIGINT', stop);
    process.removeListener('SIGTERM', stop);
    signal.removeEventListener('abort', interrupt);
    await context?.close().catch(() => {});
    await browser?.close().catch(() => {});
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.help) {
    process.stdout.write([
      'Usage: node browser-login.cjs --mcp ABSOLUTE_EXE [--profile default]',
      '       [--channel msedge|chrome|chromium] [--timeout 600]',
      'Requires Node >=20, Playwright (validated: 1.62.1), installed Chrome by default,',
      'and a reviewed MCP executable implementing auth import --stdin.',
      'Run in an interactive terminal. Sign in in the separate browser window,',
      'then press Enter in the terminal. Ctrl+C/window close cancels before save.',
      'Only local pipes carry cookies. No existing browser profile is accessed.',
      'Setup and acceptance limits: docs/browser-login-research.md',
      '',
    ].join('\n'));
    return;
  }
  if (!process.stdin.isTTY) throw failure('interactive_terminal_required');
  try { if (!fs.statSync(options.mcp).isFile()) throw failure('invalid_arguments'); }
  catch { throw failure('invalid_arguments'); }
  // DEBUG is evaluated when Playwright is loaded. Remove it in this process too.
  for (const key of Object.keys(process.env)) {
    if (['DEBUG', 'PWDEBUG', 'DEBUG_COLORS'].includes(key.toUpperCase())) delete process.env[key];
  }
  try { require.resolve('playwright'); } catch { throw failure('browser_module_unavailable'); }
  await runLogin(options);
}

if (require.main === module) main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ status: SAFE.has(error.code) ? error.code : 'browser_failed' })}\n`);
  process.exitCode = 1;
});

module.exports = { BUGBOARD_URL, parseArgs, privateEnv, cookieHeader, importCookie, runLogin };
