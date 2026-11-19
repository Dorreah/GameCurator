// Additional files of a game (profile, under Archive): compatibility fixes,
// community patches, mods, bonus material. Copied into the archive or linked
// where they are.

import { api } from './api.js';
import { esc, formatBytes, toast, confirmBox, openModal, guard } from './ui.js';

export const ADDITIONS_DROP_HINT = 'Drop files here to add them to the game';

export const ADDITION_KINDS = [
  ['fix', 'Fix', 'Compatibility fix'],
  ['patch', 'Patch', 'Community or unofficial patch'],
  ['mod', 'Mod', 'Mod'],
  ['bonus', 'Bonus', 'Bonus material (documentary, art, music...)'],
  ['other', 'Other', 'Something else'],
];
const KIND_LABEL = Object.fromEntries(ADDITION_KINDS.map(([k, l]) => [k, l]));

export const ADDITIONS_HINT = 'Compatibility fixes, community patches, mods or bonus material such as a making-of documentary – '
  + 'keep everything that belongs to the game in one place, so you never have to search for it again.';

function row(a) {
  let where = a.kind === 'copy' ? 'copy in the archive' : (a.portable ? 'linked, in the archive' : 'linked');
  if (!a.exists) where = '<span class="warn-text">not found</span>';
  else if (!a.portable) where += ' <span class="faint" title="Outside the archive: on another computer it may not be available">· this PC</span>';
  const open = a.runnable
    ? `<button class="btn small" data-add-run="${a.id}" ${a.exists ? '' : 'disabled'} title="Start the program (asks first)">▶ Run</button>`
    : (a.risky ? '' : `<button class="btn small" data-add-open="${a.id}" ${a.exists ? '' : 'disabled'}>Open</button>`);
  return `<tr class="${a.exists ? '' : 'missing'}">
    <td class="kind"><span class="add-kind k-${esc(a.category)}">${esc(KIND_LABEL[a.category] || a.category)}</span></td>
    <td class="name"><div class="mono" title="${esc(a.location)}">${esc(a.filename)}</div>
      ${a.note ? `<div class="add-note">${esc(a.note)}</div>` : ''}<div class="add-where">${where}</div></td>
    <td class="num">${formatBytes(a.sizeBytes)}</td>
    <td class="act">${open}
      <button class="btn small" data-add-show="${a.id}" ${a.exists ? '' : 'disabled'} title="Show the file in its folder">Folder</button>
      <button class="btn small" data-add-edit="${a.id}" title="Kind and note">Edit</button>
      <button class="btn small" data-add-remove="${a.id}" title="Take off the list">✕</button></td></tr>`;
}

/** Content of the "Additional files" panel. */
export function additionsPanelHtml(list) {
  return `<div class="add-head"><button class="btn" id="pf-add-file">＋ Add file…</button>
      <span class="hint">${esc(ADDITIONS_HINT)} Files can also be dragged here.</span></div>
    ${list.length ? `<table class="list additions"><tbody>${list.map(row).join('')}</tbody></table>` : ''}`;
}

function kindOptions(selected) {
  return ADDITION_KINDS.map(([k, , label]) => `<option value="${k}" ${k === selected ? 'selected' : ''}>${esc(label)}</option>`).join('');
}

/** Choose a file in a dialog, then add it. */
async function addFlow(game) {
  const f = await api.pickAdditionFile();
  if (!f) return false;
  return addFiles(game, [f]);
}

/**
 * How to keep the chosen or dropped files (copy / link), their kind and a
 * note; then adds them one by one. Returns true when something was added.
 */
export async function addFiles(game, files) {
  const inside = files.every((f) => f.insideArchive);
  const many = files.length > 1;
  const total = files.reduce((n, f) => n + f.size, 0);
  const m = openModal({
    title: many ? `Add ${files.length} files to the game` : 'Add a file to the game',
    width: 560,
    body: `<div class="notice err" id="ad-error" hidden></div>
      <div class="add-file">${files.slice(0, 8).map((f) => `<div><b class="mono">${esc(f.name)}</b> <span class="muted">· ${formatBytes(f.size)}</span></div>`).join('')}
        ${files.length > 8 ? `<div class="muted">… and ${files.length - 8} more</div>` : ''}
        ${many ? `<div class="hint">${formatBytes(total)} in total</div>` : `<div class="hint mono">${esc(files[0].location)}</div>`}</div>
      <fieldset class="group"><legend>How to keep ${many ? 'them' : 'it'}</legend>
        <label class="radio-line"><input type="radio" name="ad-mode" value="copy" ${inside ? '' : 'checked'}>
          <span><b>Copy into the archive</b><br><span class="hint">A copy is stored in Data/Additions next to your games and travels with the drive. Your original stays where it is.</span></span></label>
        <label class="radio-line"><input type="radio" name="ad-mode" value="link" ${inside ? 'checked' : ''}>
          <span><b>Link to the file where it is</b><br><span class="hint">${inside
            ? 'Already in the archive, so the link follows the drive. Nothing is copied.'
            : 'Nothing is copied – good for big files you keep elsewhere. On another computer the file may not be available.'}</span></span></label>
      </fieldset>
      <div class="form-grid" style="grid-template-columns:70px 1fr;margin-top:10px">
        <label for="ad-kind">Kind</label><select class="field" id="ad-kind">${kindOptions('fix')}</select>
        <label for="ad-note">Note</label><textarea class="field" id="ad-note" rows="3" maxlength="2000"
          placeholder="What it is and why it matters, e.g. Widescreen fix 1.3 – needed for Windows 11"></textarea>
      </div>${many ? '<p class="hint" style="margin:8px 0 0">The kind and the note apply to all the files; each can be edited later.</p>' : ''}`,
    footer: '<span class="left hint" id="ad-busy" hidden>Copying… big files can take a while.</span><button class="btn" data-close>Cancel</button><button class="btn gold" id="ad-save">Add</button>',
  });
  const q = (s) => m.el.querySelector(s);
  let pending = files;
  const done = [];
  q('#ad-save').addEventListener('click', async () => {
    const mode = q('input[name="ad-mode"]:checked').value;
    const input = { category: q('#ad-kind').value, note: q('#ad-note').value };
    q('#ad-save').disabled = true;
    q('#ad-busy').hidden = mode !== 'copy';
    const failed = [];
    for (const f of pending) {
      try {
        done.push(await api.addAddition(game.id, f.token, mode, input));
      } catch (e) {
        failed.push({ f, e });
      }
    }
    if (!failed.length) { m.close(true); return; }
    q('#ad-error').textContent = failed.map(({ f, e }) => `${f.name}: ${e}`).join('\n');
    q('#ad-error').hidden = false;
    q('#ad-busy').hidden = true;
    // a file's token is used up by the attempt: to try again, choose or drop the file again
    pending = [];
    q('#ad-save').hidden = true;
    const cancel = q('[data-close]:not(.x)');
    if (cancel) cancel.textContent = 'Close';
  });
  await m.result;
  if (done.length === 1) toast(done[0].kind === 'copy' ? `${done[0].filename} copied into the archive.` : `${done[0].filename} linked.`);
  else if (done.length > 1) toast(`${done.length} files added.`);
  return done.length > 0;
}

async function editFlow(a) {
  const m = openModal({
    title: `Edit – ${a.filename}`,
    width: 520,
    body: `<div class="form-grid" style="grid-template-columns:70px 1fr">
        <label for="ae-kind">Kind</label><select class="field" id="ae-kind">${kindOptions(a.category)}</select>
        <label for="ae-note">Note</label><textarea class="field" id="ae-note" rows="4" maxlength="2000">${esc(a.note)}</textarea></div>`,
    footer: '<button class="btn" data-close>Cancel</button><button class="btn gold" id="ae-save">Save</button>',
  });
  m.el.querySelector('#ae-save').addEventListener('click', () => m.close({
    category: m.el.querySelector('#ae-kind').value, note: m.el.querySelector('#ae-note').value,
  }));
  const input = await m.result;
  if (!input) return false;
  await api.updateAddition(a.id, input);
  return true;
}

async function removeFlow(a) {
  if (a.kind === 'link') {
    const ok = await confirmBox({
      title: 'Take off the list?',
      html: `Take <b>${esc(a.filename)}</b> off the list of additional files?\n\nThe file itself is not touched.`,
      ok: 'Take off',
    });
    if (!ok) return false;
    await api.removeAddition(a.id, false);
    return true;
  }
  const m = openModal({
    title: 'Take off the list?',
    width: 500,
    body: `<div class="dlg-icon warn">!</div><div class="dlg-text">Take <b>${esc(a.filename)}</b> off the list of additional files?
        <label class="radio-line" style="margin-top:10px"><input type="checkbox" id="ar-delete" checked>
        <span>Also delete the copy stored in the archive (${formatBytes(a.sizeBytes)}).<br><span class="hint">Your original file is not affected.</span></span></label></div>`,
    footer: '<button class="btn" data-close>Cancel</button><button class="btn gold" id="ar-ok">Take off</button>',
  });
  m.el.querySelector('#ar-ok').addEventListener('click', () => m.close({ del: m.el.querySelector('#ar-delete').checked }));
  const r = await m.result;
  if (!r) return false;
  await api.removeAddition(a.id, r.del);
  toast(r.del ? 'Taken off the list; the copy was deleted.' : 'Taken off the list; the copy stays in Data/Additions.');
  return true;
}

async function runFlow(a) {
  const ok = await confirmBox({
    title: 'Run program?',
    html: `You are about to run:\n<span class="mono">${esc(a.location)}</span>\n\nOnly run programs you trust.`,
    ok: 'Run',
  });
  if (!ok) return;
  await api.runAddition(a.id);
  toast(`Started ${a.filename}.`);
}

/** Clicks inside the panel. `reload` re-reads the list and re-renders it. */
export function bindAdditions(container, game, getList, reload) {
  container.addEventListener('click', (e) => {
    const t = e.target;
    const find = (attr) => getList().find((x) => x.id === Number(t.closest(`[${attr}]`).getAttribute(attr)));
    if (t.closest('#pf-add-file')) return guard(async () => { if (await addFlow(game)) await reload(); });
    if (t.closest('[data-add-open]')) return guard(() => api.openAddition(find('data-add-open').id));
    if (t.closest('[data-add-run]')) return guard(() => runFlow(find('data-add-run')));
    if (t.closest('[data-add-show]')) return guard(() => api.showAddition(find('data-add-show').id));
    if (t.closest('[data-add-edit]')) return guard(async () => { if (await editFlow(find('data-add-edit'))) await reload(); });
    if (t.closest('[data-add-remove]')) return guard(async () => { if (await removeFlow(find('data-add-remove'))) await reload(); });
    return undefined;
  });
}
