/**
 * Session Storage using IndexedDB
 *
 * Persists session state (open files, cursor positions, dirty buffers)
 * to survive browser refresh and accidental close.
 *
 * Database Schema:
 * - "state": Key-value session metadata
 * - "files": Open file buffers with content and cursor
 */

const DB_NAME = "nvim-web-session";
const DB_VERSION = 1;

interface CursorPosition {
  line: number;
  col: number;
}

interface FileRecord {
  path: string;
  content: string;
  cursor: CursorPosition;
  dirty: boolean;
  modified: number;
}

interface StateRecord {
  key: string;
  value: unknown;
  modified: number;
}

interface SessionSummary {
  fileCount: number;
  dirtyCount: number;
  lastModified: number;
}

interface SessionExport {
  state: StateRecord[];
  files: FileRecord[];
}

/**
 * Session storage manager using IndexedDB
 */
class SessionStorage {
  private db: IDBDatabase | null = null;
  private _initPromise: Promise<IDBDatabase> | null = null;

  /**
   * Open the database connection
   */
  async open(): Promise<IDBDatabase> {
    if (this.db) return this.db;
    if (this._initPromise) return this._initPromise;

    this._initPromise = new Promise((resolve, reject) => {
      const request = indexedDB.open(DB_NAME, DB_VERSION);

      request.onupgradeneeded = (event) => {
        const db = (event.target as IDBOpenDBRequest).result;

        // State store: key-value pairs for session metadata
        if (!db.objectStoreNames.contains("state")) {
          db.createObjectStore("state", { keyPath: "key" });
        }

        // Files store: open file buffers
        if (!db.objectStoreNames.contains("files")) {
          const filesStore = db.createObjectStore("files", { keyPath: "path" });
          filesStore.createIndex("modified", "modified", { unique: false });
          filesStore.createIndex("dirty", "dirty", { unique: false });
        }
      };

      request.onsuccess = (event) => {
        this.db = (event.target as IDBOpenDBRequest).result;
        console.log("[SessionStorage] Database opened");
        resolve(this.db);
      };

      request.onerror = (event) => {
        console.error(
          "[SessionStorage] Failed to open database:",
          (event.target as IDBOpenDBRequest).error
        );
        reject((event.target as IDBOpenDBRequest).error);
      };
    });

    return this._initPromise;
  }

  /**
   * Save a file's state
   */
  async saveFile(
    path: string,
    content: string,
    cursor: CursorPosition | undefined,
    dirty = false
  ): Promise<void> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction("files", "readwrite");
      const store = tx.objectStore("files");

      const record: FileRecord = {
        path,
        content,
        cursor: cursor || { line: 1, col: 0 },
        dirty,
        modified: Date.now(),
      };

      const request = store.put(record);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  /**
   * Get a file's saved state
   */
  async getFile(path: string): Promise<FileRecord | null> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction("files", "readonly");
      const store = tx.objectStore("files");

      const request = store.get(path);
      request.onsuccess = () => resolve((request.result as FileRecord) || null);
      request.onerror = () => reject(request.error);
    });
  }

  /**
   * List all open files in the session
   */
  async listOpenFiles(): Promise<FileRecord[]> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction("files", "readonly");
      const store = tx.objectStore("files");

      const request = store.getAll();
      request.onsuccess = () =>
        resolve((request.result as FileRecord[]) || []);
      request.onerror = () => reject(request.error);
    });
  }

  /**
   * Get only dirty (unsaved) files
   */
  async getDirtyFiles(): Promise<FileRecord[]> {
    const files = await this.listOpenFiles();
    return files.filter((f) => f.dirty);
  }

  /**
   * Delete a file from the session
   */
  async deleteFile(path: string): Promise<void> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction("files", "readwrite");
      const store = tx.objectStore("files");

      const request = store.delete(path);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  /**
   * Save session state metadata
   */
  async saveState(key: string, value: unknown): Promise<void> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction("state", "readwrite");
      const store = tx.objectStore("state");

      const record: StateRecord = { key, value, modified: Date.now() };
      const request = store.put(record);
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
    });
  }

  /**
   * Get session state metadata
   */
  async getState<T = unknown>(key: string): Promise<T | null> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction("state", "readonly");
      const store = tx.objectStore("state");

      const request = store.get(key);
      request.onsuccess = () => {
        const result = request.result as StateRecord | undefined;
        resolve(result ? (result.value as T) : null);
      };
      request.onerror = () => reject(request.error);
    });
  }

  /**
   * Check if there's a restorable session
   */
  async hasSession(): Promise<boolean> {
    const files = await this.listOpenFiles();
    return files.length > 0;
  }

  /**
   * Get session summary for restore prompt
   */
  async getSessionSummary(): Promise<SessionSummary> {
    const files = await this.listOpenFiles();
    const dirtyCount = files.filter((f) => f.dirty).length;
    const lastModified = Math.max(...files.map((f) => f.modified), 0);

    return {
      fileCount: files.length,
      dirtyCount,
      lastModified,
    };
  }

  /**
   * Clear all session data
   */
  async clear(): Promise<void> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(["state", "files"], "readwrite");

      tx.objectStore("state").clear();
      tx.objectStore("files").clear();

      tx.oncomplete = () => {
        console.log("[SessionStorage] Session cleared");
        resolve();
      };
      tx.onerror = () => reject(tx.error);
    });
  }

  /**
   * Export session for backup
   */
  async export(): Promise<SessionExport> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(["state", "files"], "readonly");

      const stateRequest = tx.objectStore("state").getAll();
      const filesRequest = tx.objectStore("files").getAll();

      tx.oncomplete = () => {
        resolve({
          state: stateRequest.result as StateRecord[],
          files: filesRequest.result as FileRecord[],
        });
      };
      tx.onerror = () => reject(tx.error);
    });
  }
}

// Global singleton instance
declare global {
  interface Window {
    __sessionStorage: SessionStorage;
  }
}

window.__sessionStorage = new SessionStorage();

console.log("[SessionStorage] Module loaded");

export { SessionStorage };
