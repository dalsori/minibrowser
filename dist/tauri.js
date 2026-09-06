export async function invoke(command, args) {
  const core = window.__TAURI__?.core;
  if (!core && new URLSearchParams(location.search).has('preview')) {
    if (command === 'getstate') return { adblock: true, engine: 'ddg' };
    if (command === 'memory_status') return { supported: true, used_mb: null, budget_mb: 1024, suspended: 0, discarded: 0, over_budget: false };
    return null;
  }
  if (!core) throw new Error('La aplicación no está conectada al backend de Tauri.');
  return core.invoke(command, args);
}
export function setStatus(element, message = '', isError = false) { element.textContent = message; element.classList.toggle('error', isError); }
export function platformShortcut(key) { const apple = /Mac|iPhone|iPad/.test(navigator.platform); return `${apple ? '⌘' : 'Ctrl'} ${key}`; }
