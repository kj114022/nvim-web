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
/**
 * Session storage manager using IndexedDB
 */
class SessionStorage {
    db = null;
    _initPromise = null;
    /**
     * Open the database connection
     */
    async open() {
        if (this.db)
            return this.db;
        if (this._initPromise)
            return this._initPromise;
        this._initPromise = new Promise((resolve, reject) => {
            const request = indexedDB.open(DB_NAME, DB_VERSION);
            request.onupgradeneeded = (event) => {
                const db = event.target.result;
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
                this.db = event.target.result;
                console.log("[SessionStorage] Database opened");
                resolve(this.db);
            };
            request.onerror = (event) => {
                console.error("[SessionStorage] Failed to open database:", event.target.error);
                reject(event.target.error);
            };
        });
        return this._initPromise;
    }
    /**
     * Save a file's state
     */
    async saveFile(path, content, cursor, dirty = false) {
        const db = await this.open();
        return new Promise((resolve, reject) => {
            const tx = db.transaction("files", "readwrite");
            const store = tx.objectStore("files");
            const record = {
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
    async getFile(path) {
        const db = await this.open();
        return new Promise((resolve, reject) => {
            const tx = db.transaction("files", "readonly");
            const store = tx.objectStore("files");
            const request = store.get(path);
            request.onsuccess = () => resolve(request.result || null);
            request.onerror = () => reject(request.error);
        });
    }
    /**
     * List all open files in the session
     */
    async listOpenFiles() {
        const db = await this.open();
        return new Promise((resolve, reject) => {
            const tx = db.transaction("files", "readonly");
            const store = tx.objectStore("files");
            const request = store.getAll();
            request.onsuccess = () => resolve(request.result || []);
            request.onerror = () => reject(request.error);
        });
    }
    /**
     * Get only dirty (unsaved) files
     */
    async getDirtyFiles() {
        const files = await this.listOpenFiles();
        return files.filter((f) => f.dirty);
    }
    /**
     * Delete a file from the session
     */
    async deleteFile(path) {
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
    async saveState(key, value) {
        const db = await this.open();
        return new Promise((resolve, reject) => {
            const tx = db.transaction("state", "readwrite");
            const store = tx.objectStore("state");
            const record = { key, value, modified: Date.now() };
            const request = store.put(record);
            request.onsuccess = () => resolve();
            request.onerror = () => reject(request.error);
        });
    }
    /**
     * Get session state metadata
     */
    async getState(key) {
        const db = await this.open();
        return new Promise((resolve, reject) => {
            const tx = db.transaction("state", "readonly");
            const store = tx.objectStore("state");
            const request = store.get(key);
            request.onsuccess = () => {
                const result = request.result;
                resolve(result ? result.value : null);
            };
            request.onerror = () => reject(request.error);
        });
    }
    /**
     * Check if there's a restorable session
     */
    async hasSession() {
        const files = await this.listOpenFiles();
        return files.length > 0;
    }
    /**
     * Get session summary for restore prompt
     */
    async getSessionSummary() {
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
    async clear() {
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
    async export() {
        const db = await this.open();
        return new Promise((resolve, reject) => {
            const tx = db.transaction(["state", "files"], "readonly");
            const stateRequest = tx.objectStore("state").getAll();
            const filesRequest = tx.objectStore("files").getAll();
            tx.oncomplete = () => {
                resolve({
                    state: stateRequest.result,
                    files: filesRequest.result,
                });
            };
            tx.onerror = () => reject(tx.error);
        });
    }
}
window.__sessionStorage = new SessionStorage();
console.log("[SessionStorage] Module loaded");
export { SessionStorage };
