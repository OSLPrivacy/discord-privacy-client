import { existsSync, globSync } from 'node:fs';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

const DEFAULT_CHROME_ARGS = [
  '--headless=new',
  '--remote-debugging-port=0',
  '--no-sandbox',
  '--disable-gpu',
  '--force-device-scale-factor=1',
  'about:blank',
];

export function locateChrome() {
  const envPath = process.env.OSL_CHROME;
  if (envPath && existsSync(envPath)) return envPath;
  const home = os.homedir();
  const patterns = [
    `${home}/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell`,
    `${home}/.cache/ms-playwright/chromium_headless_shell-*/chrome-linux/headless_shell`,
    `${home}/.cache/ms-playwright/chromium-*/chrome-linux64/chrome`,
    `${home}/.cache/ms-playwright/chromium-*/chrome-linux/chrome`,
  ];
  for (const pattern of patterns) {
    const matches = globSync(pattern).sort();
    if (matches.length > 0) return matches[matches.length - 1];
  }
  throw new Error('no Chrome/Chromium binary found; set OSL_CHROME to a local browser binary');
}

export class CDPClient {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    ws.addEventListener('message', (event) => this.onMessage(event));
  }

  onMessage(event) {
    const message = JSON.parse(event.data);
    if (message.id !== undefined) {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
      return;
    }
    if (!message.method) return;
    const listeners = this.listeners.get(message.method);
    if (listeners) for (const listener of [...listeners]) listener(message.params, message.sessionId);
  }

  send(method, params = {}, sessionId) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const payload = { id, method, params };
      if (sessionId) payload.sessionId = sessionId;
      this.ws.send(JSON.stringify(payload));
    });
  }

  on(method, listener) {
    if (!this.listeners.has(method)) this.listeners.set(method, new Set());
    this.listeners.get(method).add(listener);
    return () => this.listeners.get(method)?.delete(listener);
  }

  once(method, predicate = () => true) {
    return new Promise((resolve) => {
      const off = this.on(method, (params, sessionId) => {
        if (!predicate(params, sessionId)) return;
        off();
        resolve(params);
      });
    });
  }
}

export async function connectToChrome(chromeChild, { timeoutMs = 15_000 } = {}) {
  let buffer = '';
  const wsUrl = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error('timed out waiting for Chrome CDP')), timeoutMs);
    chromeChild.stderr.on('data', (chunk) => {
      buffer += chunk.toString('utf8');
      const match = buffer.match(/DevTools listening on (ws:\/\/\S+)/);
      if (match) {
        clearTimeout(timeout);
        resolve(match[1]);
      }
    });
    chromeChild.once('exit', (code) => {
      clearTimeout(timeout);
      reject(new Error(`Chrome exited before CDP was ready (${code})`));
    });
    chromeChild.once('error', (error) => {
      clearTimeout(timeout);
      reject(new Error(`Chrome failed to launch: ${error.message}`));
    });
  });
  const cdpPort = new URL(wsUrl).port;
  const versionInfo = await fetch(`http://127.0.0.1:${cdpPort}/json/version`).then((response) => response.json());
  const ws = new WebSocket(versionInfo.webSocketDebuggerUrl || wsUrl);
  await new Promise((resolve, reject) => {
    ws.addEventListener('open', resolve, { once: true });
    ws.addEventListener('error', () => reject(new Error('WebSocket connection failed')), { once: true });
  });
  return { cdp: new CDPClient(ws), ws, versionInfo };
}

function createPage(cdp) {
  let targetId;
  let sessionId;
  return {
    async initialize() {
      ({ targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' }));
      ({ sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true }));
      await cdp.send('Page.enable', {}, sessionId);
      await cdp.send('Runtime.enable', {}, sessionId);
      return this;
    },
    async navigate(url, { timeoutMs = 15_000 } = {}) {
      await cdp.send('Page.navigate', { url }, sessionId);
      const deadline = Date.now() + timeoutMs;
      while (Date.now() < deadline) {
        const readyState = await cdp.send('Runtime.evaluate', {
          expression: 'document.readyState',
          returnByValue: true,
        }, sessionId);
        if (readyState.result.value === 'complete') return;
        await delay(25);
      }
      throw new Error(`timed out waiting for page load: ${url}`);
    },
    async evaluate(expression) {
      const evaluated = await cdp.send('Runtime.evaluate', { expression, returnByValue: true }, sessionId);
      if (evaluated.exceptionDetails) throw new Error(evaluated.exceptionDetails.text || 'page evaluation threw');
      return evaluated.result.value;
    },
    async screenshot(params = {}) {
      const captured = await cdp.send('Page.captureScreenshot', { format: 'png', ...params }, sessionId);
      return Buffer.from(captured.data, 'base64');
    },
    send(method, params = {}) {
      return cdp.send(method, params, sessionId);
    },
    async close() {
      if (!targetId) return;
      const id = targetId;
      targetId = undefined;
      await Promise.race([
        cdp.send('Target.closeTarget', { targetId: id }).catch(() => {}),
        delay(1_000),
      ]);
    },
  };
}

export async function launchChrome({ chromePath = locateChrome(), args = DEFAULT_CHROME_ARGS, timeoutMs } = {}) {
  const child = spawn(chromePath, args, { stdio: ['ignore', 'ignore', 'pipe'] });
  try {
    const connection = await connectToChrome(child, { timeoutMs });
    return {
      ...connection,
      child,
      async openPage() {
        return createPage(connection.cdp).initialize();
      },
      async close() {
        try { connection.ws.close(); } catch { /* already closed */ }
        if (child.exitCode === null) child.kill('SIGTERM');
      },
    };
  } catch (error) {
    if (!child.killed) child.kill('SIGTERM');
    throw error;
  }
}
