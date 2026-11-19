// Minimal rich-text editor for journal entries (paragraphs, headings, lists,
// bold/italic/underline, quotes) and the HTML sanitizer used for every
// journal text that is saved or displayed.

const ALLOWED = new Set(['P', 'H2', 'H3', 'UL', 'OL', 'LI', 'B', 'STRONG', 'I', 'EM', 'U', 'BR', 'BLOCKQUOTE']);
const DROP = new Set(['SCRIPT', 'STYLE', 'IFRAME', 'OBJECT', 'EMBED', 'LINK', 'META', 'TEMPLATE', 'SVG', 'MATH', 'NOSCRIPT', 'TITLE', 'HEAD']);
const RENAME = { DIV: 'P', H1: 'H2', H4: 'H3', H5: 'H3', H6: 'H3' };

function clean(node, doc) {
  for (const child of [...node.childNodes]) {
    if (child.nodeType === Node.TEXT_NODE) continue;
    if (child.nodeType !== Node.ELEMENT_NODE) { child.remove(); continue; }
    const tag = child.tagName.toUpperCase();
    if (DROP.has(tag)) { child.remove(); continue; }
    let el = child;
    if (RENAME[tag]) {
      el = doc.createElement(RENAME[tag]);
      while (child.firstChild) el.appendChild(child.firstChild);
      child.replaceWith(el);
    } else if (!ALLOWED.has(tag)) {
      clean(child, doc);
      child.replaceWith(...child.childNodes);
      continue;
    }
    for (const a of [...el.attributes]) el.removeAttribute(a.name);
    clean(el, doc);
  }
}

/** Returns safe HTML containing only whitelisted tags without attributes. */
export function sanitizeHtml(html) {
  const doc = new DOMParser().parseFromString(`<!DOCTYPE html><body>${html || ''}</body>`, 'text/html');
  clean(doc.body, doc);
  return doc.body.innerHTML.replace(/<p><\/p>/g, '').trim();
}

export function plainText(html) {
  // keep words of adjacent blocks apart: "<h2>A</h2><p>B</p>" -> "A B"
  const spaced = sanitizeHtml(html).replace(/<\/(p|h2|h3|li|blockquote)>|<br>/gi, ' $&');
  const doc = new DOMParser().parseFromString(`<body>${spaced}</body>`, 'text/html');
  return (doc.body.textContent || '').replace(/\s+/g, ' ').trim();
}

const TOOLS = [
  { cmd: 'formatBlock', arg: 'p', label: '¶', title: 'Paragraph' },
  { cmd: 'formatBlock', arg: 'h2', label: 'H1', title: 'Heading' },
  { cmd: 'formatBlock', arg: 'h3', label: 'H2', title: 'Subheading' },
  { sep: true },
  { cmd: 'bold', label: '<b>B</b>', title: 'Bold (Ctrl+B)' },
  { cmd: 'italic', label: '<i>I</i>', title: 'Italic (Ctrl+I)' },
  { cmd: 'underline', label: '<u>U</u>', title: 'Underline (Ctrl+U)' },
  { sep: true },
  { cmd: 'insertUnorderedList', label: '• List', title: 'Bulleted list' },
  { cmd: 'insertOrderedList', label: '1. List', title: 'Numbered list' },
  { cmd: 'formatBlock', arg: 'blockquote', label: '❝ Quote', title: 'Quote' },
];

/** Builds the editor into `container`. Returns { getHtml, setHtml, focus }. */
export function createEditor(container, initialHtml = '') {
  container.innerHTML = `
    <div class="editor-toolbar">${TOOLS.map((t, i) => (t.sep ? '<span class="sep" style="width:6px"></span>'
      : `<button type="button" class="btn small" data-tool="${i}" title="${t.title}">${t.label}</button>`)).join('')}</div>
    <div class="editor rich" contenteditable="true" spellcheck="true" data-placeholder="What did you notice, feel or find out?"></div>`;
  const ed = container.querySelector('.editor');
  ed.innerHTML = sanitizeHtml(initialHtml);
  try { document.execCommand('defaultParagraphSeparator', false, 'p'); } catch (_) { /* older engines */ }

  container.querySelector('.editor-toolbar').addEventListener('mousedown', (e) => {
    const b = e.target.closest('[data-tool]');
    if (!b) return;
    e.preventDefault(); // keep the selection inside the editor
    const t = TOOLS[Number(b.dataset.tool)];
    ed.focus();
    document.execCommand(t.cmd, false, t.arg ? `<${t.arg}>` : null);
  });

  ed.addEventListener('paste', (e) => {
    e.preventDefault();
    const html = e.clipboardData.getData('text/html');
    if (html) {
      document.execCommand('insertHTML', false, sanitizeHtml(html));
    } else {
      const text = e.clipboardData.getData('text/plain');
      const paras = text.split(/\r?\n\r?\n/).map((p) => `<p>${p.replace(/[&<>]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])).replace(/\r?\n/g, '<br>')}</p>`);
      document.execCommand('insertHTML', false, paras.join(''));
    }
  });
  ed.addEventListener('drop', (e) => e.preventDefault());

  return {
    getHtml: () => sanitizeHtml(ed.innerHTML),
    setHtml: (h) => { ed.innerHTML = sanitizeHtml(h); },
    focus: () => ed.focus(),
    element: ed,
  };
}
