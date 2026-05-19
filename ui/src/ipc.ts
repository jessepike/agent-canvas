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
    last_read_at: z.number().nullable(),
    persona: z.string()
  })
  .strict();
export type FileMetadata = z.infer<typeof FileMetadata>;

export const Persona = z
  .object({
    name: z.string(),
    color: z.string(),
    display_label: z.string(),
    source: z.string()
  })
  .strict();
export type Persona = z.infer<typeof Persona>;

export const PersonaRegistry = z
  .object({
    personas: z.array(Persona),
    warning: z.string().nullable()
  })
  .strict();
export type PersonaRegistry = z.infer<typeof PersonaRegistry>;

export const SendPayload = z
  .object({
    path: z.string(),
    project: z.string(),
    persona: z.string(),
    contents: z.string(),
    note: z.string().nullable()
  })
  .strict();
export type SendPayload = z.infer<typeof SendPayload>;

export const AgentSession = z
  .object({
    id: z.string(),
    persona: z.string(),
    backbone: z.string(),
    context: z.string(),
    connected_at: z.number(),
    last_active: z.number()
  })
  .strict();
export type AgentSession = z.infer<typeof AgentSession>;

export const AddAgentSessionInput = z
  .object({
    persona: z.string(),
    backbone: z.string(),
    context: z.string()
  })
  .strict();
export type AddAgentSessionInput = z.infer<typeof AddAgentSessionInput>;

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

export async function listPersonas(): Promise<PersonaRegistry> {
  try {
    const result = await invoke<unknown>("list_personas");
    return PersonaRegistry.parse(result);
  } catch (caught) {
    throw ipcError("list_personas", caught);
  }
}

export async function sendToClipboard(payload: SendPayload): Promise<string> {
  try {
    const result = await invoke<unknown>("send_to_clipboard", { payload });
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("send_to_clipboard", caught);
  }
}

export async function togglePin(path: string): Promise<boolean> {
  try {
    const result = await invoke<unknown>("toggle_pin", { path });
    return z.boolean().parse(result);
  } catch (caught) {
    throw ipcError("toggle_pin", caught);
  }
}

export async function archiveFile(path: string): Promise<string> {
  try {
    const result = await invoke<unknown>("archive_file", { path });
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("archive_file", caught);
  }
}

export async function listAgentSessions(): Promise<AgentSession[]> {
  try {
    const result = await invoke<unknown>("list_agent_sessions");
    return z.array(AgentSession).parse(result);
  } catch (caught) {
    throw ipcError("list_agent_sessions", caught);
  }
}

export async function addAgentSession(input: AddAgentSessionInput): Promise<AgentSession> {
  try {
    const result = await invoke<unknown>("add_agent_session", { input });
    return AgentSession.parse(result);
  } catch (caught) {
    throw ipcError("add_agent_session", caught);
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
