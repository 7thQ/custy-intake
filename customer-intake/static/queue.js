(() => {
  const PORTAL_NAMES = { admin: 'Admin', cst: 'CST', neets: 'Neets', lra: 'LRA' };
  const portalId = window.location.pathname.split('/')[1];
  const portalName = PORTAL_NAMES[portalId] ?? 'Queue';
  // Admin's Queue Control spans every department, so it's the only
  // view that needs a Department column to tell entries apart.
  const showDepartment = portalId === 'admin';

  document.title = `${portalName} Queue`;
  document.getElementById('portal-title').textContent = `${portalName} Queue`;
  document.getElementById('logout-form').action = `/${portalId}/logout`;

  const headerRow = document.getElementById('header-row');
  if (showDepartment) {
    const th = document.createElement('th');
    th.textContent = 'Department';
    headerRow.insertBefore(th, headerRow.lastElementChild);
  }

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
      nameTd.textContent = entry.display_name;
      tr.appendChild(nameTd);

      const descTd = document.createElement('td');
      descTd.textContent = entry.reason_for_visit;
      tr.appendChild(descTd);

      if (showDepartment) {
        const deptTd = document.createElement('td');
        deptTd.textContent = entry.department.toUpperCase();
        tr.appendChild(deptTd);
      }

      const actionTd = document.createElement('td');
      const removeBtn = document.createElement('button');
      removeBtn.type = 'button';
      removeBtn.className = 'secondary';
      removeBtn.textContent = 'Remove';
      removeBtn.addEventListener('click', () => removeEntry(entry.session_token));
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

  async function removeEntry(sessionToken) {
    const res = await fetch(`/${portalId}/api/queue/${sessionToken}/remove`, { method: 'POST' });
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
  setInterval(loadQueue, 5000);
})();
