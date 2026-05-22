import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { Block, BlockPatch, IdentityMap, OpenDocument, WriteResult, Hash32, Comment } from "./types/blocks";
import type {
  Block as BlockType,
  BlockPatch as BlockPatchType,
  Comment as CommentType,
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
    persona: z.string(),
    review_state: z.enum(["unread", "reviewed", "needs-work", "approved"]),
    comment_count: z.number()
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

export const ActionTemplate = z
  .object({
    verb: z.string(),
    template: z.string()
  })
  .strict();
export type ActionTemplate = z.infer<typeof ActionTemplate>;

export const BinaryArtifact = z
  .object({
    kind: z.enum(["png", "pdf"]),
    data_url: z.string(),
    size: z.number(),
    mime: z.string()
  })
  .strict();
export type BinaryArtifact = z.infer<typeof BinaryArtifact>;

export const AgentSession = z
  .object({
    id: z.string(),
    source: z.enum(["mcp", "manual"]),
    persona: z.string(),
    agent: z.string(),
    project: z.string(),
    connected_at: z.number(),
    last_active: z.number().nullable(),
    is_live: z.boolean(),
    attached_paths: z.array(z.string())
  })
  .strict();
export type AgentSession = z.infer<typeof AgentSession>;

export const InstallResult = z
  .object({
    config_path: z.string(),
    action: z.enum(["created", "updated", "noop"])
  })
  .strict();
export type InstallResult = z.infer<typeof InstallResult>;

export const SessionAttachment = z
  .object({
    session_id: z.string(),
    persona: z.string(),
    agent: z.string(),
    project: z.string(),
    attached_at: z.number()
  })
  .strict();
export type SessionAttachment = z.infer<typeof SessionAttachment>;

export const SendBackResult = z
  .object({
    route: z.literal("mcp"),
    delivered: z.number()
  })
  .strict();
export type SendBackResult = z.infer<typeof SendBackResult>;

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
    myfiles_dir: z.string(),
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

export async function listDrafts(): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("list_drafts");
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("list_drafts", caught);
  }
}

export async function inboxUnreadCount(): Promise<number> {
  try {
    const result = await invoke<unknown>("inbox_unread_count");
    return z.number().parse(result);
  } catch (caught) {
    throw ipcError("inbox_unread_count", caught);
  }
}

export async function createMyFile(name: string): Promise<string> {
  try {
    const result = await invoke<unknown>("create_my_file", { name });
    return z.string().parse(result);
  } catch (caught) {
    throw ipcError("create_my_file", caught);
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

export async function getActionTemplates(): Promise<ActionTemplate[]> {
  try {
    const result = await invoke<unknown>("get_action_templates");
    return z.array(ActionTemplate).parse(result);
  } catch (caught) {
    throw ipcError("get_action_templates", caught);
  }
}

export async function setActionTemplates(templates: ActionTemplate[]): Promise<void> {
  try {
    await invoke<unknown>("set_action_templates", { templates });
  } catch (caught) {
    throw ipcError("set_action_templates", caught);
  }
}

export async function resetActionTemplates(): Promise<ActionTemplate[]> {
  try {
    const result = await invoke<unknown>("reset_action_templates");
    return z.array(ActionTemplate).parse(result);
  } catch (caught) {
    throw ipcError("reset_action_templates", caught);
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

export async function sessionAttachmentsForPath(path: string): Promise<SessionAttachment[]> {
  try {
    const result = await invoke<unknown>("session_attachments_for_path", { path });
    return z.array(SessionAttachment).parse(result);
  } catch (caught) {
    throw ipcError("session_attachments_for_path", caught);
  }
}

export async function sendBackToSession(
  path: string,
  sessionId: string,
  note: string | null,
  actionVerb: string | null
): Promise<SendBackResult> {
  try {
    const result = await invoke<unknown>("send_back_to_session", {
      path,
      sessionId,
      note,
      actionVerb
    });
    return SendBackResult.parse(result);
  } catch (caught) {
    throw ipcError("send_back_to_session", caught);
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

export async function trackPathsInInbox(paths: string[]): Promise<FileMetadata[]> {
  try {
    const result = await invoke<unknown>("track_paths_in_inbox", { paths });
    return z.array(FileMetadata).parse(result);
  } catch (caught) {
    throw ipcError("track_paths_in_inbox", caught);
  }
}

export const copyPathsToInbox = trackPathsInInbox;

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

export async function untrackFile(path: string): Promise<void> {
  try {
    await invoke<unknown>("untrack_file", { path });
  } catch (caught) {
    throw ipcError("untrack_file", caught);
  }
}

export async function deleteFileFromDisk(path: string): Promise<void> {
  try {
    await invoke<unknown>("delete_file_from_disk", { path });
  } catch (caught) {
    throw ipcError("delete_file_from_disk", caught);
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

export async function removeAgentSession(sessionId: string): Promise<void> {
  try {
    await invoke<unknown>("remove_agent_session", { sessionId });
  } catch (caught) {
    throw ipcError("remove_agent_session", caught);
  }
}

export async function disconnectMcpSession(sessionId: string): Promise<void> {
  try {
    await invoke<unknown>("disconnect_mcp_session", { sessionId });
  } catch (caught) {
    throw ipcError("disconnect_mcp_session", caught);
  }
}

export async function installMcpForClaudeCode(): Promise<InstallResult> {
  try {
    const result = await invoke<unknown>("install_mcp_for_claude_code");
    return InstallResult.parse(result);
  } catch (caught) {
    throw ipcError("install_mcp_for_claude_code", caught);
  }
}

export async function installMcpForCodex(): Promise<InstallResult> {
  try {
    const result = await invoke<unknown>("install_mcp_for_codex");
    return InstallResult.parse(result);
  } catch (caught) {
    throw ipcError("install_mcp_for_codex", caught);
  }
}

export async function installMcpForCursor(): Promise<InstallResult> {
  try {
    const result = await invoke<unknown>("install_mcp_for_cursor");
    return InstallResult.parse(result);
  } catch (caught) {
    throw ipcError("install_mcp_for_cursor", caught);
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

export async function readBinaryArtifact(path: string): Promise<BinaryArtifact> {
  try {
    const result = await invoke<unknown>("read_binary_artifact", { docPath: path });
    return BinaryArtifact.parse(result);
  } catch (caught) {
    throw ipcError("read_binary_artifact", caught);
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

export async function updateSidecarComments(docPath: string, comments: CommentType[]): Promise<void> {
  try {
    await invoke<unknown>("update_sidecar_comments", { docPath, comments: z.array(Comment).parse(comments) });
  } catch (caught) {
    throw ipcError("update_sidecar_comments", caught);
  }
}

export async function setCurrentFocus(path: string): Promise<void> {
  try {
    await invoke<unknown>("set_current_focus", { path });
  } catch (caught) {
    throw ipcError("set_current_focus", caught);
  }
}

export async function setReviewState(path: string, reviewState: FileMetadata["review_state"]): Promise<void> {
  try {
    await invoke<unknown>("set_review_state", { path, reviewState });
  } catch (caught) {
    throw ipcError("set_review_state", caught);
  }
}

// ---------------------------------------------------------------------------
// Slice 5 — Ephemeral open model + Recents
// ---------------------------------------------------------------------------

export const OpenMode = z.enum(["tracked", "ephemeral"]);
export type OpenMode = z.infer<typeof OpenMode>;

export const OpenResult = z
  .object({
    mode: OpenMode,
    path: z.string(),
    source: z.string(),
    base_hash: Hash32,
    has_conflict_markers: z.boolean(),
    relative_path: z.string(),
    name: z.string(),
    extension: z.string(),
    size: z.number(),
    mtime: z.number()
  })
  .strict();
export type OpenResult = z.infer<typeof OpenResult>;

export const RecentEntry = z
  .object({
    path: z.string(),
    last_opened: z.number(),
    title: z.string()
  })
  .strict();
export type RecentEntry = z.infer<typeof RecentEntry>;

export async function openPath(path: string): Promise<OpenResult> {
  try {
    const result = await invoke<unknown>("open_path", { path });
    return OpenResult.parse(result);
  } catch (caught) {
    throw ipcError("open_path", caught);
  }
}

export async function closeEphemeralPath(path: string): Promise<void> {
  try {
    await invoke<unknown>("close_ephemeral_path", { path });
  } catch (caught) {
    throw ipcError("close_ephemeral_path", caught);
  }
}

export async function listRecents(): Promise<RecentEntry[]> {
  try {
    const result = await invoke<unknown>("list_recents");
    return z.array(RecentEntry).parse(result);
  } catch (caught) {
    throw ipcError("list_recents", caught);
  }
}

// ---------------------------------------------------------------------------
// Slice 4 — Startup buffer
// ---------------------------------------------------------------------------

export async function takePendingOpens(): Promise<string[]> {
  try {
    const result = await invoke<unknown>("take_pending_opens");
    return z.array(z.string()).parse(result);
  } catch (caught) {
    throw ipcError("take_pending_opens", caught);
  }
}

// ---------------------------------------------------------------------------
// Slice 7 — Agent messages
// ---------------------------------------------------------------------------

export const AgentMessage = z
  .object({
    id: z.string(),
    session_id: z.string(),
    persona: z.string(),
    agent: z.string(),
    severity: z.enum(["info", "warn", "error"]),
    message: z.string(),
    action_artifact_path: z.string().nullable(),
    action_label: z.string().nullable(),
    created_at: z.number()
  })
  .strict();
export type AgentMessage = z.infer<typeof AgentMessage>;

export async function listAgentMessages(): Promise<AgentMessage[]> {
  try {
    const result = await invoke<unknown>("list_agent_messages");
    return z.array(AgentMessage).parse(result);
  } catch (caught) {
    throw ipcError("list_agent_messages", caught);
  }
}

export async function acknowledgeAgentMessage(id: string): Promise<void> {
  try {
    await invoke<unknown>("acknowledge_agent_message", { id });
  } catch (caught) {
    throw ipcError("acknowledge_agent_message", caught);
  }
}

// ---------------------------------------------------------------------------
// Slice 0.5 — Interaction protocol (protocol v1.1.0)
// ---------------------------------------------------------------------------

/** AskUserQuestion-shaped option (§3 envelope). */
export const InteractionOption = z
  .object({
    key: z.string(),
    label: z.string(),
    description: z.string().optional(),
    recommended: z.boolean().optional()
  })
  .passthrough();
export type InteractionOption = z.infer<typeof InteractionOption>;

/** AskUserQuestion-shaped question (§3 envelope). */
export const InteractionQuestion = z
  .object({
    question_id: z.string(),
    question: z.string(),
    header: z.string().optional(),
    multiSelect: z.boolean().optional(),
    options: z.array(InteractionOption)
  })
  .passthrough();
export type InteractionQuestion = z.infer<typeof InteractionQuestion>;

/** A dispatched interaction row as returned by list_interactions / get_interaction. */
export const Interaction = z
  .object({
    interaction_id: z.string(),
    session_id: z.string(),
    class: z.enum(["decision-set", "document-review", "approval-gate", "visual-artifact"]),
    title: z.string().nullable(),
    artifact_path: z.string().nullable(),
    artifact_inline: z.string().nullable(),
    trace_id: z.string().nullable(),
    request_json: z.string(),
    status: z.enum(["pending", "submitted", "draft", "dismissed"]),
    response_json: z.string().nullable(),
    created_at: z.number(),
    responded_at: z.number().nullable(),
    read_at: z.number().nullable()
  })
  .strict();
export type Interaction = z.infer<typeof Interaction>;

/** §4 decision-set response entry. */
export const DecisionSetResponse = z.object({
  question_id: z.string(),
  selected: z.array(z.string()),
  note: z.string().optional()
});
export type DecisionSetResponse = z.infer<typeof DecisionSetResponse>;

/** §4 response payload (common envelope fields). */
export const InteractionResponsePayload = z
  .object({
    interaction_id: z.string(),
    class: z.enum(["decision-set", "document-review", "approval-gate", "visual-artifact"]),
    artifact_path: z.string().nullable().optional(),
    status: z.enum(["submitted", "draft", "dismissed"]),
    submitted_at: z.string(), // ISO-8601 Z
    // decision-set
    responses: z.array(DecisionSetResponse).optional(),
    // approval-gate
    decision: z.string().optional(),
    reason: z.string().optional(),
    // document-review / visual-artifact
    comments: z.array(z.unknown()).optional(),
    edits: z.array(z.unknown()).optional()
  })
  .passthrough();
export type InteractionResponsePayload = z.infer<typeof InteractionResponsePayload>;

export async function listInteractions(): Promise<Interaction[]> {
  try {
    const result = await invoke<unknown>("list_interactions");
    return z.array(Interaction).parse(result);
  } catch (caught) {
    throw ipcError("list_interactions", caught);
  }
}

export async function getInteraction(interactionId: string): Promise<Interaction | null> {
  try {
    const result = await invoke<unknown>("get_interaction", { interactionId });
    return Interaction.nullable().parse(result);
  } catch (caught) {
    throw ipcError("get_interaction", caught);
  }
}

export async function submitInteractionResponse(
  interactionId: string,
  payload: InteractionResponsePayload
): Promise<void> {
  try {
    await invoke<unknown>("submit_interaction_response", {
      interactionId,
      payload
    });
  } catch (caught) {
    throw ipcError("submit_interaction_response", caught);
  }
}
