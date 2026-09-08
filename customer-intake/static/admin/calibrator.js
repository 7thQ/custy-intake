(() => {
  const pageFile = decodeURIComponent(window.location.pathname.split('/').filter(Boolean).pop());
  document.getElementById('opened-label').textContent = `Opened: ${pageFile}`;

  const canvas = document.getElementById('canvas');
  const ctx = canvas.getContext('2d');
  const editorEl = document.getElementById('editor');
  const pageErrorEl = document.getElementById('page-error');
  const fieldListEl = document.getElementById('field-list');
  const noFieldsEl = document.getElementById('no-fields');
  const saveBtn = document.getElementById('save-btn');
  const schemaStatusEl = document.getElementById('schema-status');

  let image = null;
  let boxes = [];
  let selected = null;
  let drag = { type: 'none' };
  let draftRect = null;

  function rotateVec(x, y, sin, cos) {
    return { x: x * cos - y * sin, y: x * sin + y * cos };
  }

  function corners(b) {
    const hw = b.width / 2, hh = b.height / 2;
    const theta = (b.rotation_deg * Math.PI) / 180;
    const sin = Math.sin(theta), cos = Math.cos(theta);
    return [
      [-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh],
    ].map(([lx, ly]) => {
      const r = rotateVec(lx, ly, sin, cos);
      return { x: b.center_x + r.x, y: b.center_y + r.y };
    });
  }

  // Fixed image-pixel offset above a selected box's top edge for the
  // rotate handle. The old desktop editor used a fixed *screen*-pixel
  // offset instead — this is close enough at the sizes these forms
  // render at, without needing a display-scale in this math too.
  function rotateHandle(b) {
    const halfH = b.height / 2;
    const dist = halfH + 30;
    const theta = (b.rotation_deg * Math.PI) / 180;
    const sin = Math.sin(theta), cos = Math.cos(theta);
    const r = rotateVec(0, -dist, sin, cos);
    return { x: b.center_x + r.x, y: b.center_y + r.y };
  }

  function hitTest(px, py) {
    for (let i = boxes.length - 1; i >= 0; i--) {
      const b = boxes[i];
      const theta = (-b.rotation_deg * Math.PI) / 180;
      const sin = Math.sin(theta), cos = Math.cos(theta);
      const r = rotateVec(px - b.center_x, py - b.center_y, sin, cos);
      if (Math.abs(r.x) <= b.width / 2 && Math.abs(r.y) <= b.height / 2) return i;
    }
    return -1;
  }

  function pointDist(a, b) {
    return Math.hypot(a.x - b.x, a.y - b.y);
  }

  function toImagePx(evt) {
    const rect = canvas.getBoundingClientRect();
    const scaleX = canvas.width / rect.width;
    const scaleY = canvas.height / rect.height;
    return { x: (evt.clientX - rect.left) * scaleX, y: (evt.clientY - rect.top) * scaleY };
  }

  function redraw() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    if (image) ctx.drawImage(image, 0, 0);

    boxes.forEach((b, idx) => {
      const isSel = idx === selected;
      const color = isSel ? '#ffc800' : '#00c8ff';
      const pts = corners(b);

      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(pts[0].x, pts[0].y);
      for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i].x, pts[i].y);
      ctx.closePath();
      ctx.stroke();

      ctx.fillStyle = color;
      ctx.font = '16px sans-serif';
      ctx.fillText(b.field_key || '(unnamed)', pts[0].x + 2, pts[0].y - 6);

      if (isSel) {
        const handle = rotateHandle(b);
        ctx.beginPath();
        ctx.moveTo(b.center_x, b.center_y);
        ctx.lineTo(handle.x, handle.y);
        ctx.strokeStyle = color;
        ctx.lineWidth = 1.5;
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(handle.x, handle.y, 8, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();
      }
    });

    if (draftRect) {
      const x = Math.min(draftRect.x0, draftRect.x1);
      const y = Math.min(draftRect.y0, draftRect.y1);
      const w = Math.abs(draftRect.x1 - draftRect.x0);
      const h = Math.abs(draftRect.y1 - draftRect.y0);
      ctx.strokeStyle = '#00c8ff';
      ctx.lineWidth = 2;
      ctx.strokeRect(x, y, w, h);
    }
  }

  function updateRotationDisplay(idx) {
    const el = document.getElementById(`rotation-${idx}`);
    if (el) el.textContent = `rotation: ${Math.round(boxes[idx].rotation_deg)} deg`;
  }

  function renderFieldList() {
    fieldListEl.innerHTML = '';
    noFieldsEl.hidden = boxes.length > 0;

    boxes.forEach((b, idx) => {
      const block = document.createElement('div');
      block.className = 'field-block' + (idx === selected ? ' selected' : '');

      const title = document.createElement('div');
      title.className = 'title';
      title.textContent = b.field_key || '(unnamed)';
      title.onclick = () => {
        selected = idx;
        renderFieldList();
        redraw();
      };
      block.appendChild(title);

      const keyRow = document.createElement('div');
      keyRow.className = 'row';
      const keyLabel = document.createElement('label');
      keyLabel.textContent = 'field_key:';
      const keyInput = document.createElement('input');
      keyInput.type = 'text';
      keyInput.value = b.field_key;
      keyInput.oninput = () => {
        b.field_key = keyInput.value;
        title.textContent = b.field_key || '(unnamed)';
        redraw();
      };
      keyRow.append(keyLabel, keyInput);
      block.appendChild(keyRow);

      const labelRow = document.createElement('div');
      labelRow.className = 'row';
      const labelLabel = document.createElement('label');
      labelLabel.textContent = 'label:';
      const labelInput = document.createElement('input');
      labelInput.type = 'text';
      labelInput.value = b.label;
      labelInput.oninput = () => {
        b.label = labelInput.value;
      };
      labelRow.append(labelLabel, labelInput);
      block.appendChild(labelRow);

      const kindRow = document.createElement('div');
      kindRow.className = 'row';
      const kindLabelEl = document.createElement('label');
      kindLabelEl.textContent = 'kind:';
      kindRow.appendChild(kindLabelEl);
      ['Text', 'Checkbox'].forEach((kindOpt) => {
        const wrapper = document.createElement('label');
        wrapper.style.fontWeight = '400';
        wrapper.style.width = 'auto';
        wrapper.style.marginRight = '10px';
        const radio = document.createElement('input');
        radio.type = 'radio';
        radio.name = `kind-${idx}`;
        radio.checked = b.kind === kindOpt;
        radio.onchange = () => {
          b.kind = kindOpt;
        };
        wrapper.append(radio, document.createTextNode(` ${kindOpt}`));
        kindRow.appendChild(wrapper);
      });
      block.appendChild(kindRow);

      const meta = document.createElement('div');
      meta.className = 'meta';
      meta.id = `rotation-${idx}`;
      meta.textContent = `rotation: ${Math.round(b.rotation_deg)} deg`;
      block.appendChild(meta);

      const deleteBtn = document.createElement('button');
      deleteBtn.className = 'delete-btn';
      deleteBtn.textContent = 'Delete';
      deleteBtn.onclick = () => {
        boxes.splice(idx, 1);
        if (selected === idx) selected = null;
        else if (selected !== null && selected > idx) selected -= 1;
        renderFieldList();
        redraw();
      };
      block.appendChild(deleteBtn);

      fieldListEl.appendChild(block);
    });
  }

  function onPointerDown(evt) {
    canvas.setPointerCapture(evt.pointerId);
    const p = toImagePx(evt);

    if (selected !== null) {
      const handle = rotateHandle(boxes[selected]);
      if (pointDist(p, handle) <= 16) {
        drag = { type: 'rotating', index: selected };
        return;
      }
    }

    const idx = hitTest(p.x, p.y);
    if (idx !== -1) {
      selected = idx;
      renderFieldList();
      const b = boxes[idx];
      drag = { type: 'moving', index: idx, offsetX: p.x - b.center_x, offsetY: p.y - b.center_y };
    } else {
      selected = null;
      renderFieldList();
      drag = { type: 'creating' };
      draftRect = { x0: p.x, y0: p.y, x1: p.x, y1: p.y };
    }
    redraw();
  }

  function onPointerMove(evt) {
    if (drag.type === 'none') return;
    const p = toImagePx(evt);

    if (drag.type === 'creating') {
      draftRect.x1 = p.x;
      draftRect.y1 = p.y;
    } else if (drag.type === 'moving') {
      const b = boxes[drag.index];
      b.center_x = p.x - drag.offsetX;
      b.center_y = p.y - drag.offsetY;
    } else if (drag.type === 'rotating') {
      const b = boxes[drag.index];
      const dx = p.x - b.center_x, dy = p.y - b.center_y;
      b.rotation_deg = (Math.atan2(dy, dx) * 180) / Math.PI + 90;
      updateRotationDisplay(drag.index);
    }
    redraw();
  }

  function onPointerUp() {
    if (drag.type === 'creating' && draftRect) {
      const w = Math.abs(draftRect.x1 - draftRect.x0);
      const h = Math.abs(draftRect.y1 - draftRect.y0);
      if (w > 4 && h > 4) {
        boxes.push({
          field_key: '',
          label: '',
          kind: 'Text',
          center_x: (draftRect.x0 + draftRect.x1) / 2,
          center_y: (draftRect.y0 + draftRect.y1) / 2,
          width: w,
          height: h,
          rotation_deg: 0,
        });
        selected = boxes.length - 1;
        renderFieldList();
      }
    }
    drag = { type: 'none' };
    draftRect = null;
    redraw();
  }

  canvas.addEventListener('pointerdown', onPointerDown);
  canvas.addEventListener('pointermove', onPointerMove);
  canvas.addEventListener('pointerup', onPointerUp);
  canvas.addEventListener('pointercancel', onPointerUp);

  function fitCanvasToViewport() {
    const wrap = document.querySelector('.calibrator-canvas-wrap');
    const availW = Math.max(wrap.clientWidth - 20, 200);
    const availH = Math.max(window.innerHeight - 220, 200);
    const scale = Math.min(availW / canvas.width, availH / canvas.height, 1.0);
    canvas.style.width = `${canvas.width * scale}px`;
    canvas.style.height = `${canvas.height * scale}px`;
  }
  window.addEventListener('resize', () => {
    if (image) fitCanvasToViewport();
  });

  async function loadImage() {
    const res = await fetch(`/admin/api/pages/${encodeURIComponent(pageFile)}/image`);
    if (res.status === 401) {
      window.location.href = '/admin/login';
      return false;
    }
    if (!res.ok) {
      pageErrorEl.textContent = `Failed to render page: ${await res.text()}`;
      pageErrorEl.hidden = false;
      return false;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    image = new Image();
    await new Promise((resolve, reject) => {
      image.onload = resolve;
      image.onerror = reject;
      image.src = url;
    });

    canvas.width = image.naturalWidth;
    canvas.height = image.naturalHeight;
    fitCanvasToViewport();
    editorEl.hidden = false;
    return true;
  }

  async function loadFields() {
    const res = await fetch(`/admin/api/pages/${encodeURIComponent(pageFile)}/fields`);
    if (res.status === 401) {
      window.location.href = '/admin/login';
      return;
    }
    const data = await res.json();
    boxes = data.fields;
    renderFieldList();
    redraw();
  }

  function showSchemaStatus(message, isError) {
    schemaStatusEl.textContent = message;
    schemaStatusEl.hidden = false;
    schemaStatusEl.classList.toggle('error', !!isError);
  }

  saveBtn.addEventListener('click', async () => {
    saveBtn.disabled = true;
    try {
      const res = await fetch(`/admin/api/pages/${encodeURIComponent(pageFile)}/fields`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ fields: boxes }),
      });
      if (!res.ok) {
        showSchemaStatus((await res.text()) || 'Failed to save schema.', true);
        return;
      }
      const data = await res.json();
      showSchemaStatus(`Saved ${data.saved_fields} field(s).`, false);
    } catch (err) {
      showSchemaStatus(`Failed to save schema: ${err}`, true);
    } finally {
      saveBtn.disabled = false;
    }
  });

  (async () => {
    if (await loadImage()) {
      await loadFields();
    }
  })();
})();
