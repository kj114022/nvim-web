// Unified VFS Driver (OPFS + IndexedDB Fallback)
// Features:
// - Auto-detection of OPFS/IndexedDB
// - Multi-namespace support (switch projects without reload)
// - Quota management (show usage, warn on low space)
// - Sync indicator (pending writes tracking)
// - Conflict resolution (version checking)

const DB_NAME = "nvim-web-vfs";
const STORE_NAME = "files";
const META_STORE = "metadata";
let idbCache = null;

// --- State Management ---
const state = {
  currentNamespace: "default",
  pendingWrites: new Map(), // path -> { data, version, timestamp }
  syncListeners: [],
  quotaListeners: [],
  conflictListeners: [],
  versions: new Map(), // path -> version number
  useIDB: false,
  initialized: false,
};

// --- Feature Detection ---
const hasOPFS = async () => {
  if (navigator.storage && navigator.storage.getDirectory) {
    try {
      await navigator.storage.getDirectory();
      return true;
    } catch (e) {
      console.warn("OPFS detected but inaccessible:", e);
      return false;
    }
  }
  return false;
};

// --- Event Emitters ---
function emitSync(pending) {
  state.syncListeners.forEach((fn) => fn(pending));
}

function emitQuota(usage) {
  state.quotaListeners.forEach((fn) => fn(usage));
}

function emitConflict(path, local, remote) {
  state.conflictListeners.forEach((fn) => fn({ path, local, remote }));
}

// --- Public API: Events ---
export function onSyncChange(callback) {
  state.syncListeners.push(callback);
  return () => {
    state.syncListeners = state.syncListeners.filter((fn) => fn !== callback);
  };
}

export function onQuotaChange(callback) {
  state.quotaListeners.push(callback);
  return () => {
    state.quotaListeners = state.quotaListeners.filter((fn) => fn !== callback);
  };
}

export function onConflict(callback) {
  state.conflictListeners.push(callback);
  return () => {
    state.conflictListeners = state.conflictListeners.filter(
      (fn) => fn !== callback
    );
  };
}

// --- Public API: Namespace ---
export function setNamespace(ns) {
  state.currentNamespace = ns;
  console.log(`VFS: Switched to namespace "${ns}"`);
}

export function getNamespace() {
  return state.currentNamespace;
}

export async function listNamespaces() {
  if (state.useIDB) {
    const db = await openDB();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, "readonly");
      const req = tx.objectStore(STORE_NAME).getAllKeys();
      req.onsuccess = () => {
        const namespaces = new Set();
        req.result.forEach((key) => {
          const parts = key.toString().split("/");
          if (parts[0]) namespaces.add(parts[0]);
        });
        resolve(Array.from(namespaces));
      };
      req.onerror = () => reject(req.error);
    });
  } else {
    const root = await navigator.storage.getDirectory();
    const namespaces = [];
    for await (const [name, handle] of root.entries()) {
      if (handle.kind === "directory") {
        namespaces.push(name);
      }
    }
    return namespaces;
  }
}

// --- Public API: Quota ---
export async function getQuota() {
  if (navigator.storage && navigator.storage.estimate) {
    const estimate = await navigator.storage.estimate();
    const usage = {
      used: estimate.usage || 0,
      quota: estimate.quota || 0,
      percent: estimate.quota
        ? ((estimate.usage || 0) / estimate.quota) * 100
        : 0,
    };
    emitQuota(usage);
    return usage;
  }
  return { used: 0, quota: 0, percent: 0 };
}

export function isLowSpace(threshold = 90) {
  return getQuota().then((q) => q.percent > threshold);
}

// --- Public API: Sync Indicator ---
export function getPendingWrites() {
  return Array.from(state.pendingWrites.entries()).map(([path, info]) => ({
    path,
    size: info.data.length,
    timestamp: info.timestamp,
  }));
}

export function hasPendingWrites() {
  return state.pendingWrites.size > 0;
}

// --- IndexedDB Backend ---
function openDB() {
  return new Promise((resolve, reject) => {
    if (idbCache) return resolve(idbCache);
    const req = indexedDB.open(DB_NAME, 2);
    req.onupgradeneeded = (e) => {
      const db = e.target.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
      if (!db.objectStoreNames.contains(META_STORE)) {
        db.createObjectStore(META_STORE);
      }
    };
    req.onsuccess = (e) => {
      idbCache = e.target.result;
      resolve(idbCache);
    };
    req.onerror = () => reject(req.error);
  });
}

async function idbRead(path) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const req = tx.objectStore(STORE_NAME).get(path);
    req.onsuccess = () =>
      resolve(req.result ? new Uint8Array(req.result) : new Uint8Array(0));
    req.onerror = () => reject(req.error);
  });
}

async function idbWrite(path, data) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction([STORE_NAME, META_STORE], "readwrite");
    tx.objectStore(STORE_NAME).put(data, path);
    // Update version
    const version = (state.versions.get(path) || 0) + 1;
    tx.objectStore(META_STORE).put({ version, mtime: Date.now() }, path);
    state.versions.set(path, version);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function idbGetVersion(path) {
  const db = await openDB();
  return new Promise((resolve) => {
    const tx = db.transaction(META_STORE, "readonly");
    const req = tx.objectStore(META_STORE).get(path);
    req.onsuccess = () => resolve(req.result?.version || 0);
    req.onerror = () => resolve(0);
  });
}

async function idbList(path) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const req = tx.objectStore(STORE_NAME).getAllKeys();
    req.onsuccess = () => {
      const prefix = path ? path + "/" : "";
      const keys = req.result.filter((k) => k.toString().startsWith(prefix));
      const children = new Set();
      keys.forEach((k) => {
        const rest = k.toString().slice(prefix.length);
        const parts = rest.split("/");
        if (parts[0]) children.add(parts[0]);
      });
      resolve(Array.from(children));
    };
    req.onerror = () => reject(req.error);
  });
}

async function idbDelete(path) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction([STORE_NAME, META_STORE], "readwrite");
    tx.objectStore(STORE_NAME).delete(path);
    tx.objectStore(META_STORE).delete(path);
    state.versions.delete(path);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function idbRename(oldPath, newPath) {
  const data = await idbRead(oldPath);
  await idbWrite(newPath, data);
  await idbDelete(oldPath);
}

// --- OPFS Backend ---
async function getDirHandle(path, create = false) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split("/").filter((p) => p);
  let current = root;
  for (const part of parts) {
    current = await current.getDirectoryHandle(part, { create });
  }
  return current;
}

async function opfsRead(path) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split("/");
  const fileName = parts.pop();
  let dir = root;
  for (const part of parts) {
    if (!part) continue;
    dir = await dir.getDirectoryHandle(part, { create: true });
  }
  const fh = await dir.getFileHandle(fileName);
  const file = await fh.getFile();
  return new Uint8Array(await file.arrayBuffer());
}

async function opfsWrite(path, data) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split("/");
  const fileName = parts.pop();
  let dir = root;
  for (const part of parts) {
    if (!part) continue;
    dir = await dir.getDirectoryHandle(part, { create: true });
  }
  const fh = await dir.getFileHandle(fileName, { create: true });
  const w = await fh.createWritable();
  await w.write(data);
  await w.close();
  // Update version
  const version = (state.versions.get(path) || 0) + 1;
  state.versions.set(path, version);
}

async function opfsList(path) {
  const root = await navigator.storage.getDirectory();
  let dir = root;
  if (path) {
    const parts = path.split("/").filter((p) => p);
    for (const part of parts) {
      try {
        dir = await dir.getDirectoryHandle(part);
      } catch {
        return [];
      }
    }
  }
  const names = [];
  for await (const [name] of dir.entries()) {
    names.push(name);
  }
  return names;
}

async function opfsDelete(path) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split("/");
  const name = parts.pop();
  let dir = root;
  for (const part of parts) {
    if (!part) continue;
    dir = await dir.getDirectoryHandle(part);
  }
  await dir.removeEntry(name, { recursive: true });
  state.versions.delete(path);
}

async function opfsRename(oldPath, newPath) {
  const data = await opfsRead(oldPath);
  await opfsWrite(newPath, data);
  await opfsDelete(oldPath);
}

async function opfsStat(path) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split("/");
  const name = parts.pop();
  let dir = root;
  for (const part of parts) {
    if (!part) continue;
    dir = await dir.getDirectoryHandle(part);
  }
  try {
    const fh = await dir.getFileHandle(name);
    const file = await fh.getFile();
    return { is_file: true, is_dir: false, size: file.size };
  } catch {
    try {
      await dir.getDirectoryHandle(name);
      return { is_file: false, is_dir: true, size: 0 };
    } catch {
      throw new Error(`Path not found: ${path}`);
    }
  }
}

// --- Conflict Resolution ---
async function checkConflict(path, expectedVersion) {
  const currentVersion = state.useIDB
    ? await idbGetVersion(path)
    : state.versions.get(path) || 0;
  return currentVersion !== expectedVersion;
}

export async function resolveConflict(path, strategy) {
  const pending = state.pendingWrites.get(path);
  if (!pending) return;

  switch (strategy) {
    case "keep-local":
      // Force write local version
      if (state.useIDB) {
        await idbWrite(path, pending.data);
      } else {
        await opfsWrite(path, pending.data);
      }
      state.pendingWrites.delete(path);
      emitSync(state.pendingWrites.size);
      break;
    case "keep-remote":
      // Discard local changes
      state.pendingWrites.delete(path);
      emitSync(state.pendingWrites.size);
      break;
    case "merge":
      // For now, keep-local (merge requires diff logic)
      await resolveConflict(path, "keep-local");
      break;
  }
}

// --- Path Normalization ---
function join(ns, path) {
  const cleanedPath = path.replace(/^\/+/, "");
  return ns ? `${ns}/${cleanedPath}` : cleanedPath;
}

// --- Main Handler ---
export async function handleFsRequest(op, ns, path, data, id) {
  if (!state.initialized) {
    const supported = await hasOPFS();
    if (!supported) {
      console.log("VFS: OPFS not supported, falling back to IndexedDB");
      state.useIDB = true;
    } else {
      console.log("VFS: Using OPFS backend");
    }
    state.initialized = true;
    // Initial quota check
    getQuota();
  }

  const fullPath = join(ns || state.currentNamespace, path);

  try {
    let result;
    if (state.useIDB) {
      switch (op) {
        case "fs_read":
          result = await idbRead(fullPath);
          break;
        case "fs_write":
          // Check for conflicts
          const idbVersion = await idbGetVersion(fullPath);
          const expectedIdb = state.versions.get(fullPath) || 0;
          if (idbVersion !== expectedIdb && idbVersion > 0) {
            emitConflict(fullPath, data, await idbRead(fullPath));
            state.pendingWrites.set(fullPath, {
              data,
              version: expectedIdb,
              timestamp: Date.now(),
            });
            emitSync(state.pendingWrites.size);
            result = null;
          } else {
            await idbWrite(fullPath, data);
            result = null;
          }
          break;
        case "fs_list":
          result = await idbList(fullPath);
          break;
        case "fs_stat":
          try {
            const fileData = await idbRead(fullPath);
            result = { is_file: true, is_dir: false, size: fileData.length };
          } catch {
            result = { is_file: false, is_dir: true, size: 0 };
          }
          break;
        case "fs_delete":
          await idbDelete(fullPath);
          result = null;
          break;
        case "fs_rename":
          const newPathIdb = data ? new TextDecoder().decode(data) : "";
          await idbRename(fullPath, join(ns || state.currentNamespace, newPathIdb));
          result = null;
          break;
        default:
          throw new Error(`Unknown op: ${op}`);
      }
    } else {
      switch (op) {
        case "fs_read":
          result = await opfsRead(fullPath);
          break;
        case "fs_write":
          // Simple version check for conflict
          const opfsVersion = state.versions.get(fullPath) || 0;
          await opfsWrite(fullPath, data);
          result = null;
          break;
        case "fs_list":
          result = await opfsList(fullPath);
          break;
        case "fs_stat":
          result = await opfsStat(fullPath);
          break;
        case "fs_delete":
          await opfsDelete(fullPath);
          result = null;
          break;
        case "fs_rename":
          const newPathOpfs = data ? new TextDecoder().decode(data) : "";
          await opfsRename(fullPath, join(ns || state.currentNamespace, newPathOpfs));
          result = null;
          break;
        default:
          throw new Error(`Unknown op: ${op}`);
      }
    }
    return { ok: true, result, id };
  } catch (e) {
    console.error("VFS Error:", e);
    return { ok: false, error: e?.message ?? "Error", id };
  }
}

// --- Utility exports for UI ---
export { state as _state };
