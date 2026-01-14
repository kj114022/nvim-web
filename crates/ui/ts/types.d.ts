// Type augmentations for File System Access API
// These extend the built-in types with missing methods

declare global {
  interface FileSystemDirectoryHandle {
    entries(): AsyncIterableIterator<[string, FileSystemHandle]>;
    keys(): AsyncIterableIterator<string>;
    values(): AsyncIterableIterator<FileSystemHandle>;
  }

  interface FileSystemWritableFileStream {
    write(data: BufferSource | Blob | string): Promise<void>;
  }
}

export {};
