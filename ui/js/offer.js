// "Back from the game": a small window on top of all others that offers a
// journal entry right after a game started with Play has ended. No play time
// is counted or shown.

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);
const $ = (id) => document.getElementById(id);

function esc(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

let busy = false;

async function load() {
  const o = await invoke('journal_offer');
  if (!o) return;
  $('of-text').innerHTML = `You just finished playing <b>${esc(o.title)}</b>.\n\nWould you like to write down your impressions in the Gaming Journal while they are still fresh?`;
  $('of-more').textContent = o.waiting ? `+${o.waiting} more` : '';
  busy = false;
}

async function answer(write) {
  if (busy) return;
  busy = true;
  try {
    await invoke('answer_journal_offer', { write, never: $('of-never').checked });
  } catch (e) {
    busy = false;
    $('of-text').textContent = String(e);
  }
}

$('of-write').addEventListener('click', () => answer(true));
$('of-later').addEventListener('click', () => answer(false));
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') { e.preventDefault(); answer(false); }
});
document.addEventListener('contextmenu', (e) => e.preventDefault());
document.addEventListener('dragover', (e) => e.preventDefault());
document.addEventListener('drop', (e) => e.preventDefault());
tauri.event.listen('gc-offer-changed', () => load().catch(() => {}));
load().catch((e) => { $('of-text').textContent = String(e); });
