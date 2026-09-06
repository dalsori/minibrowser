// Manual integration check against a running debug build:
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223 cargo run ...
// node tests/webview-memory.cjs
// Uses local synthetic audio and temporarily selects a 512 MB budget; restores it.
const http = require('node:http');
const assert = require('node:assert/strict');
const { once } = require('node:events');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const debug = 'http://127.0.0.1:9223';

async function pages() { return (await (await fetch(`${debug}/json/list`)).json()).filter(p => p.type === 'page'); }
async function connect(page) {
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await once(socket, 'open');
  let id = 0;
  const pending = new Map();
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
  });
  return {
    async evaluate(expression) {
      const request = ++id;
      const reply = new Promise(resolve => pending.set(request, resolve));
      socket.send(JSON.stringify({ id: request, method: 'Runtime.evaluate', params: {
        expression, awaitPromise: true, returnByValue: true, userGesture: true
      } }));
      const message = await Promise.race([reply, sleep(10000).then(() => { throw new Error('CDP timeout'); })]);
      if (message.error || message.result?.exceptionDetails) throw new Error(JSON.stringify(message));
      return message.result.result.value;
    },
    close() { socket.close(); }
  };
}
async function waitPage(predicate) {
  for (let i = 0; i < 100; i++) {
    const found = (await pages().catch(() => [])).find(predicate);
    if (found) return found;
    await sleep(200);
  }
  throw new Error('Page did not appear');
}

const wav = Buffer.alloc(44 + 8000 * 2 * 600);
wav.write('RIFF'); wav.writeUInt32LE(wav.length - 8, 4); wav.write('WAVEfmt ', 8);
wav.writeUInt32LE(16, 16); wav.writeUInt16LE(1, 20); wav.writeUInt16LE(1, 22);
wav.writeUInt32LE(8000, 24); wav.writeUInt32LE(16000, 28);
wav.writeUInt16LE(2, 32); wav.writeUInt16LE(16, 34); wav.write('data', 36);
wav.writeUInt32LE(wav.length - 44, 40);
// Silent PCM is intentional: playback protection must work even when muted.
const server = http.createServer((req, res) => {
  if (req.url === '/audio.wav') {
    const range = req.headers.range?.match(/^bytes=(\d+)-(\d*)$/);
    const start = range ? Number(range[1]) : 0;
    const end = range?.[2] ? Math.min(Number(range[2]), wav.length - 1) : wav.length - 1;
    res.writeHead(range ? 206 : 200, { 'Content-Type': 'audio/wav', 'Content-Length': end - start + 1,
      'Accept-Ranges': 'bytes', ...(range ? { 'Content-Range': `bytes ${start}-${end}/${wav.length}` } : {}) });
    res.end(wav.subarray(start, end + 1)); return;
  }
  const index = req.url.split('/').at(-1);
  res.writeHead(200, { 'Content-Type': 'text/html' });
  res.end(`<!doctype html><title>Memory fixture ${index}</title>
    <body style="background:#15171d;color:white;font:20px system-ui"><h1>Memory fixture ${index}</h1>
    <audio controls loop muted src="/audio.wav"></audio><p>Local memory integration test</p></body>`);
});

(async () => {
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const origin = `http://127.0.0.1:${server.address().port}`;
  let current;
  let originalBudget;
  try {
    const initial = await waitPage(page => page.url.startsWith('http://tauri.localhost'));
    assert.equal((await pages()).length, 1, 'Use a fresh test instance with one home workspace');
    current = await connect(initial);
    originalBudget = (await current.evaluate("window.__TAURI__.core.invoke('memory_status')")).budget_mb;
    await current.evaluate("window.__TAURI__.core.invoke('setmemorybudget', {budgetMb:1024})");
    for (let i = 0; i < 5; i++) {
      await current.evaluate(`location.href=${JSON.stringify(`${origin}/page/${i}`)}`);
      current.close();
      const fixture = await waitPage(page => page.title === `Memory fixture ${i}`);
      current = await connect(fixture);
      await current.evaluate(`(async()=>{const media=document.querySelector('audio');
        if(media.readyState<1) await new Promise(resolve=>media.addEventListener('loadedmetadata',resolve,{once:true}));
        media.currentTime=12; ${i === 0 ? 'await media.play();' : 'media.pause();'} return media.paused;})()`);
      await current.evaluate("window.__TAURI__.core.invoke('wsnew')");
      current.close();
      const home = await waitPage(page => page.url.startsWith('http://tauri.localhost'));
      current = await connect(home);
    }
    assert.equal((await current.evaluate("window.__TAURI__.core.invoke('getworkspaces')")).total, 6);
    console.log('Created six workspaces; one has playing muted audio.');
    let status;
    for (let i = 0; i < 30; i++) {
      await sleep(5000);
      status = await current.evaluate("window.__TAURI__.core.invoke('memory_status')");
      assert.equal((await current.evaluate("window.__TAURI__.core.invoke('getworkspaces')")).active, 6, 'Workspace changed during automated test');
      console.log('Idle:', status);
      if (status.suspended + status.discarded >= 4) break;
    }
    assert.ok(status.suspended + status.discarded >= 4, 'inactive paused pages should sleep');
    // Keep pressure in the active workspace so sleeping pages must be discarded.
    await current.evaluate("window.__TAURI__.core.invoke('setmemorybudget', {budgetMb:512})");
    await current.evaluate('globalThis.memoryTestBuffers=Array.from({length:3},()=>new Uint8Array(128*1024*1024).fill(1)); true');
    for (let i = 0; i < 24; i++) {
      await sleep(5000);
      status = await current.evaluate("window.__TAURI__.core.invoke('memory_status')");
      assert.equal((await current.evaluate("window.__TAURI__.core.invoke('getworkspaces')")).active, 6, 'Workspace changed during automated test');
      console.log('Pressure:', status);
      if (status.discarded >= 4) break;
    }
    assert.equal(status.discarded, 4, 'only the four inactive paused pages should be discarded');
    await current.evaluate('globalThis.memoryTestBuffers=null');
    await current.evaluate("window.__TAURI__.core.invoke('ws',{dir:1})");
    current.close();
    current = await connect(await waitPage(page => page.title === 'Memory fixture 0'));
    assert.equal(await current.evaluate("document.querySelector('audio').paused"), false, 'playing workspace must survive');
    await current.evaluate("window.__TAURI__.core.invoke('ws',{dir:1})");
    current.close();
    current = await connect(await waitPage(page => page.title === 'Memory fixture 1'));
    await sleep(2000);
    const media = await current.evaluate("({paused:document.querySelector('audio').paused,time:document.querySelector('audio').currentTime})");
    assert.equal(media.paused, true);
    assert.ok(Math.abs(media.time - 12) < 1, `position should restore: ${JSON.stringify(media)}`);
    console.log('PASS: six workspaces, suspension, discard, playback protection and paused media restoration.');
  } finally {
    if (current && originalBudget) {
      try {
        await current.evaluate("location.href='http://tauri.localhost'"); current.close();
        current = await connect(await waitPage(page => page.url.startsWith('http://tauri.localhost')));
        await current.evaluate(`window.__TAURI__.core.invoke('setmemorybudget',{budgetMb:${originalBudget}})`);
      } catch (error) { console.error('Restore budget manually:', originalBudget, error.message); }
    }
    current?.close(); server.closeAllConnections(); server.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
