import { invoke, setStatus } from './tauri.js';

const footer = document.querySelector('.footer');
const section = document.createElement('div');
section.className = 'setting';
section.style.display = 'block';
section.innerHTML = `<div class="setting-copy">
  <strong><label for="memory-budget">Presupuesto de memoria</label></strong>
  <span>Los espacios inactivos pueden recargarse al volver. La reproducción y los formularios editados se conservan.</span>
  </div>
  <select id="memory-budget" style="margin-top:14px;padding:10px;border-radius:8px;background:#1b1e25;color:inherit;border:1px solid var(--border)">
    <option value="512">512 MB</option><option value="768">768 MB</option>
    <option value="1024">1 GB (recomendado)</option><option value="1536">1.5 GB</option>
    <option value="2048">2 GB</option><option value="4096">4 GB</option><option value="8192">8 GB</option>
  </select>
  <p id="memory-usage" class="lede" style="font-size:12px;margin-top:10px" role="status"></p>`;
footer.before(section);
const select = section.querySelector('select');
const usage = section.querySelector('#memory-usage');
const status = document.querySelector('#status');
let previous = '1024';
let initialized = false;
let saving = false;
select.disabled = true;

async function refresh() {
  try {
    const memory = await invoke('memory_status');
    if (!memory.supported) {
      usage.textContent = 'El ahorro automático de memoria está disponible en Windows.';
      return;
    }
    if (!initialized) {
      previous = String(memory.budget_mb);
      if (![...select.options].some(option => option.value === previous)) {
        select.add(new Option(`${previous} MB`, previous));
      }
      select.value = previous;
      select.disabled = false;
      initialized = true;
    }
    const measured = memory.used_mb === null ? 'Midiendo memoria…' : `Uso estimado: ${memory.used_mb} MB`;
    usage.textContent = `${measured} · ${memory.suspended} en reposo · ${memory.discarded} descargados. ` +
      (memory.over_budget ? 'Por encima del objetivo: se prioriza conservar el contenido activo.' : 'Es un objetivo; puede haber picos de consumo.');
  } catch (error) {
    usage.textContent = `No se pudo consultar la memoria: ${String(error)}`;
  }
}
select.addEventListener('change', async () => {
  if (saving) return;
  saving = true;
  select.disabled = true;
  try {
    await invoke('setmemorybudget', { budgetMb: Number(select.value) });
    previous = select.value;
    setStatus(status, 'Guardado');
  } catch (error) {
    select.value = previous;
    setStatus(status, String(error), true);
  } finally {
    saving = false;
    select.disabled = false;
    await refresh();
  }
});
await refresh();
setInterval(refresh, 5000);
