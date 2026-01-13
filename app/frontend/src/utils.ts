import { QueueItem } from "./state";

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

export function stateLabel(state: QueueItem["state"], progress: number | null): string {
  switch (state) {
    case "WAITING":
      return "Pending";
    case "WORKING":
      if (typeof progress === "number") {
        if (progress >= 100) {
          return "Saving";
        }
        return `${Math.round(progress)}%`;
      }
      return "0%";
    case "COMPLETE":
      return "Finished";
    case "FAILED":
      return "Failed";
    default:
      return state;
  }
}

export function badgeContentFor(
  state: QueueItem["state"],
  progress: number | null,
  label: string,
): string {
  if (state === "WORKING" && typeof progress === "number") {
    return `<span class="badge-fill" style="width: ${progress}%"></span><span class="badge-text">${label}</span>`;
  }
  return `<span class="badge-text">${label}</span>`;
}

export function isValidYoutubeUrl(value: string): boolean {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return false;
  }
  if (!["http:", "https:"].includes(url.protocol)) {
    return false;
  }
  const host = url.hostname.toLowerCase();
  if (host === "youtu.be") {
    return Boolean(url.pathname.replace("/", ""));
  }
  if (host === "www.youtube.com" || host === "youtube.com" || host === "m.youtube.com") {
    return url.pathname.startsWith("/watch") && url.searchParams.has("v");
  }
  return false;
}
