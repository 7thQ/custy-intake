(() => {
  const PORTAL_NAMES = { admin: 'Admin', cst: 'CST', neets: 'Neets', lra: 'LRA' };
  const portalId = window.location.pathname.split('/')[1];
  const portalName = PORTAL_NAMES[portalId] ?? 'Queue';

  document.title = `${portalName} Queue`;
  document.getElementById('portal-title').textContent = `${portalName} Queue`;
  document.getElementById('logout-form').action = `/${portalId}/logout`;

  const rowsEl = document.getElementById('queue-rows');
  const emptyEl = document.getElementById('empty-message');
  const errorEl = document.getElementById('error');

  function showError(message) {
    errorEl.textContent = message;
    errorEl.hidden = false;
  }

  function renderRows(entries) {
    rowsEl.innerHTML = '';
    emptyEl.hidden = entries.length > 0;

    for (const entry of entries) {
      const tr = document.createElement('tr');

      const nameTd = document.createElement('td');
      nameTd.textContent = entry.name;
      tr.appendChild(nameTd);

      const descTd = document.createElement('td');
      descTd.textContent = entry.description;
      tr.appendChild(descTd);

      const actionTd = document.createElement('td');
      const removeBtn = document.createElement('button');
      removeBtn.type = 'button';
      removeBtn.className = 'secondary';
      removeBtn.textContent = 'Remove';
      removeBtn.addEventListener('click', () => removeEntry(entry.id));
      actionTd.appendChild(removeBtn);
      tr.appendChild(actionTd);

      rowsEl.appendChild(tr);
    }
  }

  async function loadQueue() {
    const res = await fetch(`/${portalId}/api/queue`);
    if (res.status === 401) {
      window.location.href = `/${portalId}/login`;
      return;
    }
    if (!res.ok) {
      showError(`Failed to load queue: ${await res.text()}`);
      return;
    }
    renderRows(await res.json());
  }

  async function removeEntry(id) {
    const res = await fetch(`/${portalId}/api/queue/${id}/remove`, { method: 'POST' });
    if (res.status === 401) {
      window.location.href = `/${portalId}/login`;
      return;
    }
    if (!res.ok) {
      showError(`Failed to remove entry: ${await res.text()}`);
      return;
    }
    loadQueue();
  }

  loadQueue();
})();
