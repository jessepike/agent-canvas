import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { Block, BlockPatch, IdentityMap, OpenDocument, WriteResult, Hash32 } from "./types/blocks";
import type {
  Block as BlockType,
  BlockPatch as BlockPatchType,
  IdentityMap as IdentityMapType,
  OpenDocument as OpenDocumentType,
  WriteResult as WriteResultType
} from "./types/blocks";

export const Hash32Schema = Hash32;

export const FileMetadata = z
  .object({
    path: z.string(),
    relative_path: z.string(),
    name: z.string(),
    extension: z.string(),
    size: z.number(),
    mtime: z.number(),
    last_seen_hash: Hash32,
    pinned: z.boolean(),
    archived: z.boolean(),
    last_read_at: z.number().nullable()
  })
  .strict();
export type FileMetadata = z.infer<typeof FileMetadata>;

export const BootstrapInfo = z
  .object({
    canvas_root: z.string(),
    inbox_dir: z.string(),
    projects_dir: z.string(),
    archive_dir: z.string(),
    state_db: z.string(),
    user_path: z.string()
  })
  .strict();
export type BootstrapInfo = z.infer<typeof BootstrapInfo>;

function ipcError(command: string, caught: unknown): Error {
  if (caught instanceof z.ZodError) {
    return new Error(`IPC contract drift: ${command} returned invalid data: ${caught.message}`);
  }

  if (caught instanceof Error) {
    return caught;
  }

  return new Error(String(caught));
}

export async function parseDocument(source: string): Promise<BlockType[]> {
  try {
    const result = await invoke<unknown>("parse_document", { source });
    return z.array(Block).parse(result);
  } catch (caught) {
    throw ipcError("parse_document", caught);
  }
}

export async function getBootstrapInfo(): Promise<BootstrapInfo> {
  try {
    const result = await invoke<unknown>("bootstrap_info");
    return BootstrapInfo.parse(result);
  } catch (caught) {
    throw ipcError("bootstrap_info", caught);
  }
}

export async function listInbox(): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("list_inbox");
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("list_inbox", caught);
  }
}

export async function listProjects(): Promise<string[]> {
  try {
    const result = await invoke<unknown>("list_projects");
    return z.array(z.string()).parse(result);
  } catch (caught) {
    throw ipcError("list_projects", caught);
  }
}

export async function listProjectFiles(project: string): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("list_project_files", { project });
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("list_project_files", caught);
  }
}

export async function saveDocument(source: string, patches: BlockPatchType[]): Promise<string> {
  try {
    const result = await invoke<unknown>("save_document", { source, patches });
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("save_document", caught);
  }
}

export async function openDocument(path: string): Promise<OpenDocumentType> {
  try {
    const result = await invoke<unknown>("open_document", { docPath: path });
    return OpenDocument.parse(result);
  } catch (caught) {
    throw ipcError("open_document", caught);
  }
}

export async function writeDocument(
  path: string,
  source: string,
  baseHash: number[]
): Promise<WriteResultType> {
  try {
    const result = await invoke<unknown>("write_document", {
      docPath: path,
      source,
      baseHash: Hash32.parse(baseHash)
    });
    return WriteResult.parse(result);
  } catch (caught) {
    throw ipcError("write_document", caught);
  }
}

export async function loadSidecar(docPath: string): Promise<IdentityMapType> {
  try {
    const result = await invoke<unknown>("load_sidecar", { docPath });
    return IdentityMap.parse(result);
  } catch (caught) {
    throw ipcError("load_sidecar", caught);
  }
}

export async function saveSidecar(docPath: string, map: IdentityMapType): Promise<void> {
  try {
    const result = await invoke<unknown>("save_sidecar", { docPath, map });
    z.null().parse(result);
  } catch (caught) {
    throw ipcError("save_sidecar", caught);
  }
}
