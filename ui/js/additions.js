// Additional files of a game (profile, under Archive): compatibility fixes,
// community patches, mods, bonus material. Copied into the archive or linked
// where they are.

import { api } from './api.js';
import { esc, formatBytes, toast, confirmBox, openModal, guard } from './ui.js';
import { openHint } from './store.js';
import { t, tn, mark } from './i18n.js';

/** English keys, translated where shown. */
export const ADDITIONS_DROP_HINT = mark('Drop files here to add them to the game');

export const ADDITION_KINDS = [
  ['fix', mark('Fix'), mark('Compatibility fix')],
  ['patch', mark('Patch'), mark('Community or unofficial patch')],
  ['mod', mark('Mod'), mark('Mod')],
  ['bonus', mark('Bonus'), mark('Bonus material (documentary, art, music...)')],
  ['other', mark('Other'), mark('Something else')],
];
const KIND_LABEL = Object.fromEntries(ADDITION_KINDS.map(([k, l]) => [k, l]));
const kindLabel = (k) => (KIND_LABEL[k] ? t(KIND_LABEL[k]) : k);

export const ADDITIONS_HINT = mark('Compatibility fixes, community patches, mods or bonus material such as a making-of documentary – keep everything that belongs to the game in one place, so you never have to search for it again.');

function row(a) {
  let where = a.kind === 'copy' ? t('copy in the archive') : (a.portable ? t('linked, in the archive') : t('linked'));
  if (!a.exists) where = `<span class="warn-text">${t('not found')}</span>`;
  else if (!a.portable) where += ` <span class="faint" title="${t('Outside the archive: on another computer it may not be available')}">· ${t('this PC')}</span>`;
  const open = a.runnable
    ? `<button class="btn small" data-add-run="${a.id}" ${a.exists ? '' : 'disabled'} title="${t('Start the program (asks first)')}">▶ ${t('Run')}</button>`
    : (a.risky ? '' : `<button class="btn small" data-add-open="${a.id}" ${a.exists ? '' : 'disabled'} title="${esc(openHint(a.filename))}">${t('Open')}</button>`);
  return `<tr class="${a.exists ? '' : 'missing'}">
    <td class="kind"><span class="add-kind k-${esc(a.category)}">${esc(kindLabel(a.category))}</span></td>
    <td class="name">${a.displayName
      ? `<div class="fname renamed" title="${esc(`${a.displayName}\n${t('File: {path}', { path: a.location })}`)}">${esc(a.displayName)}</div>`
      : `<div class="mono fname" title="${esc(a.location)}">${esc(a.filename)}</div>`}
      ${a.note ? `<div class="add-note">${esc(a.note)}</div>` : ''}<div class="add-where">${where}</div></td>
    <td class="num">${formatBytes(a.sizeBytes)}</td>
    <td class="act">${open}
      <button class="btn small icon" data-add-edit="${a.id}" title="${t('Name in the catalog, kind and note – the file keeps its name')}">✎</button>
      <button class="btn small" data-add-show="${a.id}" ${a.exists ? '' : 'disabled'} title="${t('Show the file in its folder')}">${t('Folder')}</button>
      <button class="btn small" data-add-remove="${a.id}" title="${t('Take off the list')}">✕</button></td></tr>`;
}

/** Content of the "Additional files" panel. */
export function additionsPanelHtml(list) {
  return `<div class="add-head"><button class="btn" id="pf-add-file">＋ ${t('Add file…')}</button>
      <span class="hint">${esc(t(ADDITIONS_HINT))} ${t('Files can also be dragged here.')}</span></div>
    ${list.length ? `<table class="list additions"><tbody>${list.map(row).join('')}</tbody></table>` : ''}`;
}

function kindOptions(selected) {
  return ADDITION_KINDS.map(([k, , label]) => `<option value="${k}" ${k === selected ? 'selected' : ''}>${esc(t(label))}</option>`).join('');
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
    title: many ? t('Add {n} files to the game', { n: files.length }) : t('Add a file to the game'),
    width: 560,
    body: `<div class="notice err" id="ad-error" hidden></div>
      <div class="add-file">${files.slice(0, 8).map((f) => `<div><b class="mono">${esc(f.name)}</b> <span class="muted">· ${formatBytes(f.size)}</span></div>`).join('')}
        ${files.length > 8 ? `<div class="muted">${t('… and {n} more', { n: files.length - 8 })}</div>` : ''}
        ${many ? `<div class="hint">${t('{size} in total', { size: formatBytes(total) })}</div>` : `<div class="hint mono">${esc(files[0].location)}</div>`}</div>
      <fieldset class="group"><legend>${many ? t('How to keep them') : t('How to keep it')}</legend>
        <label class="radio-line"><input type="radio" name="ad-mode" value="copy" ${inside ? '' : 'checked'}>
          <span><b>${t('Copy into the archive')}</b><br><span class="hint">${t('A copy is stored in Data/Additions next to your games and travels with the drive. Your original stays where it is.')}</span></span></label>
        <label class="radio-line"><input type="radio" name="ad-mode" value="link" ${inside ? 'checked' : ''}>
          <span><b>${t('Link to the file where it is')}</b><br><span class="hint">${inside
            ? t('Already in the archive, so the link follows the drive. Nothing is copied.')
            : t('Nothing is copied – good for big files you keep elsewhere. On another computer the file may not be available.')}</span></span></label>
      </fieldset>
      <div class="form-grid" style="grid-template-columns:70px 1fr;margin-top:10px">
        <label for="ad-kind">${t('Kind')}</label><select class="field" id="ad-kind">${kindOptions('fix')}</select>
        <label for="ad-note">${t('Note')}</label><textarea class="field" id="ad-note" rows="3" maxlength="2000"
          placeholder="${esc(t('What it is and why it matters, e.g. Widescreen fix 1.3 – needed for Windows 11'))}"></textarea>
      </div>${many ? `<p class="hint" style="margin:8px 0 0">${t('The kind and the note apply to all the files; each can be edited later.')}</p>` : ''}`,
    footer: `<span class="left hint" id="ad-busy" hidden>${t('Copying… big files can take a while.')}</span><button class="btn" data-close>${t('Cancel')}</button><button class="btn gold" id="ad-save">${t('Add')}</button>`,
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
    if (cancel) cancel.textContent = t('Close');
  });
  await m.result;
  if (done.length === 1) toast(done[0].kind === 'copy' ? t('{name} copied into the archive.', { name: done[0].filename }) : t('{name} linked.', { name: done[0].filename }));
  else if (done.length > 1) toast(tn(done.length, '{n} file added.', '{n} files added.'));
  return done.length > 0;
}

/** Name in the catalog (the file keeps its own name), kind and note. */
async function editFlow(a) {
  const m = openModal({
    title: t('Edit – {name}', { name: a.displayName || a.filename }),
    width: 540,
    body: `<div class="notice err" id="ae-error" hidden></div>
      <div class="form-grid" style="grid-template-columns:70px 1fr">
        <label for="ae-name">${t('Name')}</label><input class="field" id="ae-name" maxlength="120" spellcheck="false" value="${esc(a.displayName || a.filename)}">
        <span></span><span class="hint" style="margin-top:-4px">${t('Only the name in the catalog – the file keeps its name: <span class="mono">{name}</span>', { name: esc(a.filename) })}</span>
        <label for="ae-kind">${t('Kind')}</label><select class="field" id="ae-kind">${kindOptions(a.category)}</select>
        <label for="ae-note">${t('Note')}</label><textarea class="field" id="ae-note" rows="4" maxlength="2000">${esc(a.note)}</textarea></div>`,
    footer: `<button class="btn" data-close>${t('Cancel')}</button><button class="btn gold" id="ae-save">${t('Save')}</button>`,
  });
  const q = (sel) => m.el.querySelector(sel);
  q('#ae-save').addEventListener('click', async () => {
    try {
      await api.updateAddition(a.id, { category: q('#ae-kind').value, note: q('#ae-note').value, displayName: q('#ae-name').value });
      m.close(true);
    } catch (e) {
      q('#ae-error').textContent = String(e);
      q('#ae-error').hidden = false;
    }
  });
  return Boolean(await m.result);
}

async function removeFlow(a) {
  if (a.kind === 'link') {
    const ok = await confirmBox({
      title: t('Take off the list?'),
      html: t('Take <b>{name}</b> off the list of additional files?\n\nThe file itself is not touched.', { name: esc(a.displayName || a.filename) }),
      ok: t('Take off'),
    });
    if (!ok) return false;
    await api.removeAddition(a.id, false);
    return true;
  }
  const m = openModal({
    title: t('Take off the list?'),
    width: 500,
    body: `<div class="dlg-icon warn">!</div><div class="dlg-text">${t('Take <b>{name}</b> off the list of additional files?', { name: esc(a.displayName || a.filename) })}
        <label class="radio-line" style="margin-top:10px"><input type="checkbox" id="ar-delete" checked>
        <span>${t('Also delete the copy stored in the archive ({size}).', { size: formatBytes(a.sizeBytes) })}<br><span class="hint">${t('Your original file is not affected.')}</span></span></label></div>`,
    footer: `<button class="btn" data-close>${t('Cancel')}</button><button class="btn gold" id="ar-ok">${t('Take off')}</button>`,
  });
  m.el.querySelector('#ar-ok').addEventListener('click', () => m.close({ del: m.el.querySelector('#ar-delete').checked }));
  const r = await m.result;
  if (!r) return false;
  await api.removeAddition(a.id, r.del);
  toast(r.del ? t('Taken off the list; the copy was deleted.') : t('Taken off the list; the copy stays in Data/Additions.'));
  return true;
}

async function runFlow(a) {
  const ok = await confirmBox({
    title: t('Run program?'),
    html: t('You are about to run:\n<span class="mono">{path}</span>\n\nOnly run programs you trust.', { path: esc(a.location) }),
    ok: t('Run'),
  });
  if (!ok) return;
  await api.runAddition(a.id);
  toast(t('Started {name}.', { name: a.filename }));
}

/** Clicks inside the panel. `reload` re-reads the list and re-renders it. */
export function bindAdditions(container, game, getList, reload) {
  container.addEventListener('click', (e) => {
    const tg = e.target;
    const find = (attr) => getList().find((x) => x.id === Number(tg.closest(`[${attr}]`).getAttribute(attr)));
    if (tg.closest('#pf-add-file')) return guard(async () => { if (await addFlow(game)) await reload(); });
    if (tg.closest('[data-add-open]')) return guard(() => api.openAddition(find('data-add-open').id).then((how) => { if (how === 'external') toast(t('Opened with the default application.')); }));
    if (tg.closest('[data-add-run]')) return guard(() => runFlow(find('data-add-run')));
    if (tg.closest('[data-add-show]')) return guard(() => api.showAddition(find('data-add-show').id));
    if (tg.closest('[data-add-edit]')) return guard(async () => { if (await editFlow(find('data-add-edit'))) await reload(); });
    if (tg.closest('[data-add-remove]')) return guard(async () => { if (await removeFlow(find('data-add-remove'))) await reload(); });
    return undefined;
  });
}
