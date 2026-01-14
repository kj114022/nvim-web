// OPFS (Origin Private File System) bridge for wasm-bindgen
// Provides file operations for VFS backend in browser

interface FsStatResult {
  is_file: boolean;
  is_dir: boolean;
  size: number;
}

interface FsResponse {
  ok: boolean;
  result?: Uint8Array | FsStatResult | string[] | null;
  error?: string;
  id: number;
}

// Get namespace root directory in OPFS
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
  await w.write(new Blob([data as unknown as BlobPart]));
  await w.close();
}

// Get file/directory metadata
async function fsStat(ns: string, path: string): Promise<FsStatResult> {
  const dir = await nsRoot(ns);
  try {
    const fh = await dir.getFileHandle(path);
    const f = await fh.getFile();
    return { is_file: true, is_dir: false, size: f.size };
  } catch {
    try {
      await dir.getDirectoryHandle(path);
      return { is_file: false, is_dir: true, size: 0 };
    } catch {
      throw new Error(`Path not found: ${path}`);
    }
  }
}

// List directory contents
async function fsList(ns: string, path: string): Promise<string[]> {
  const dir = await nsRoot(ns);
  const targetDir = path ? await dir.getDirectoryHandle(path) : dir;
  const names: string[] = [];
  for await (const [name] of targetDir.entries()) {
    names.push(name);
  }
  return names;
}

// Main handler called from Rust WASM
// Returns: { ok: boolean, result: any, id: number }
export async function handleFsRequest(
  op: string,
  ns: string,
  path: string,
  data: Uint8Array | undefined,
  id: number
): Promise<FsResponse> {
  try {
    let result: Uint8Array | FsStatResult | string[] | null;
    switch (op) {
      case "fs_read":
        result = await fsRead(ns, path);
        break;
      case "fs_write":
        if (!data) throw new Error("Missing data for write");
        await fsWrite(ns, path, data);
        result = null;
        break;
      case "fs_stat":
        result = await fsStat(ns, path);
        break;
      case "fs_list":
        result = await fsList(ns, path);
        break;
      default:
        throw new Error(`Unknown fs operation: ${op}`);
    }
    return { ok: true, result, id };
  } catch (e) {
    const error = e instanceof Error ? e.message : "Unknown error";
    return { ok: false, error, id };
  }
}
