export type QueueItem = {
  id: string;
  youtube_url: string;
  title: string;
  artist: string;
  thumbnail_url?: string;
  duration?: number;
  state: "WAITING" | "WORKING" | "COMPLETE" | "FAILED";
  progress?: number | null;
  error?: string | null;
};

export type VersionInfo = {
  current: string;
  latest?: string;
  is_latest?: boolean;
  consistency?: string;
  release_url?: string;
};

export type PreviewResponse = {
  url: string;
};

export const API_BASE = "http://127.0.0.1:47815";

export const state = {
  queue: [] as QueueItem[],
  version: null as VersionInfo | null,
  format: "flac",
  exportFormat: "xlsx",
  dir: "",
  preview: { id: "", url: "" },
  isBusy: false,
  busyCount: 0,
  busyMessage: "",
  urlError: "",
  isAdding: false,
};
