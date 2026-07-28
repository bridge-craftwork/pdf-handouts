// PDF Handouts — browser front end.
//
// The whole pipeline runs in WebAssembly in this page: files are read with the
// File API, handed to the Rust builder as bytes, and the finished PDF comes
// back as a blob. Nothing is uploaded.

import init, { HandoutBuilder, describe_input } from './pkg/pdf_handouts_wasm.js';

const el = (id) => document.getElementById(id);

const dropzone = el('dropzone');
const fileInput = el('file-input');
const fileListWrap = el('file-list-wrap');
const fileList = el('file-list');
const buildButton = el('build');
const statusEl = el('status');
const resultEl = el('result');
const errorEl = el('error');
const downloadEl = el('download');
const summaryEl = el('result-summary');
const notesEl = el('notes');

/** Files chosen so far: { name, bytes: Uint8Array, kind: string }. */
let files = [];
/** Object URL for the last build, revoked before creating the next. */
let lastUrl = null;
let wasmReady = false;

// ---------------------------------------------------------------- wasm setup

init()
  .then(() => {
    wasmReady = true;
    setStatus(files.length ? '' : 'Add some files to get started.');
    refresh();
  })
  .catch((err) => {
    setStatus('');
    showError(
      'Could not load the PDF engine.\n\n' +
      'This page needs WebAssembly, and must be served over http(s) rather ' +
      'than opened directly from disk.\n\n' + err
    );
  });

// -------------------------------------------------------------- file loading

/** Natural-order compare so "2." sorts before "10.". */
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });

function sortByName() {
  files.sort((a, b) => collator.compare(a.name, b.name));
  refresh();
}

async function addFiles(fileHandles) {
  // Snapshot first. Both `input.files` and `dataTransfer.files` are live
  // collections that the browser empties out from under us — the input when we
  // reset `.value`, the dataTransfer once its event handler returns — and this
  // function awaits, so iterating them directly loses all but the first file.
  const handles = Array.from(fileHandles);
  const incoming = [];

  for (const handle of handles) {
    const bytes = new Uint8Array(await handle.arrayBuffer());
    // describe_input returns "" for anything we cannot use, so an unusable
    // drop is flagged straight away rather than after a failed build.
    const kind = wasmReady ? describe_input(handle.name, bytes) : '';
    incoming.push({ name: handle.name, bytes, kind });
  }

  // Sort each batch on arrival so a "1. , 2. , 3." set lands in order; the
  // list stays hand-reorderable afterwards.
  incoming.sort((a, b) => collator.compare(a.name, b.name));
  files = files.concat(incoming);

  clearError();
  refresh();
}

// ------------------------------------------------------------------ file list

function refresh() {
  fileListWrap.hidden = files.length === 0;
  fileList.replaceChildren(...files.map(renderItem));

  const unusable = files.filter((f) => !f.kind);
  buildButton.disabled = !wasmReady || files.length === 0 || unusable.length > 0;

  if (unusable.length > 0) {
    const names = unusable.map((f) => f.name).join(', ');
    setStatus(`Remove the unsupported file${unusable.length > 1 ? 's' : ''}: ${names}`);
  } else if (files.length > 0) {
    setStatus(`${files.length} file${files.length > 1 ? 's' : ''} ready.`);
  } else if (wasmReady) {
    setStatus('Add some files to get started.');
  }
}

function renderItem(file, index) {
  const li = document.createElement('li');
  li.className = 'file-item';
  li.draggable = true;
  li.dataset.index = String(index);

  const grip = document.createElement('span');
  grip.className = 'grip';
  grip.textContent = '⠿';
  grip.setAttribute('aria-hidden', 'true');

  const name = document.createElement('span');
  name.className = 'file-name';
  name.textContent = file.name;

  const badge = document.createElement('span');
  badge.className = file.kind ? 'badge' : 'badge bad';
  badge.textContent = file.kind || 'unsupported';

  const remove = document.createElement('button');
  remove.type = 'button';
  remove.className = 'remove';
  remove.textContent = '×';
  remove.title = `Remove ${file.name}`;
  remove.setAttribute('aria-label', `Remove ${file.name}`);
  remove.addEventListener('click', () => {
    files.splice(index, 1);
    clearError();
    refresh();
  });

  li.append(grip, name, badge, remove);
  attachDragHandlers(li);
  return li;
}

// ------------------------------------------------------------ drag to reorder

let dragFrom = null;

function attachDragHandlers(li) {
  li.addEventListener('dragstart', (e) => {
    dragFrom = Number(li.dataset.index);
    li.classList.add('dragging');
    e.dataTransfer.effectAllowed = 'move';
    // Firefox will not start a drag without payload.
    e.dataTransfer.setData('text/plain', String(dragFrom));
  });

  li.addEventListener('dragend', () => {
    li.classList.remove('dragging');
    dragFrom = null;
  });

  li.addEventListener('dragover', (e) => {
    if (dragFrom === null) return; // a file drop, not a reorder
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    li.classList.add('drag-over');
  });

  li.addEventListener('dragleave', () => li.classList.remove('drag-over'));

  li.addEventListener('drop', (e) => {
    if (dragFrom === null) return;
    e.preventDefault();
    e.stopPropagation();
    li.classList.remove('drag-over');

    const to = Number(li.dataset.index);
    if (dragFrom === to) return;

    const [moved] = files.splice(dragFrom, 1);
    files.splice(to, 0, moved);
    refresh();
  });
}

// ---------------------------------------------------------------- drop target

['dragenter', 'dragover'].forEach((type) =>
  dropzone.addEventListener(type, (e) => {
    e.preventDefault();
    dropzone.classList.add('dragging');
  })
);

['dragleave', 'drop'].forEach((type) =>
  dropzone.addEventListener(type, () => dropzone.classList.remove('dragging'))
);

dropzone.addEventListener('drop', (e) => {
  e.preventDefault();
  if (e.dataTransfer.files.length > 0) addFiles(e.dataTransfer.files);
});

// Dropping anywhere else on the page should not navigate away from it.
window.addEventListener('dragover', (e) => e.preventDefault());
window.addEventListener('drop', (e) => e.preventDefault());

dropzone.addEventListener('click', () => fileInput.click());
dropzone.addEventListener('keydown', (e) => {
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault();
    fileInput.click();
  }
});

fileInput.addEventListener('change', () => {
  if (fileInput.files.length > 0) addFiles(fileInput.files);
  fileInput.value = ''; // let the same file be picked again later
});

el('sort-by-name').addEventListener('click', sortByName);

// --------------------------------------------------------------------- build

buildButton.addEventListener('click', async () => {
  clearError();
  resultEl.hidden = true;
  buildButton.disabled = true;
  setStatus('Building…');

  // Yield once so the browser paints the "Building…" state before the
  // synchronous wasm call blocks the main thread.
  await new Promise((resolve) => setTimeout(resolve, 0));

  try {
    const builder = new HandoutBuilder();
    for (const file of files) builder.add_file(file.name, file.bytes);

    const result = builder.build(JSON.stringify(collectOptions()));

    const blob = new Blob([result.pdf], { type: 'application/pdf' });
    if (lastUrl) URL.revokeObjectURL(lastUrl);
    lastUrl = URL.createObjectURL(blob);

    downloadEl.href = lastUrl;
    downloadEl.download = suggestedFilename();
    downloadEl.textContent = `Download ${downloadEl.download}`;

    const kb = Math.max(1, Math.round(result.pdf.length / 1024));
    summaryEl.textContent = `Built from ${files.length} file${files.length > 1 ? 's' : ''} — ${kb} KB.`;

    const notes = Array.from(result.notes);
    notesEl.replaceChildren(...notes.map((text) => {
      const li = document.createElement('li');
      li.textContent = text;
      return li;
    }));

    resultEl.hidden = false;
    setStatus('Done.');
  } catch (err) {
    setStatus('');
    showError(String(err && err.message ? err.message : err));
  } finally {
    buildButton.disabled = false;
  }
});

function collectOptions() {
  return {
    title: el('title').value,
    footerLeft: el('footer-left').value,
    footerCenter: el('footer-center').value,
    footerRight: el('footer-right').value,
    date: el('date').value,
    headerFont: el('header-font').value,
    footerFont: el('footer-font').value,
    fit: el('fit').value,
  };
}

/** Name the download after the title, so saved handouts are identifiable. */
function suggestedFilename() {
  const title = el('title').value.trim();
  if (!title) return 'handout.pdf';
  const safe = title.replace(/[^\w\s-]/g, '').replace(/\s+/g, '-').slice(0, 60);
  return `${safe || 'handout'}.pdf`;
}

// -------------------------------------------------------------------- helpers

function setStatus(text) {
  statusEl.textContent = text;
}

function showError(message) {
  errorEl.textContent = message;
  errorEl.hidden = false;
}

function clearError() {
  errorEl.hidden = true;
  errorEl.textContent = '';
}
