(() => {
  const answers = {};
  let issuers = [];
  let crossedOut = '';
  let applyOptions = [];
  let itemCount = 1;

  const statusEl = document.getElementById('status');
  const schemaErrorEl = document.getElementById('schema-error');
  const formCard = document.getElementById('form-card');
  const issuerSelect = document.getElementById('issuer-select');
  const issuersErrorEl = document.getElementById('issuers-error');
  const itemCountSelect = document.getElementById('item-count');
  const itemRows = document.getElementById('item-rows');
  const dateOfIssue = document.getElementById('date-of-issue');
  const backedUpSelect = document.getElementById('backed-up-select');
  const applyOptionsList = document.getElementById('apply-options-list');
  const applyOptionsSummary = document.getElementById('apply-options-summary');
  const extraSection = document.getElementById('extra-fields-section');
  const extraFieldsEl = document.getElementById('extra-fields');
  const submitBtn = document.getElementById('submit-btn');

  function showStatus(message, isError) {
    statusEl.textContent = message;
    statusEl.hidden = false;
    statusEl.classList.toggle('error', !!isError);
  }

  function bindText(id, key) {
    const el = document.getElementById(id);
    answers[key] = answers[key] || '';
    el.value = answers[key];
    el.addEventListener('input', () => {
      answers[key] = el.value;
    });
  }

  function renderItemRows() {
    itemRows.innerHTML = '';
    for (let n = 1; n <= itemCount; n++) {
      const serialKey = `serial_number_${n}`;
      const descKey = `discription_of_item_${n}`;
      answers[serialKey] = answers[serialKey] || '';
      answers[descKey] = answers[descKey] || '';

      const serialLabel = document.createElement('label');
      serialLabel.textContent = `Serial Number ${n}:`;
      const serialInput = document.createElement('input');
      serialInput.type = 'text';
      serialInput.value = answers[serialKey];
      serialInput.addEventListener('input', () => {
        answers[serialKey] = serialInput.value;
      });

      const descLabel = document.createElement('label');
      descLabel.textContent = `Description of Item ${n}:`;
      const descInput = document.createElement('input');
      descInput.type = 'text';
      descInput.value = answers[descKey];
      descInput.addEventListener('input', () => {
        answers[descKey] = descInput.value;
      });

      itemRows.append(serialLabel, serialInput, descLabel, descInput);
    }
  }

  function onItemCountChange() {
    const newCount = parseInt(itemCountSelect.value, 10);
    // Clear rows no longer shown so a stale value from a previously
    // higher count can't get silently submitted.
    for (let n = newCount + 1; n <= 5; n++) {
      delete answers[`serial_number_${n}`];
      delete answers[`discription_of_item_${n}`];
    }
    itemCount = newCount;
    renderItemRows();
  }

  function renderIssuers() {
    issuerSelect.innerHTML = '<option value="">Select who is issuing this...</option>';
    issuers.forEach((issuer, idx) => {
      const opt = document.createElement('option');
      opt.value = String(idx);
      opt.textContent = issuer.name_grade_org;
      issuerSelect.appendChild(opt);
    });
  }

  function onIssuerChange() {
    const idx = issuerSelect.value;
    if (idx === '') return;
    const issuer = issuers[parseInt(idx, 10)];
    answers['Issued_to_name_grade_orgn'] = issuer.name_grade_org;
    answers['issued_to_signature'] = issuer.name;
  }

  function onBackedUpChange() {
    const value = backedUpSelect.value;
    if (value === 'yes') {
      answers['yes'] = crossedOut;
      answers['no'] = '';
    } else if (value === 'no') {
      answers['yes'] = '';
      answers['no'] = crossedOut;
    }
  }

  function updateApplySummary() {
    const selectedLabels = applyOptions
      .filter((opt) => answers[opt.field_key] === crossedOut)
      .map((opt) => opt.label);
    applyOptionsSummary.textContent = selectedLabels.length ? selectedLabels.join(', ') : 'None selected';
  }

  function renderApplyOptions() {
    applyOptionsList.innerHTML = '';
    applyOptions.forEach((opt) => {
      answers[opt.field_key] = answers[opt.field_key] || '';
      const label = document.createElement('label');
      const checkbox = document.createElement('input');
      checkbox.type = 'checkbox';
      checkbox.checked = answers[opt.field_key] === crossedOut;
      checkbox.addEventListener('change', () => {
        answers[opt.field_key] = checkbox.checked ? crossedOut : '';
        updateApplySummary();
      });
      label.append(checkbox, document.createTextNode(opt.label));
      applyOptionsList.appendChild(label);
    });
    updateApplySummary();
  }

  function isChecked(value) {
    const v = (value || '').trim().toLowerCase();
    return v === 'yes' || v === 'true' || v === '1';
  }

  function renderExtraFields(fields) {
    if (!fields.length) {
      extraSection.hidden = true;
      return;
    }
    extraSection.hidden = false;
    extraFieldsEl.innerHTML = '';
    fields.forEach((field) => {
      answers[field.field_key] = answers[field.field_key] || '';
      const label = document.createElement('label');
      label.textContent = `${field.label}:`;

      if (field.kind === 'Checkbox') {
        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.checked = isChecked(answers[field.field_key]);
        checkbox.addEventListener('change', () => {
          answers[field.field_key] = checkbox.checked ? 'yes' : 'no';
        });
        extraFieldsEl.append(label, checkbox);
      } else {
        const input = document.createElement('input');
        input.type = 'text';
        input.value = answers[field.field_key];
        input.addEventListener('input', () => {
          answers[field.field_key] = input.value;
        });
        extraFieldsEl.append(label, input);
      }
    });
  }

  async function init() {
    let data;
    try {
      const res = await fetch('/api/issue-receipt/init');
      data = await res.json();
    } catch (err) {
      schemaErrorEl.textContent = `Failed to load form: ${err}`;
      schemaErrorEl.hidden = false;
      return;
    }

    if (data.schema_error) {
      schemaErrorEl.textContent = data.schema_error;
      schemaErrorEl.hidden = false;
      return;
    }

    formCard.hidden = false;

    crossedOut = data.crossed_out_marker;
    applyOptions = data.apply_options;
    issuers = data.issuers;

    if (data.issuers_error) {
      issuersErrorEl.textContent = data.issuers_error;
      issuersErrorEl.hidden = false;
    } else if (!issuers.length) {
      issuersErrorEl.textContent = '(no issuers configured)';
      issuersErrorEl.hidden = false;
    }

    answers['date_of_issue'] = data.date_of_issue;
    dateOfIssue.textContent = data.date_of_issue;

    renderIssuers();
    renderItemRows();
    renderApplyOptions();
    renderExtraFields(data.extra_fields);

    bindText('f-POC_name', 'POC_name');
    bindText('f-POC_unit_org', 'POC_unit_org');
    bindText('f-POC_contact_number', 'POC_contact_number');
    bindText('f-ticket_number', 'ticket_number');
    bindText('f-remarks', 'remarks');
    bindText('f-customer_intial', 'customer_intial');

    issuerSelect.addEventListener('change', onIssuerChange);
    itemCountSelect.addEventListener('change', onItemCountChange);
    backedUpSelect.addEventListener('change', onBackedUpChange);
  }

  async function onSubmit() {
    submitBtn.disabled = true;
    try {
      const res = await fetch('/api/issue-receipt/submit', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ fields: answers }),
      });
      const data = await res.json();
      if (data.ok) {
        // The submission itself is what matters most and is saved
        // above; filling the PDF is best-effort on top of that. Either
        // way the ticket is captured, so we still return Home for the
        // next customer even if the fill failed — the error just
        // surfaced in the status message carried over to Home.
        sessionStorage.setItem('cst_status', data.message);
        window.location.href = '/';
        return;
      }
      showStatus(data.message, true);
    } catch (err) {
      showStatus(`Failed to save ticket: ${err}`, true);
    } finally {
      submitBtn.disabled = false;
    }
  }

  submitBtn.addEventListener('click', onSubmit);
  init();
})();
