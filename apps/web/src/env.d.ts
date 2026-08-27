/// <reference types="vite/client" />

interface FileSystemHandle {
  queryPermission?(descriptor?: { mode?: "read" | "readwrite" }): Promise<PermissionState>;
  requestPermission?(descriptor?: { mode?: "read" | "readwrite" }): Promise<PermissionState>;
}

interface DataTransferItem {
  getAsFileSystemHandle?(): Promise<FileSystemHandle | null>;
}

interface Window {
  showDirectoryPicker?: () => Promise<FileSystemDirectoryHandle>;
  __TAURI_INTERNALS__?: unknown;
}
