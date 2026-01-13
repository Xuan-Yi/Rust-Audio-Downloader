import "./style.css";

import {
  deleteQueueItem,
  fetchDefaultDir,
  fetchPreview,
  fetchQueue,
  fetchSample,
  fetchVersion,
  postAddQueue,
  postClearQueue,
  postDownloadAll,
  postExportQueue,
  postImportQueue,
  postUpdateQueue,
} from "./api";
import { state } from "./state";
import { render, renderShell, renderQueue, syncActionsCollapse, syncPreviewPlayer } from "./ui";
import { isValidYoutubeUrl } from "./utils";

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) {
  throw new Error("Missing app root");
}

async function bootstrap(): Promise<void> {
  if (!app.dataset.initialized) {
    renderShell(app);
    bindEvents();
  }

  await Promise.all([loadQueue(), loadVersion(), loadDefaultDir()]);
  render();
  setInterval(async () => {
    await loadQueue();
    renderQueue();
  }, 3000);
}

async function loadQueue(): Promise<void> {
  const queue = await fetchQueue();
  if (!queue) {
    return;
  }
  state.queue = queue;
  if (state.preview.id && !state.queue.some((item) => item.id === state.preview.id)) {
    state.preview = { id: "", url: "" };
    syncPreviewPlayer(false);
  }
}

async function loadVersion(): Promise<void> {
  const version = await fetchVersion();
  if (!version) {
    return;
  }
  state.version = version;
}

async function loadDefaultDir(): Promise<void> {
  state.dir = await fetchDefaultDir();
}

function bindEvents(): void {
  const addBtn = document.querySelector<HTMLButtonElement>("#addBtn");
  const urlInput = document.querySelector<HTMLInputElement>("#urlInput");
  const formatSelect = document.querySelector<HTMLSelectElement>("#formatSelect");
  const exportFormatSelect = document.querySelector<HTMLSelectElement>("#exportFormatSelect");
  const downloadBtn = document.querySelector<HTMLButtonElement>("#downloadBtn");
  const exportBtn = document.querySelector<HTMLButtonElement>("#exportBtn");
  const sampleBtn = document.querySelector<HTMLButtonElement>("#sampleBtn");
  const importInput = document.querySelector<HTMLInputElement>("#importInput");
  const importBrowseBtn = document.querySelector<HTMLButtonElement>("#importBrowseBtn");
  const clearCompleteBtn = document.querySelector<HTMLButtonElement>("#clearCompleteBtn");
  const clearFailedBtn = document.querySelector<HTMLButtonElement>("#clearFailedBtn");
  const clearAllBtn = document.querySelector<HTMLButtonElement>("#clearAllBtn");

  addBtn?.addEventListener("click", async () => {
    if (!urlInput) {
      return;
    }
    const value = urlInput.value.trim();
    if (!value) {
      return;
    }
    if (!isValidYoutubeUrl(value)) {
      state.urlError = "Please enter a valid YouTube URL.";
      render();
      return;
    }
    state.urlError = "";
    await addQueue(value);
    urlInput.value = "";
  });

  urlInput?.addEventListener("keydown", async (event) => {
    if (event.key !== "Enter") {
      return;
    }
    const value = urlInput.value.trim();
    if (!value) {
      return;
    }
    if (!isValidYoutubeUrl(value)) {
      state.urlError = "Please enter a valid YouTube URL.";
      render();
      return;
    }
    state.urlError = "";
    await addQueue(value);
    urlInput.value = "";
  });

  formatSelect?.addEventListener("change", () => {
    state.format = formatSelect.value;
  });

  exportFormatSelect?.addEventListener("change", () => {
    state.exportFormat = exportFormatSelect.value;
  });

  downloadBtn?.addEventListener("click", async () => {
    await downloadAll();
  });

  exportBtn?.addEventListener("click", async () => {
    await exportQueue();
  });

  sampleBtn?.addEventListener("click", async () => {
    await downloadSample();
  });

  importBrowseBtn?.addEventListener("click", () => {
    importInput?.click();
  });

  importInput?.addEventListener("change", async () => {
    if (!importInput.files || importInput.files.length === 0) {
      return;
    }
    await importQueue(importInput.files[0]);
    importInput.value = "";
  });

  clearCompleteBtn?.addEventListener("click", () => clearQueue("complete"));
  clearFailedBtn?.addEventListener("click", () => clearQueue("failed"));
  clearAllBtn?.addEventListener("click", () => clearQueue("all"));
  window.addEventListener("resize", syncActionsCollapse);

  const queueSection = document.querySelector<HTMLDivElement>("#queueSection");
  queueSection?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const card = target.closest<HTMLDivElement>(".queue-card");
    const id = card?.dataset.id;
    if (!id) {
      return;
    }
    if (target.closest("button.preview")) {
      previewItem(id);
    }
    if (target.closest("button.delete")) {
      deleteItem(id);
    }
  });

  queueSection?.addEventListener("focusout", (event) => {
    const target = event.target as HTMLElement;
    if (!(target instanceof HTMLInputElement)) {
      return;
    }
    const card = target.closest<HTMLDivElement>(".queue-card");
    const id = card?.dataset.id;
    if (!id) {
      return;
    }
    if (target.classList.contains("title")) {
      updateQueue(id, { title: target.value });
    }
    if (target.classList.contains("artist")) {
      updateQueue(id, { artist: target.value });
    }
  });
}

async function addQueue(url: string): Promise<void> {
  state.isAdding = true;
  render();
  setBusy(true, "Fetching video info...");
  try {
    const ok = await postAddQueue(url);
    if (!ok) {
      return;
    }
    await loadQueue();
    renderQueue();
  } finally {
    setBusy(false);
    state.isAdding = false;
    render();
  }
}

async function updateQueue(id: string, payload: { title?: string; artist?: string }): Promise<void> {
  await postUpdateQueue(id, payload);
}

async function deleteItem(id: string): Promise<void> {
  await deleteQueueItem(id);
  await loadQueue();
  renderQueue();
}

async function clearQueue(mode: string): Promise<void> {
  await postClearQueue(mode);
  await loadQueue();
  renderQueue();
}

async function downloadAll(): Promise<void> {
  if (!state.dir) {
    return;
  }
  await postDownloadAll(state.format);
}

async function importQueue(file: File): Promise<void> {
  setBusy(true, "Loading items from file (yt-dlp can take a while)...");
  try {
    const ok = await postImportQueue(file);
    if (!ok) {
      return;
    }
    await loadQueue();
    renderQueue();
  } finally {
    setBusy(false);
  }
}

async function exportQueue(): Promise<void> {
  const blob = await postExportQueue(state.exportFormat);
  if (!blob) {
    return;
  }
  triggerDownload(blob, `AudioDownloader_export.${state.exportFormat}`);
}

async function downloadSample(): Promise<void> {
  const blob = await fetchSample();
  if (!blob) {
    return;
  }
  triggerDownload(blob, "Sample.xlsx");
}

async function previewItem(id: string): Promise<void> {
  const data = await fetchPreview(id);
  if (!data) {
    return;
  }
  state.preview = { id, url: data.url };
  syncPreviewPlayer(true);
}

function setBusy(active: boolean, message?: string): void {
  if (active) {
    state.busyCount += 1;
    if (message) {
      state.busyMessage = message;
    }
    state.isBusy = true;
  } else {
    state.busyCount = Math.max(0, state.busyCount - 1);
    if (state.busyCount === 0) {
      state.isBusy = false;
      state.busyMessage = "";
    }
  }
  render();
}

function triggerDownload(blob: Blob, name: string): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

bootstrap().catch((error) => {
  console.error(error);
});
