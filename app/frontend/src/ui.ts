import { API_BASE, state } from "./state";
import { badgeContentFor, escapeHtml, stateLabel } from "./utils";

export function renderShell(app: HTMLDivElement): void {
  app.innerHTML = `
    <section class="hero">
      <div class="title-block">
        <h1>Audio Downloader</h1>
        <p>Queue, preview, and download YouTube audio with curated metadata.</p>
      </div>
      <div class="version-card">
        <h3>Version</h3>
        <span id="versionCurrent">Current: v0.0.0</span>
        <span id="versionLatest">Latest: Unknown</span>
        <span id="versionConsistency" class="version-consistency"></span>
      </div>
    </section>

    <section class="controls">
      <div class="panel">
        <div class="search-row">
          <input id="urlInput" placeholder="Paste YouTube URL here" />
          <button id="addBtn" class="button-accent">Add</button>
        </div>
        <div class="search-status">
          <span id="urlStatus"></span>
          <div id="urlError"></div>
        </div>
        <div class="options-grid">
          <label>
            Output format
            <select id="formatSelect">
              <option value="flac">flac</option>
              <option value="mp3">mp3</option>
              <option value="m4a">m4a</option>
              <option value="wav">wav</option>
            </select>
          </label>
          <label>
            Export format
            <select id="exportFormatSelect">
              <option value="xlsx">xlsx</option>
              <option value="csv">csv</option>
            </select>
          </label>
        </div>
        <div class="actions">
          <button id="downloadBtn">Download All</button>
          <button id="exportBtn" class="secondary">Export Queue</button>
          <button id="importBrowseBtn" class="ghost" type="button">Import List</button>
          <details id="extraActions" class="actions-collapse">
            <summary>More Actions</summary>
            <div class="actions-group">
              <button id="sampleBtn" class="ghost">Sample XLSX</button>
              <button id="clearCompleteBtn" class="ghost">Delete Complete</button>
              <button id="clearFailedBtn" class="ghost">Delete Failed</button>
              <button id="clearAllBtn" class="ghost">Delete All (Except Working)</button>
            </div>
          </details>
        </div>
        <input id="importInput" type="file" accept=".xlsx,.csv" />
        <div id="busyStatus" class="busy-status" aria-live="polite"></div>
      </div>

      <div class="panel player">
        <strong>Preview Player</strong>
        <span id="previewStatus">Idle</span>
        <audio id="previewPlayer" controls></audio>
      </div>
    </section>

    <section class="queue" id="queueSection"></section>

    <section class="footer">
      <span>Backend: ${API_BASE}</span>
      <span>GitHub: https://github.com/Xuan-Yi/Rust-Audio-Downloader.git</span>
    </section>
  `;
  app.dataset.initialized = "true";
}

export function render(): void {
  const versionCurrent = document.querySelector<HTMLSpanElement>("#versionCurrent");
  const versionLatest = document.querySelector<HTMLSpanElement>("#versionLatest");
  if (versionCurrent) {
    versionCurrent.textContent = `Current: ${state.version?.current ?? "v0.0.0"}`;
  }
  if (versionLatest) {
    versionLatest.textContent = `Latest: ${state.version?.latest ?? "Unknown"}`;
  }
  const versionConsistency = document.querySelector<HTMLSpanElement>("#versionConsistency");
  if (versionConsistency) {
    const message = state.version?.consistency?.trim() ?? "";
    if (message) {
      versionConsistency.textContent = message;
      versionConsistency.dataset.active = "true";
    } else {
      versionConsistency.textContent = "";
      delete versionConsistency.dataset.active;
    }
  }

  const formatSelect = document.querySelector<HTMLSelectElement>("#formatSelect");
  if (formatSelect && document.activeElement !== formatSelect) {
    formatSelect.value = state.format;
  }
  const exportFormatSelect = document.querySelector<HTMLSelectElement>("#exportFormatSelect");
  if (exportFormatSelect && document.activeElement !== exportFormatSelect) {
    exportFormatSelect.value = state.exportFormat;
  }
  const busyStatus = document.querySelector<HTMLDivElement>("#busyStatus");
  if (busyStatus) {
    if (state.isBusy) {
      busyStatus.textContent = state.busyMessage || "Loading...";
      busyStatus.dataset.active = "true";
    } else {
      busyStatus.textContent = "";
      delete busyStatus.dataset.active;
    }
  }

  const urlError = document.querySelector<HTMLDivElement>("#urlError");
  if (urlError) {
    urlError.textContent = state.urlError;
    if (state.urlError) {
      urlError.dataset.active = "true";
    } else {
      delete urlError.dataset.active;
    }
  }

  const urlStatus = document.querySelector<HTMLSpanElement>("#urlStatus");
  if (urlStatus) {
    urlStatus.textContent = state.isAdding ? "Loading..." : "";
    if (state.isAdding) {
      urlStatus.dataset.active = "true";
    } else {
      delete urlStatus.dataset.active;
    }
  }

  syncPreviewPlayer(false);
  syncActionsCollapse();
  renderQueue();
}

export function renderQueue(): void {
  const queueSection = document.querySelector<HTMLDivElement>("#queueSection");
  if (!queueSection) {
    return;
  }
  if (state.queue.length === 0) {
    queueSection.innerHTML = `<div class="queue-empty">Queue is empty. Add a YouTube URL to begin.</div>`;
    return;
  }

  queueSection.innerHTML = state.queue
    .map((item) => {
      const badgeClass = item.state.toLowerCase();
      const isActive = item.id === state.preview.id;
      const activeClass = isActive ? " active" : "";
      const progressValue =
        typeof item.progress === "number" ? Math.min(100, Math.max(0, item.progress)) : null;
      const thumbnail = item.thumbnail_url
        ? `<img src="${item.thumbnail_url}" alt="${escapeHtml(item.title)}" />`
        : `<div class="thumb-placeholder"></div>`;
      const error = item.error ? `title="${escapeHtml(item.error)}"` : "";
      const statusLabel = stateLabel(item.state, progressValue);
      const badgeContent = badgeContentFor(item.state, progressValue, statusLabel);
      return `
        <div class="queue-card${activeClass}" data-id="${item.id}">
          ${thumbnail}
          <div class="queue-info">
            <input class="title" value="${escapeHtml(item.title)}" />
            <input class="artist" value="${escapeHtml(item.artist)}" />
            <div class="badge ${badgeClass}" ${error}>${badgeContent}</div>
          </div>
          <div class="queue-actions">
            <button class="preview">Preview</button>
            <button class="delete">Remove</button>
          </div>
        </div>
      `;
    })
    .join("");
}

export function syncPreviewPlayer(autoplay: boolean): void {
  const status = document.querySelector<HTMLSpanElement>("#previewStatus");
  if (status) {
    if (state.preview.id) {
      const current = state.queue.find((item) => item.id === state.preview.id);
      const title = current?.title?.trim() ?? "";
      const artist = current?.artist?.trim() ?? "";
      const label = title || artist ? [title, artist].filter(Boolean).join(" — ") : state.preview.id;
      status.textContent = `Now playing: ${label}`;
    } else {
      status.textContent = "Idle";
    }
  }

  const player = document.querySelector<HTMLAudioElement>("#previewPlayer");
  if (!player) {
    return;
  }
  const nextSrc = state.preview.url ? `${API_BASE}${state.preview.url}` : "";
  if (!nextSrc) {
    player.pause();
    if (player.src) {
      player.removeAttribute("src");
      player.load();
    }
    return;
  }
  if (player.src !== nextSrc) {
    player.src = nextSrc;
    if (nextSrc && autoplay) {
      player.play().catch(() => undefined);
    }
  } else if (nextSrc && autoplay) {
    player.play().catch(() => undefined);
  }
}

export function syncActionsCollapse(): void {
  const extraActions = document.querySelector<HTMLDetailsElement>("#extraActions");
  if (!extraActions) {
    return;
  }
  if (window.matchMedia("(max-width: 980px)").matches) {
    extraActions.removeAttribute("open");
  } else {
    extraActions.setAttribute("open", "true");
  }
}
