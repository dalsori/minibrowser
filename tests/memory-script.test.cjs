const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const script = fs.readFileSync('src-tauri/src/memory.js', 'utf8');

function page({ media = [], frames = [] } = {}) {
  const listeners = new Map();
  const calls = [];
  const document = {
    querySelector: () => null,
    querySelectorAll: selector => selector === 'video,audio' ? media : selector === 'iframe,frame' ? frames : [],
    addEventListener: (name, callback) => listeners.set(name, callback),
    removeEventListener: (name, callback) => { if (listeners.get(name) === callback) listeners.delete(name); }
  };
  const window = { __TAURI__: { core: { invoke: (command, payload) => { calls.push({ command, payload }); return Promise.resolve(); } } } };
  window.top = window;
  const context = { window, document, location: { href: 'https://www.youtube.com/watch?v=one' },
    scrollX: 0, scrollY: 200, scrollTo: () => {}, setInterval: () => 1, clearInterval: () => {}, setTimeout: () => 1 };
  vm.runInNewContext(script, context);
  return { window, listeners, calls, check() { window.__minibrowserMemoryCheck(7); return calls.at(-1).payload.snapshot; } };
}

test('protects playing media, including muted video', () => {
  assert.equal(page({ media: [{ paused: false, ended: false, muted: true, currentTime: 4 }] }).check().protected, true);
});
test('paused video saves position and can be suspended', () => {
  const snapshot = page({ media: [{ paused: true, currentTime: 85, volume: 0.5, muted: false, playbackRate: 1.5 }] }).check();
  assert.equal(snapshot.protected, false);
  assert.equal(snapshot.media[0].time, 85);
});
test('protects user edits but ignores synthetic input', () => {
  const p = page();
  p.listeners.get('input')({ isTrusted: false });
  assert.equal(p.check().protected, false);
  p.listeners.get('input')({ isTrusted: true });
  assert.equal(p.check().protected, true);
});
test('protects cross-origin frames whose media state is unknown', () => {
  assert.equal(page({ frames: [{ contentDocument: null }] }).check().protected, true);
});
test('detects playback in same-origin frames', () => {
  const frameDocument = { querySelector: () => null, querySelectorAll: selector => selector === 'video,audio' ? [{ paused: false, ended: false }] : [] };
  assert.equal(page({ frames: [{ contentDocument: frameDocument }] }).check().protected, true);
});
test('restores paused media position, volume and speed', () => {
  const media = { readyState: 1, paused: false, pause() { this.paused = true; } };
  const p = page({ media: [media] });
  p.window.__minibrowserMemoryRestore({ url: 'https://www.youtube.com/watch?v=one', x: 0, y: 0,
    media: [{ time: 85, volume: 0.5, muted: true, rate: 1.5 }] });
  assert.equal(media.paused, true);
  assert.equal(media.currentTime, 85);
  assert.equal(media.playbackRate, 1.5);
  assert.equal(media.volume, 0.5);
  assert.equal(media.muted, true);
});
test('never applies a snapshot to a different URL', () => {
  const media = { pause() { throw new Error('must not touch other page'); } };
  const p = page({ media: [media] });
  p.window.__minibrowserMemoryRestore({ url: 'https://example.com', media: [{}] });
});
