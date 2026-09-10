(() => {
  // Fetched from the server rather than built from
  // window.location.origin — the server applies the same
  // localhost/0.0.0.0 substitution it uses for the QR code, so this
  // can't end up showing a dead address either if the lobby display
  // was opened via one of those instead of its real LAN IP.
  fetch('/api/staff-url')
    .then((res) => res.json())
    .then(({ url }) => { document.getElementById('staff-url').textContent = url; })
    .catch(() => {});

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
