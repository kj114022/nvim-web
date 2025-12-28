// Unified VFS Driver (OPFS + IndexedDB Fallback)
// Automatically detects browser capabilities and chooses the best backend.

const DB_NAME = "nvim-web-vfs";
const STORE_NAME = "files";
let idbCache = null;

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

// --- IndexedDB Backend (Fallback) ---
function openDB() {
  return new Promise((resolve, reject) => {
    if (idbCache) return resolve(idbCache);
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = (e) => {
      const db = e.target.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
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
    req.onsuccess = () => resolve(req.result ? new Uint8Array(req.result) : new Uint8Array(0));
    req.onerror = () => reject(req.error);
  });
}

async function idbWrite(path, data) {
  const db = await openDB();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    const req = tx.objectStore(STORE_NAME).put(data, path);
    req.onsuccess = () => resolve();
    req.onerror = () => reject(req.error);
  });
}

async function idbList(path) {
  const db = await openDB();
  // Simple prefix scan for "directory" listing
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const req = tx.objectStore(STORE_NAME).getAllKeys();
    req.onsuccess = () => {
      // Filter keys that start with path/
      // Note: This is a basic simulation. Real usage might need a separate dir tree.
      // For now, we assume flat-ish usage or simple prefix matching.
      const prefix = path ? path + "/" : "";
      const keys = req.result.filter(k => k.toString().startsWith(prefix));
      // Extract immediate children
      const children = new Set();
      keys.forEach(k => {
        const rest = k.toString().slice(prefix.length);
        const parts = rest.split('/');
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
    const tx = db.transaction(STORE_NAME, "readwrite");
    const req = tx.objectStore(STORE_NAME).delete(path);
    req.onsuccess = () => resolve();
    req.onerror = () => reject(req.error);
  });
}

async function idbRename(oldPath, newPath) {
  const data = await idbRead(oldPath);
  await idbWrite(newPath, data);
  await idbDelete(oldPath);
}

// --- OPFS Backend (Primary) ---
async function getDirHandle(path, create = false) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split('/').filter(p => p);
  let current = root;
  for (const part of parts) {
    current = await current.getDirectoryHandle(part, { create });
  }
  return current;
}

async function opfsRead(path) {
  const root = await navigator.storage.getDirectory();
  // Simplified path handling - assuming flat namespace for now or simple nesting
  // Real implementation needs full path walking
  // For nvim-web, we map namespaces. 
  // Let's assume path is a full relative path.
  
  // Recursive helper to get file handle
  const parts = path.split('/');
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
  const parts = path.split('/');
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
}

async function opfsList(path) {
  const root = await navigator.storage.getDirectory();
  let dir = root;
  if (path) {
    const parts = path.split('/').filter(p => p);
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
  const parts = path.split('/');
  const name = parts.pop();
  let dir = root;
  for (const part of parts) {
    if (!part) continue;
    dir = await dir.getDirectoryHandle(part);
  }
  await dir.removeEntry(name, { recursive: true });
}

async function opfsRename(oldPath, newPath) {
  // OPFS doesn't have native rename, so copy and delete
  const data = await opfsRead(oldPath);
  await opfsWrite(newPath, data);
  await opfsDelete(oldPath);
}

async function opfsStat(path) {
  const root = await navigator.storage.getDirectory();
  const parts = path.split('/');
  const name = parts.pop();
  let dir = root;
  for (const part of parts) {
    if (!part) continue;
    dir = await dir.getDirectoryHandle(part);
  }
  
  // Try as file first
  try {
    const fh = await dir.getFileHandle(name);
    const file = await fh.getFile();
    return { is_file: true, is_dir: false, size: file.size };
  } catch {
    // Try as directory
    try {
      await dir.getDirectoryHandle(name);
      return { is_file: false, is_dir: true, size: 0 };
    } catch {
      throw new Error(`Path not found: ${path}`);
    }
  }
}

// --- Main Handler ---
// Delegates to the appropriate backend
let useIDB = false;
let initialized = false;

/*
  Operations:
  - fs_read(ns, path)
  - fs_write(ns, path, data)
  - fs_list(ns, path)
  - fs_stat(ns, path)
  - fs_delete(ns, path)
  - fs_rename(ns, path, newPath) - newPath passed via data parameter
*/

// Normalize path: ns + path
function join(ns, path) {
    // Basic joining: "default/myfile.txt"
    const cleanedPath = path.replace(/^\/+/, '');
    return ns ? `${ns}/${cleanedPath}` : cleanedPath;
}

export async function handleFsRequest(op, ns, path, data, id) {
  if (!initialized) {
    const supported = await hasOPFS();
    if (!supported) {
      console.log("VFS: OPFS not supported, falling back to IndexedDB");
      useIDB = true;
    } else {
      console.log("VFS: Using OPFS backend");
    }
    initialized = true;
  }

  const fullPath = join(ns, path);

  try {
    let result;
    if (useIDB) {
      // IndexedDB Fallback
      switch (op) {
        case "fs_read": result = await idbRead(fullPath); break;
        case "fs_write": await idbWrite(fullPath, data); result = null; break;
        case "fs_list": result = await idbList(fullPath); break;
        case "fs_stat": 
             // Simple stat check
             try {
                 const fileData = await idbRead(fullPath);
                 result = { is_file: true, is_dir: false, size: fileData.length }; 
             } catch {
                 result = { is_file: false, is_dir: true, size: 0 };
             }
             break;
        case "fs_delete": await idbDelete(fullPath); result = null; break;
        case "fs_rename":
             const newPathIdb = data ? new TextDecoder().decode(data) : '';
             await idbRename(fullPath, join(ns, newPathIdb));
             result = null;
             break;
        default: throw new Error(`Unknown op: ${op}`);
      }
    } else {
      // OPFS
      switch (op) {
        case "fs_read": result = await opfsRead(fullPath); break;
        case "fs_write": await opfsWrite(fullPath, data); result = null; break;
        case "fs_list": result = await opfsList(fullPath); break;
        case "fs_stat": result = await opfsStat(fullPath); break;
        case "fs_delete": await opfsDelete(fullPath); result = null; break;
        case "fs_rename":
             const newPathOpfs = data ? new TextDecoder().decode(data) : '';
             await opfsRename(fullPath, join(ns, newPathOpfs));
             result = null;
             break;
        default: throw new Error(`Unknown op: ${op}`);
      }
    }
    return { ok: true, result, id };
  } catch (e) {
    console.error("VFS Error:", e);
    return { ok: false, error: e?.message ?? "Error", id };
  }
}
