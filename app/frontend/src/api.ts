import { API_BASE, PreviewResponse, QueueItem, VersionInfo } from "./state";

export async function fetchQueue(): Promise<QueueItem[] | null> {
  const response = await fetch(`${API_BASE}/api/queue`);
  if (!response.ok) {
    return null;
  }
  return (await response.json()) as QueueItem[];
}

export async function fetchVersion(): Promise<VersionInfo | null> {
  const response = await fetch(`${API_BASE}/api/version`);
  if (!response.ok) {
    return null;
  }
  return (await response.json()) as VersionInfo;
}

export async function fetchDefaultDir(): Promise<string> {
  const response = await fetch(`${API_BASE}/api/default-dir`);
  if (!response.ok) {
    return "";
  }
  const data = (await response.json()) as { path: string };
  return data.path;
}

export async function postAddQueue(url: string): Promise<boolean> {
  const response = await fetch(`${API_BASE}/api/queue/add`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ url }),
  });
  return response.ok;
}

export async function postUpdateQueue(
  id: string,
  payload: { title?: string; artist?: string },
): Promise<void> {
  await fetch(`${API_BASE}/api/queue/update`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id, ...payload }),
  });
}

export async function deleteQueueItem(id: string): Promise<void> {
  await fetch(`${API_BASE}/api/queue/${id}`, { method: "DELETE" });
}

export async function postClearQueue(mode: string): Promise<void> {
  await fetch(`${API_BASE}/api/queue/clear`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ mode }),
  });
}

export async function postDownloadAll(format: string): Promise<void> {
  await fetch(`${API_BASE}/api/download`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ format }),
  });
}

export async function postImportQueue(file: File): Promise<boolean> {
  const form = new FormData();
  form.append("file", file);
  const response = await fetch(`${API_BASE}/api/import`, { method: "POST", body: form });
  return response.ok;
}

export async function postExportQueue(format: string): Promise<Blob | null> {
  const response = await fetch(`${API_BASE}/api/export`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ format }),
  });
  if (!response.ok) {
    return null;
  }
  return response.blob();
}

export async function fetchSample(): Promise<Blob | null> {
  const response = await fetch(`${API_BASE}/api/sample`);
  if (!response.ok) {
    return null;
  }
  return response.blob();
}

export async function fetchPreview(id: string): Promise<PreviewResponse | null> {
  const response = await fetch(`${API_BASE}/api/preview/${id}`);
  if (!response.ok) {
    return null;
  }
  return (await response.json()) as PreviewResponse;
}
