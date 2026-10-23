// The browser build in a headless Chrome, scripted (no window, no OS key events): console output, screenshots,
// keys. usage: node web/headless.mjs URL OUTDIR "wait:ms,shot:name,key:Enter,click:x:y,metrics:W:H:SW:SH,media:feature:value,gestureb64:BASE64JS,mobile:W:H,touch:start|move|end:X:Y[:ID],eval:js,evalb64:BASE64JS,files:SELECTOR:PATH|PATH,drop:X:Y:PATH,initb64:BASE64JS"
import { spawn } from 'node:child_process';
import { writeFileSync, mkdirSync } from 'node:fs';
const [url, out, script] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
const port = 9333 + Math.floor(Math.random() * 500);
const chrome = spawn('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome', [
  '--headless=new', `--remote-debugging-port=${port}`, `--user-data-dir=${out}/profile`, '--enable-unsafe-webgpu',
  '--window-size=960,760', '--autoplay-policy=no-user-gesture-required', 'about:blank'], { stdio: 'ignore' });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let ws, id = 0; const pending = new Map(); const log = [];
const send = (method, params = {}, sessionId) => new Promise((res) => { const i = ++id; pending.set(i, res); ws.send(JSON.stringify({ id: i, method, params, sessionId })); });
for (let k = 0; k < 50; k++) { try { const r = await fetch(`http://127.0.0.1:${port}/json/version`); if (r.ok) { const j = await r.json(); ws = new WebSocket(j.webSocketDebuggerUrl); break; } } catch {} await sleep(200); }
await new Promise((r) => ws.addEventListener('open', r));
ws.addEventListener('message', (m) => { const d = JSON.parse(m.data); if (d.id && pending.has(d.id)) { pending.get(d.id)(d.result ?? d); pending.delete(d.id); }
  if (d.method === 'Runtime.consoleAPICalled') log.push(`${((Date.now()-t0)/1000).toFixed(1)}s ${d.params.type}: ${d.params.args.map((a) => a.value ?? a.description ?? '').join(' ')}`);
  if (d.method === 'Runtime.exceptionThrown') log.push(`${((Date.now()-t0)/1000).toFixed(1)}s EXCEPTION ${d.params.exceptionDetails.exception?.description ?? d.params.exceptionDetails.text}`); });
const t0 = Date.now();
const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
const s = (m, p) => send(m, p, sessionId);
await s('Runtime.enable'); await s('Page.enable');
await s('Emulation.setFocusEmulationEnabled', { enabled: true });
await s('Page.navigate', { url });
for (const step of script.split(',')) {
  const [op, a, b] = step.split(':');
  if (op === 'wait') await sleep(+a);
  if (op === 'shot') { const r = await s('Page.captureScreenshot', { format: 'png' }); writeFileSync(`${out}/${a}.png`, Buffer.from(r.data, 'base64')); log.push(`${((Date.now()-t0)/1000).toFixed(1)}s shot ${a}`); }
  if (op === 'click') { for (const type of ['mousePressed', 'mouseReleased']) await s('Input.dispatchMouseEvent', { type, x: +a, y: +b, button: 'left', clickCount: 1 }); }
  if (op === 'key') { const codes = { Enter: 13, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, Escape: 27 };
    const k = a; const kc = codes[k] ?? k.toUpperCase().charCodeAt(0); const code = k.length === 1 ? `Key${k.toUpperCase()}` : k;
    await s('Input.dispatchKeyEvent', { type: 'keyDown', key: k, code, windowsVirtualKeyCode: kc });
    await sleep(60);
    await s('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: kc }); }
  // metrics:W:H:SW:SH: the page's view W x H on a screen SW x SH (CSS pixels; 0: as it is)
  if (op === 'metrics') { const [, w, h, sw, sh] = step.split(':').map(Number);
    await s('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 0, mobile: false, screenWidth: sw, screenHeight: sh }); }
  // media:display-mode:fullscreen (media:: clears): CSS media features as a browser in that state reports them
  if (op === 'media') await s('Emulation.setEmulatedMedia', { features: a ? [{ name: a, value: b }] : [] });
  // mobile:W:H a phone's screen (touch, no mouse); touch:start|move|end:X:Y[:ID] a finger
  if (op === 'mobile') { await s('Emulation.setDeviceMetricsOverride', { width: +a, height: +b, deviceScaleFactor: 2, mobile: true, screenWidth: +a, screenHeight: +b });
    await s('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 }); }
  if (op === 'touch') { const [, kind, x, y, id] = step.split(':');
    const type = { start: 'touchStart', move: 'touchMove', end: 'touchEnd' }[kind];
    await s('Input.dispatchTouchEvent', { type, touchPoints: kind === 'end' ? [] : [{ x: +x, y: +y, id: +(id ?? 0) }] }); }
  if (op === 'evalb64') { const r = await s('Runtime.evaluate', { expression: Buffer.from(a, 'base64').toString(), awaitPromise: true, returnByValue: true }); log.push(`eval: ${JSON.stringify(r.result?.value)}`); }
  // gestureb64:BASE64JS: as evalb64, run as if on the user's input (full screen, sound)
  if (op === 'gestureb64') { const r = await s('Runtime.evaluate', { expression: Buffer.from(a, 'base64').toString(), awaitPromise: true, returnByValue: true, userGesture: true }); log.push(`eval: ${JSON.stringify(r.result?.value ?? r.exceptionDetails?.exception?.description)}`); }
  // files:SELECTOR:PATH|PATH files picked in an <input type=file>; drop:X:Y:PATH a file or folder dropped there (a
  // folder's handle: DataTransferItem.getAsFileSystemHandle(), what a test gives a page for showDirectoryPicker)
  if (op === 'files') { const { root } = await s('DOM.getDocument');
    const { nodeId } = await s('DOM.querySelector', { nodeId: root.nodeId, selector: a });
    await s('DOM.setFileInputFiles', { nodeId, files: step.slice(op.length + a.length + 2).split('|') }); }
  if (op === 'drop') { const data = { items: [], files: [step.split(':').slice(3).join(':')], dragOperationsMask: 1 };
    for (const type of ['dragEnter', 'dragOver', 'drop']) await s('Input.dispatchDragEvent', { type, x: +a, y: +b, data }); }
  // initb64:BASE64JS run before the page's own scripts from the next load on (then eval:location.reload())
  if (op === 'initb64') await s('Page.addScriptToEvaluateOnNewDocument', { source: Buffer.from(a, 'base64').toString() });
  if (op === 'eval') { const r = await s('Runtime.evaluate', { expression: step.slice(5), awaitPromise: true, returnByValue: true }); log.push(`eval: ${JSON.stringify(r.result?.value)}`); }
}
writeFileSync(`${out}/console.txt`, log.join('\n'));
console.log(log.join('\n'));
chrome.kill();
process.exit(0);
