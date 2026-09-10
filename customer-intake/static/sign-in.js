(() => {
  const form = document.getElementById('sign-in-form');
  const submitBtn = document.getElementById('submit-btn');
  const errorEl = document.getElementById('error');
  const requiredIds = ['name', 'squadron', 'reason'];

  const now = new Date();
  document.getElementById('time-in').textContent = now.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  document.getElementById('sign-in-date').textContent = now.toLocaleDateString();

  function isValid() {
    return requiredIds.every((id) => document.getElementById(id).value.trim() !== '');
  }

  function updateSubmitState() {
    // Deliberately not `disabled`: the button stays clickable while
    // red so clicking it while incomplete can highlight the empty
    // fields instead of just doing nothing.
    submitBtn.classList.toggle('submit-ready', isValid());
  }

  requiredIds.forEach((id) => {
    document.getElementById(id).addEventListener('input', () => {
      document.getElementById(id).classList.remove('field-error');
      updateSubmitState();
    });
  });
  updateSubmitState();

  function highlightMissingFields() {
    requiredIds.forEach((id) => {
      const el = document.getElementById(id);
      el.classList.toggle('field-error', el.value.trim() === '');
    });
  }

  function showError(message) {
    errorEl.textContent = message;
    errorEl.hidden = false;
  }

  form.addEventListener('submit', async (event) => {
    event.preventDefault();
    if (!isValid()) {
      highlightMissingFields();
      return;
    }

    submitBtn.disabled = true;
    try {
      const res = await fetch('/api/sign-in', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: document.getElementById('name').value.trim(),
          rank: document.getElementById('rank').value.trim() || null,
          squadron: document.getElementById('squadron').value.trim(),
          reason_for_visit: document.getElementById('reason').value.trim(),
          ticket_number: document.getElementById('ticket').value.trim() || null,
        }),
      });
      if (!res.ok) {
        showError(`Failed to sign in: ${await res.text()}`);
        submitBtn.disabled = false;
        return;
      }
      window.location.href = '/services';
    } catch (err) {
      showError(`Failed to sign in: ${err}`);
      submitBtn.disabled = false;
    }
  });
})();
