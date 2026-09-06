// Runs in the top document. Unknown embedded content is protected conservatively.
(() => {
  if (window.top !== window) return;
  let edited = false;
  document.addEventListener('input', event => {
    if (event.isTrusted) edited = true;
  }, true);
  const inspect = (doc, result) => {
    if (doc.querySelector('[contenteditable="true"]:focus')) result.protected = true;
    for (const field of doc.querySelectorAll('input,textarea,select')) {
      if (field.tagName === 'SELECT' ? [...field.options].some(option => option.selected !== option.defaultSelected)
        : field.type === 'checkbox' || field.type === 'radio' ? field.checked !== field.defaultChecked
        : field.value !== field.defaultValue) result.protected = true;
    }
    for (const media of doc.querySelectorAll('video,audio')) {
      if (!media.paused && !media.ended) result.protected = true;
    }
    for (const frame of doc.querySelectorAll('iframe,frame')) {
      try {
        if (!frame.contentDocument) result.protected = true;
        else inspect(frame.contentDocument, result);
      } catch { result.protected = true; }
    }
  };
  window.__minibrowserMemoryCheck = token => {
    const result = { protected: edited, url: location.href, x: scrollX, y: scrollY,
      media: [...document.querySelectorAll('video,audio')].slice(0, 16).map(media => ({
        time: Number.isFinite(media.currentTime) ? media.currentTime : 0,
        volume: media.volume, muted: media.muted, rate: media.playbackRate
      })) };
    inspect(document, result);
    window.__TAURI__?.core.invoke('memory_report', { token, snapshot: result }).catch(() => {});
  };
  let restoreStarted = false;
  window.__minibrowserMemoryRestore = snapshot => {
    if (location.href !== snapshot.url) return;
    if (restoreStarted) return;
    restoreStarted = true;
    const restored = new WeakSet();
    const apply = () => {
      document.querySelectorAll('video,audio').forEach((media, index) => {
        const saved = snapshot.media[index];
        if (!saved || restored.has(media)) return;
        // A reloaded YouTube page may autoplay: keep previously paused media paused.
        media.pause();
        if (media.readyState < 1) return;
        try {
          media.currentTime = saved.time;
          media.volume = saved.volume;
          media.muted = saved.muted;
          media.playbackRate = saved.rate;
          restored.add(media);
        } catch { /* Retry once metadata/seekable ranges are available. */ }
      });
    };
    const onPlay = event => {
      const index = [...document.querySelectorAll('video,audio')].indexOf(event.target);
      if (snapshot.media[index]) event.target.pause();
    };
    document.addEventListener('play', onPlay, true);
    apply();
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', () => scrollTo(snapshot.x, snapshot.y), { once: true });
    } else scrollTo(snapshot.x, snapshot.y);
    const timer = setInterval(apply, 500);
    const stop = () => {
      clearInterval(timer);
      document.removeEventListener('play', onPlay, true);
      document.removeEventListener('pointerdown', stop, true);
      document.removeEventListener('keydown', stop, true);
    };
    document.addEventListener('pointerdown', stop, true);
    document.addEventListener('keydown', stop, true);
    setTimeout(stop, 30000);
  };
  window.__TAURI__?.core.invoke('memory_restore').then(snapshot => {
    if (snapshot) window.__minibrowserMemoryRestore(snapshot);
  }).catch(() => {});
})();
