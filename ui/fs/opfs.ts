// Browser-side OPFS (Origin Private File System) service
// Provides deterministic file operations for VFS backend

// Protocol types
type FsRequest =
  | { type: "fs_read"; ns: string; path: string; id: number }
  | { type: "fs_write"; ns: string; path: string; data: Uint8Array; id: number }
  | { type: "fs_stat"; ns: string; path: string; id: number }
  | { type: "fs_list"; ns: string; path: string; id: number };

type FsResponse =
  | { ok: true; result: any; id: number }
  | { ok: false; error: string; id: number };

// Get namespace root directory in OPFS
// Layout: OPFS/nvim-web/{namespace}/{files}
async function nsRoot(ns: string): Promise<FileSystemDirectoryHandle> {
  const root = await navigator.storage.getDirectory();
  const app = await root.getDirectoryHandle("nvim-web", { create: true });
  return await app.getDirectoryHandle(ns, { create: true });
}

// Read file as bytes
async function fsRead(ns: string, path: string): Promise<Uint8Array> {
  const dir = await nsRoot(ns);
  const fh = await dir.getFileHandle(path);
  const file = await fh.getFile();
  return new Uint8Array(await file.arrayBuffer());
}

// Write file (create or overwrite)
async function fsWrite(ns: string, path: string, data: Uint8Array): Promise<void> {
  const dir = await nsRoot(ns);
  const fh = await dir.getFileHandle(path, { create: true });
  const w = await fh.createWritable();
  await w.write(data);
  await w.close();
}

// Get file/directory metadata
async function fsStat(ns: string, path: string) {
  const dir = await nsRoot(ns);
  try {
    const fh = await dir.getFileHandle(path);
    const f = await fh.getFile();
    return { is_file: true, is_dir: false, size: f.size };
  } catch {
    const dh = await dir.getDirectoryHandle(path);
    return { is_file: false, is_dir: true, size: 0 };
  }
}

// List directory contents (basenames only)
async function fsList(ns: string, path: string): Promise<string[]> {
  const dir = await nsRoot(ns);
  const targetDir = path ? await dir.getDirectoryHandle(path) : dir;
  const names: string[] = [];
  for await (const [name] of targetDir.entries()) {
    names.push(name);
  }
  return names;
}

// Handle FS request from host
export async function handleFsRequest(msg: FsRequest): Promise<FsResponse> {
  try {
    let result;
    switch (msg.type) {
      case "fs_read":
        result = await fsRead(msg.ns, msg.path);
        break;
      case "fs_write":
        await fsWrite(msg.ns, msg.path, msg.data);
        result = null;
        break;
      case "fs_stat":
        result = await fsStat(msg.ns, msg.path);
        break;
      case "fs_list":
        result = await fsList(msg.ns, msg.path);
        break;
      default:
        throw new Error("unknown fs request type");
    }
    return { ok: true, result, id: msg.id };
  } catch (e: any) {
    return { ok: false, error: e?.message ?? "unknown error", id: msg.id };
  }
}
