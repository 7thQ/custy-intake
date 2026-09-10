(() => {
  // Same address this browser itself used to load the page — always
  // correct for whatever machine/IP the lobby display is running on,
  // no server round-trip needed.
  document.getElementById('staff-url').textContent = `${window.location.origin}/staff`;

  const rowsEl = document.getElementById('queue-rows');
  const emptyEl = document.getElementById('empty-message');

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

      const deptTd = document.createElement('td');
      deptTd.textContent = entry.department.toUpperCase();
      tr.appendChild(deptTd);

      rowsEl.appendChild(tr);
    }
  }

  async function loadQueue() {
    try {
      const res = await fetch('/api/queue');
      if (res.ok) renderRows(await res.json());
    } catch {
      // Passive display — a transient fetch failure just means the
      // board doesn't refresh this cycle; it'll try again shortly.
    }
  }

  loadQueue();
  setInterval(loadQueue, 5000);
})();
