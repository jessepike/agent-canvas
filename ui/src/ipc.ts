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
    contents: z.string(),
    note: z.string().nullable(),
    action_verb: z.string()
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

export async function listProjectCounts(): Promise<Map<string, number>> {
  try {
    const result = await invoke<unknown>("list_project_counts");
    const counts = z.record(z.string(), z.number()).parse(result);
    return new Map(Object.entries(counts));
  } catch (caught) {
    throw ipcError("list_project_counts", caught);
  }
}

export async function renameProject(oldName: string, newName: string): Promise<void> {
  try {
    await invoke<unknown>("rename_project", { old: oldName, new: newName });
  } catch (caught) {
    throw ipcError("rename_project", caught);
  }
}

export async function deleteProjectIfEmpty(name: string): Promise<void> {
  try {
    await invoke<unknown>("delete_project_if_empty", { name });
  } catch (caught) {
    throw ipcError("delete_project_if_empty", caught);
  }
}

export async function getProjectDefaultAgent(project: string): Promise<string | null> {
  try {
    const result = await invoke<unknown>("get_project_default_agent", { project });
    return z.string().nullable().parse(result);
  } catch (caught) {
    throw ipcError("get_project_default_agent", caught);
  }
}

export async function setProjectDefaultAgent(project: string, sessionId: string): Promise<void> {
  try {
    await invoke<unknown>("set_project_default_agent", { project, sessionId });
  } catch (caught) {
    throw ipcError("set_project_default_agent", caught);
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

export async function reloadPersonaRegistry(): Promise<PersonaRegistry> {
  try {
    const result = await invoke<unknown>("reload_persona_registry");
    return PersonaRegistry.parse(result);
  } catch (caught) {
    throw ipcError("reload_persona_registry", caught);
  }
}

export async function getDefaultActionVerb(): Promise<string> {
  try {
    const result = await invoke<unknown>("get_default_action_verb");
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("get_default_action_verb", caught);
  }
}

export async function setDefaultActionVerb(verb: string): Promise<void> {
  try {
    await invoke<unknown>("set_default_action_verb", { verb });
  } catch (caught) {
    throw ipcError("set_default_action_verb", caught);
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

export async function sendMultiToClipboard(payloads: SendPayload[]): Promise<string> {
  try {
    const result = await invoke<unknown>("send_multi_to_clipboard", { payloads });
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("send_multi_to_clipboard", caught);
  }
}

export async function renameFile(oldPath: string, newName: string): Promise<FileMetadata> {
  try {
    const result = await invoke<unknown>("rename_file", { oldPath, newName });
    return FileMetadata.parse(result);
  } catch (caught) {
    throw ipcError("rename_file", caught);
  }
}

export async function exportFileTo(sourcePath: string, targetPath: string): Promise<void> {
  try {
    await invoke<unknown>("export_file_to", { sourcePath, targetPath });
  } catch (caught) {
    throw ipcError("export_file_to", caught);
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

export type ConflictStrategy = "replace" | "keep_both" | "cancel";

export async function copyPathsToInbox(paths: string[]): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("copy_paths_to_inbox", { paths });
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("copy_paths_to_inbox", caught);
  }
}

export async function moveFileToProject(
  path: string,
  project: string,
  strategy: ConflictStrategy
): Promise<FileMetadata> {
  try {
    const result = await invoke<unknown>("move_file_to_project", { path, project, strategy });
    return FileMetadata.parse(result);
  } catch (caught) {
    throw ipcError("move_file_to_project", caught);
  }
}

export async function moveFileToArchive(path: string, strategy: ConflictStrategy): Promise<FileMetadata> {
  try {
    const result = await invoke<unknown>("move_file_to_archive", { path, strategy });
    return FileMetadata.parse(result);
  } catch (caught) {
    throw ipcError("move_file_to_archive", caught);
  }
}

export async function copyTextToClipboard(text: string): Promise<string> {
  try {
    const result = await invoke<unknown>("copy_text_to_clipboard", { text });
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("copy_text_to_clipboard", caught);
  }
}

export async function revealInFinder(path: string): Promise<void> {
  try {
    await invoke<unknown>("reveal_in_finder", { path });
  } catch (caught) {
    throw ipcError("reveal_in_finder", caught);
  }
}

export async function deleteFile(path: string): Promise<void> {
  try {
    await invoke<unknown>("delete_file", { path });
  } catch (caught) {
    throw ipcError("delete_file", caught);
  }
}

export async function targetFileExists(target: "project" | "archive", filename: string, project?: string): Promise<boolean> {
  try {
    const result = await invoke<unknown>("target_file_exists", { target, project: project ?? null, filename });
    return z.boolean().parse(result);
  } catch (caught) {
    throw ipcError("target_file_exists", caught);
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

export async function listArchive(): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("list_archive");
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("list_archive", caught);
  }
}

export async function listPinned(): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("list_pinned");
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("list_pinned", caught);
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
