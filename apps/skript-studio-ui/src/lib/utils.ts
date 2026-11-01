import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** Extract the file name (with extension) from a full path. */
export function basename(path: string): string {
  const parts = path.replace(/\\/g, "/").split("/");
  return parts[parts.length - 1] ?? path;
}

/** Strip the extension from a filename. */
export function stripExt(name: string): string {
  const i = name.lastIndexOf(".");
  return i === -1 ? name : name.slice(0, i);
}

/** Get the extension (without the dot) from a filename, lowercased. */
export function extname(name: string): string {
  const i = name.lastIndexOf(".");
  return i === -1 ? "" : name.slice(i + 1).toLowerCase();
}

/** Get the directory portion of a path. */
export function dirname(path: string): string {
  const norm = path.replace(/\\/g, "/");
  const i = norm.lastIndexOf("/");
  return i === -1 ? "" : norm.slice(0, i);
}

/** Join path segments with `/`. Preserves a leading slash if the first
 *  segment is absolute (Unix-style `/home/...` or `C:\...`). */
export function joinPath(...parts: string[]): string {
  if (parts.length === 0) return "";
  const normalized = parts.map((p) => p.replace(/\\/g, "/"));
  const first = normalized[0];
  const isAbsolute = first.startsWith("/");
  const isWindowsDrive = /^[A-Za-z]:\//.test(first);

  const stripped = normalized
    .map((p) => p.replace(/^\/+|\/+$/g, ""))
    .filter((p) => p.length > 0);

  let joined = stripped.join("/");
  if (isWindowsDrive) {
    // Already includes the drive prefix from the first segment.
  } else if (isAbsolute) {
    joined = "/" + joined;
  }
  return joined;
}

/** Format a byte count as a human-readable string. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Debounce any async function. */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  ms: number,
): (...args: A) => void {
  let t: ReturnType<typeof setTimeout> | null = null;
  return (...args: A) => {
    if (t) clearTimeout(t);
    t = setTimeout(() => fn(...args), ms);
  };
}

/** Truncate a string in the middle, keeping both ends visible. */
export function truncateMiddle(s: string, max = 40): string {
  if (s.length <= max) return s;
  const half = Math.floor((max - 1) / 2);
  return `${s.slice(0, half)}…${s.slice(-half)}`;
}

/**
 * Convert a `file://` URI to an OS-native filesystem path. Returns
 * `null` for non-file URIs. Handles the leading triple-slash on Windows
 * (`file:///C:/...`) and the leading slash on Unix (`file:///home/...`).
 *
 * Mirrors what the LSP server expects to receive in `textDocument/didOpen`.
 */
export function fileUriToPath(uri: string): string | null {
  if (!uri.startsWith("file:")) return null;
  // Strip the `file:` prefix
  let rest = uri.slice("file:".length);
  // Decode percent-escapes (e.g. %20 → space). Monaco URIs are encoded.
  try {
    rest = decodeURIComponent(rest);
  } catch {
    // Fall through with the un-decoded form.
  }
  // Strip leading slashes (file:///, file://, file:/)
  rest = rest.replace(/^\/+/, "");

  // Windows drive path: `C:/Users/...` → `C:\Users\...`
  if (/^[A-Za-z]:\//.test(rest)) {
    return rest.replace(/\//g, "\\");
  }
  // Unix path: re-root with `/`
  return "/" + rest;
}

/**
 * Convert an OS-native filesystem path to a `file://` URI. Inverse of
 * `fileUriToPath`. Used when we need to send a path back to the LSP
 * (e.g. manually invoking `textDocument/didOpen`).
 */
export function pathToFileUri(path: string): string {
  const isWindows = /^[A-Za-z]:[\\/]/.test(path);
  const normalized = isWindows ? path.replace(/\\/g, "/") : path;
  const encoded = encodeURIComponent(normalized).replace(/%2F/gi, "/");
  return `file:///${encoded.replace(/^\//, "")}`;
}
