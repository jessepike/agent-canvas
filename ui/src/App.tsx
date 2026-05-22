import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Dispatch, MouseEvent as ReactMouseEvent, SetStateAction } from "react";
import { listen, TauriEvent } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { RenderedView } from "./components/RenderedView";
import { SourceView, type SourceFormat, type SourceSelection, type SourceViewHandle } from "./components/SourceView";
import { injectBootstrap } from "./htmlBootstrap";
import { useFocusTrap } from "./hooks/useFocusTrap";
import {
  addAgentSession,
  archiveFile,
  closeEphemeralPath,
  copyTextToClipboard,
  createMyFile,
  deleteFileFromDisk,
  deleteProjectIfEmpty,
  exportFileTo,
  getActionTemplates,
  getDefaultActionVerb,
  getBootstrapInfo,
  getProjectDefaultAgent,
  inboxUnreadCount,
  installMcpForClaudeCode,
  installMcpForCodex,
  installMcpForCursor,
  listAgentSessions,
  listArchive,
  listDrafts,
  listInbox,
  listPinned,
  listProjectFiles,
  listPersonas,
  listProjectCounts,
  listProjects,
  listRecents,
  loadSidecar,
  moveFileToArchive,
  moveFileToProject,
  openDocument,
  openPath,
  parseDocument,
  readBinaryArtifact,
  renameFile,
  disconnectMcpSession,
  revealInFinder,
  reloadPersonaRegistry,
  removeAgentSession,
  renameProject,
  resetActionTemplates,
  sendBackToSession,
  sendMultiToClipboard,
  sendToClipboard,
  sessionAttachmentsForPath,
  setActionTemplates,
  setDefaultActionVerb,
  setCurrentFocus,
  setProjectDefaultAgent,
  setReviewState,
  takePendingOpens,
  targetFileExists,
  trackPathsInInbox,
  togglePin,
  untrackFile,
  updateSidecarComments,
  writeDocument,
  type ActionTemplate,
  type BootstrapInfo,
  type AgentSession,
  type ConflictStrategy,
  type FileMetadata,
  type PersonaRegistry,
  type RecentEntry,
  type SessionAttachment
} from "./ipc";
import type { BaseSnapshot, Block, Comment, CommentAnchor } from "./types/blocks";
import "./styles.css";

type OpenArtifact = {
  path: string;
  source: string;
  baseHash: number[];
  blocks: Block[];
  dirty: boolean;
  kind: ArtifactKind;
  dataUrl?: string;
  size?: number;
  mime?: string;
};

type ArtifactKind = "md" | "html" | "png" | "json" | "txt" | "pdf" | "unsupported";
type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

type FsEventPayload = {
  kind: string;
  path: string | null;
};

type FocusAndOpenPayload = {
  path: string;
};

type NotifyUserPayload = {
  severity: "info" | "warn" | "error";
  message: string;
  action?: {
    label: string;
    artifact_path: string;
  } | null;
};

type TauriDragDropPayload = {
  paths?: string[];
};

type AgentMenu = {
  x: number;
  y: number;
  session: AgentSession;
} | null;

type FileMenu = {
  x: number;
  y: number;
  file: FileMetadata;
} | null;

type ProjectMenu = {
  x: number;
  y: number;
  project: string;
} | null;

type AnnotationSelection = TextAnnotationSelection | HtmlAnnotationSelection | null;

type TextAnnotationSelection = {
  kind: "text";
  rect: DOMRect;
  startOffset: number;
  endOffset: number;
};

type HtmlAnnotationSelection = {
  kind: "html";
  rect: DOMRect;
  startOffset: number;
  endOffset: number;
  text: string;
};

type IframeBridgeMessage = {
  type?: unknown;
  range?: {
    startOffset?: unknown;
    endOffset?: unknown;
    text?: unknown;
  };
  level?: unknown;
  message?: unknown;
  payload?: unknown;
};

type MergeConflict = {
  path: string;
  filename: string;
  draftSource: string;
  baseSnapshot: BaseSnapshot | null;
  diskSource: string;
  diskHash: number[];
} | null;

const ACTION_VERBS = ["Review", "Revise", "Expand", "Critique", "Summarize", "Respond to"] as const;

export default function App() {
  const [bootstrap, setBootstrap] = useState<BootstrapInfo | null>(null);
  const [files, setFiles] = useState<FileMetadata[]>([]);
  const [projects, setProjects] = useState<string[]>([]);
  const [projectCounts, setProjectCounts] = useState<Map<string, number>>(new Map());
  const [mode, setMode] = useState<"inbox" | "drafts" | "project" | "archive" | "pinned" | "recents">("inbox");
  const [currentProject, setCurrentProject] = useState<string | null>(null);
  const [projectFiles, setProjectFiles] = useState<FileMetadata[]>([]);
  const [archiveFiles, setArchiveFiles] = useState<FileMetadata[]>([]);
  const [pinnedFiles, setPinnedFiles] = useState<FileMetadata[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [personas, setPersonas] = useState<PersonaRegistry | null>(null);
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [showSessionForm, setShowSessionForm] = useState(false);
  const [sessionPersona, setSessionPersona] = useState("cto");
  const [sessionBackbone, setSessionBackbone] = useState("claude");
  const [sessionContext, setSessionContext] = useState("AGRC");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [paletteIndex, setPaletteIndex] = useState(0);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const paletteRef = useRef<HTMLElement | null>(null);
  const sourceViewRef = useRef<SourceViewHandle | null>(null);
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [artifact, setArtifact] = useState<OpenArtifact | null>(null);
  const [confirmBeforeRemove, setConfirmBeforeRemoveState] = useState<boolean>(() => {
    try {
      return window.localStorage.getItem("agentcanvas.confirmBeforeRemove") === "true";
    } catch {
      return false;
    }
  });
  const setConfirmBeforeRemove = useCallback((next: boolean) => {
    setConfirmBeforeRemoveState(next);
    try {
      window.localStorage.setItem("agentcanvas.confirmBeforeRemove", next ? "true" : "false");
    } catch {
      // localStorage may be unavailable in some Tauri contexts; preference stays in-memory.
    }
  }, []);
  const toggleConfirmBeforeRemove = useCallback(() => {
    setConfirmBeforeRemove(!confirmBeforeRemove);
    const message = confirmBeforeRemove
      ? "Remove confirmation: off (click × removes immediately)"
      : "Remove confirmation: on";
    setHandoffToast(message);
    window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
  }, [confirmBeforeRemove, setConfirmBeforeRemove]);
  const [editMode, setEditMode] = useState(false);
  const [sourceMode, setSourceMode] = useState(false);
  // Live preview blocks — kept in sync with artifact.source via debounced parseDocument.
  // Falls back to artifact.blocks until the first parse completes.
  const [previewBlocks, setPreviewBlocks] = useState<Block[] | null>(null);
  const latestPreviewSourceRef = useRef<string | null>(null);
  const [jsonViewMode, setJsonViewMode] = useState<"source" | "tree">("source");
  const [conflict, setConflict] = useState(false);
  const [mergeConflict, setMergeConflict] = useState<MergeConflict>(null);
  const [annotationSelection, setAnnotationSelection] = useState<AnnotationSelection>(null);
  const [comments, setComments] = useState<Comment[]>([]);
  const [commentsOpen, setCommentsOpen] = useState(false);
  const fileLevelOpenCount = useMemo(
    () => comments.filter((c) => !c.resolved && c.anchor.kind === "file_level").length,
    [comments]
  );
  const [commentDialog, setCommentDialog] = useState<AnnotationSelection>(null);
  const [fileLevelDialogOpen, setFileLevelDialogOpen] = useState(false);
  const [hoveredCommentId, setHoveredCommentId] = useState<string | null>(null);
  const [actionTemplatesOpen, setActionTemplatesOpen] = useState(false);
  const [actionTemplates, setActionTemplatesState] = useState<ActionTemplate[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [handoffToast, setHandoffToast] = useState<string | null>(null);
  const [handoffToastBody, setHandoffToastBody] = useState<string | null>(null);
  const [handoffToastAction, setHandoffToastAction] = useState<NotifyUserPayload["action"]>(null);
  const [sendPopoverOpen, setSendPopoverOpen] = useState(false);
  const [showAgentPicker, setShowAgentPicker] = useState(false);
  const [agentPickerOpen, setAgentPickerOpen] = useState(false);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [attachedSessions, setAttachedSessions] = useState<SessionAttachment[]>([]);
  const [defaultAgentId, setDefaultAgentId] = useState<string | null>(null);
  const [defaultActionVerb, setDefaultActionVerbState] = useState("Review");
  const [sendActionVerb, setSendActionVerb] = useState("Review");
  const [customActionVerb, setCustomActionVerb] = useState("");
  const [sendNote, setSendNote] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isOpening, setIsOpening] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [arrivedPaths, setArrivedPaths] = useState<Set<string>>(new Set());
  const [draftFiles, setDraftFiles] = useState<FileMetadata[]>([]);
  const [inboxUnread, setInboxUnread] = useState(0);
  const [recents, setRecents] = useState<RecentEntry[]>([]);
  // Track the currently open ephemeral path so we can release its transient watch on close.
  const [ephemeralPath, setEphemeralPath] = useState<string | null>(null);
  const [newFileDialogOpen, setNewFileDialogOpen] = useState(false);
  const [agentMenu, setAgentMenu] = useState<AgentMenu>(null);
  const [fileMenu, setFileMenu] = useState<FileMenu>(null);
  const [projectMenu, setProjectMenu] = useState<ProjectMenu>(null);
  const [renamingProject, setRenamingProject] = useState<string | null>(null);
  const [deletingProject, setDeletingProject] = useState<string | null>(null);
  const [renamingFile, setRenamingFile] = useState<FileMetadata | null>(null);
  const [conflictDialog, setConflictDialog] = useState<{
    filename: string;
    target: string;
    resolve: (strategy: ConflictStrategy) => void;
  } | null>(null);
  const [confirmDialog, setConfirmDialog] = useState<{
    title: string;
    body: string;
    confirmLabel: string;
    destructive: boolean;
    resolve: (ok: boolean) => void;
  } | null>(null);
  const [pendingSendPath, setPendingSendPath] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const currentProjectKey = currentProject ?? "Inbox";
  useFocusTrap(paletteRef, paletteOpen ? () => setPaletteOpen(false) : undefined);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [
        nextBootstrap,
        nextFiles,
        nextDrafts,
        nextProjects,
        nextProjectCounts,
        nextPersonas,
        nextSessions,
        nextDefaultVerb,
        nextActionTemplates,
        nextPinned,
        nextArchive,
        nextUnread,
        nextRecents
      ] = await Promise.all([
        getBootstrapInfo(),
        listInbox(),
        listDrafts(),
        listProjects(),
        listProjectCounts(),
        listPersonas(),
        listAgentSessions(),
        getDefaultActionVerb(),
        getActionTemplates(),
        listPinned(),
        listArchive(),
        inboxUnreadCount(),
        listRecents()
      ]);
      setBootstrap(nextBootstrap);
      setFiles(nextFiles);
      setDraftFiles(nextDrafts);
      setProjects(nextProjects);
      setProjectCounts(nextProjectCounts);
      setPersonas(nextPersonas);
      setSessions(nextSessions);
      setDefaultActionVerbState(nextDefaultVerb);
      setActionTemplatesState(nextActionTemplates);
      setPinnedFiles(nextPinned);
      setArchiveFiles(nextArchive);
      setInboxUnread(nextUnread);
      setRecents(nextRecents);
      setSelectedPath((current) => current ?? nextFiles[0]?.path ?? null);
      setSelectedPaths((current) => current.size > 0 ? current : new Set(nextFiles[0]?.path ? [nextFiles[0].path] : []));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Slice 4: drain the cold-launch pending-opens buffer on mount, then listen for
  // warm open-external events (fired when the app is already running and the user
  // opens a file from Finder/open -a).
  useEffect(() => {
    let disposed = false;

    // Cold-launch path: drain anything buffered before the webview attached.
    void takePendingOpens().then((paths) => {
      if (disposed) return;
      for (const filePath of paths) {
        void openExternalPath(filePath);
      }
    }).catch(() => { /* best-effort */ });

    // Warm path: listen for open-external events emitted by RunEvent::Opened.
    const unlistenOpenExternal = listen<{ path: string }>(
      "agentcanvas://open-external",
      (event) => {
        if (disposed) return;
        void openExternalPath(event.payload.path);
      }
    );

    return () => {
      disposed = true;
      void unlistenOpenExternal.then((dispose) => dispose());
    };
  // openExternalPath is stable across renders (useCallback); omitting from deps is intentional.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (!selectedPath) {
      return;
    }
    void setCurrentFocus(selectedPath).catch((caught) => {
      setError(caught instanceof Error ? caught.message : String(caught));
    });
  }, [selectedPath]);

  const addSession = useCallback(async () => {
    try {
      const session = await addAgentSession({
        persona: sessionPersona,
        backbone: sessionBackbone,
        context: sessionContext
      });
      setSessions((current) => [session, ...current]);
      setShowSessionForm(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [sessionBackbone, sessionContext, sessionPersona]);

  const removeManualSession = useCallback(async (sessionId: string) => {
    try {
      await removeAgentSession(sessionId);
      setSessions((current) => current.filter((session) => session.id !== sessionId));
      if (defaultAgentId === sessionId) {
        setDefaultAgentId(null);
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [defaultAgentId]);

  const disconnectLiveSession = useCallback(async (sessionId: string) => {
    try {
      await disconnectMcpSession(sessionId);
      setSessions((current) => current.filter((session) => session.id !== sessionId));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const selectedFile = useMemo(
    () => [...files, ...draftFiles, ...projectFiles, ...archiveFiles, ...pinnedFiles].find((file) => file.path === selectedPath) ?? null,
    [archiveFiles, draftFiles, files, pinnedFiles, projectFiles, selectedPath]
  );
  const selectedFileMetadatas = useMemo(() => {
    const byPath = new Map([...files, ...draftFiles, ...projectFiles, ...archiveFiles, ...pinnedFiles].map((file) => [file.path, file]));
    return [...selectedPaths].map((path) => byPath.get(path)).filter((file): file is FileMetadata => Boolean(file));
  }, [archiveFiles, draftFiles, files, pinnedFiles, projectFiles, selectedPaths]);
  const multiSelectActive = selectedPaths.size > 1;
  useEffect(() => {
    let disposed = false;
    if (!artifact || multiSelectActive) {
      setAttachedSessions([]);
      return;
    }
    void sessionAttachmentsForPath(artifact.path)
      .then((attachments) => {
        if (!disposed) {
          setAttachedSessions(attachments);
        }
      })
      .catch((caught) => setError(caught instanceof Error ? caught.message : String(caught)));
    return () => {
      disposed = true;
    };
  }, [artifact?.path, multiSelectActive]);
  const filteredFiles = useMemo(() => filterFilesByQuery(files, mode === "inbox" ? searchQuery : ""), [files, mode, searchQuery]);
  const filteredDraftFiles = useMemo(
    () => filterFilesByQuery(draftFiles, mode === "drafts" ? searchQuery : ""),
    [draftFiles, mode, searchQuery]
  );
  const filteredProjectFiles = useMemo(
    () => filterFilesByQuery(projectFiles, mode === "project" ? searchQuery : ""),
    [mode, projectFiles, searchQuery]
  );
  const filteredArchiveFiles = useMemo(
    () => filterFilesByQuery(archiveFiles, mode === "archive" ? searchQuery : ""),
    [archiveFiles, mode, searchQuery]
  );
  const filteredPinnedFiles = useMemo(
    () => filterFilesByQuery(pinnedFiles, mode === "pinned" ? searchQuery : ""),
    [mode, pinnedFiles, searchQuery]
  );
  const visibleFiles = useMemo(() => {
    if (mode === "archive") {
      return filteredArchiveFiles;
    }
    if (mode === "pinned") {
      return filteredPinnedFiles;
    }
    if (mode === "project") {
      return filteredProjectFiles;
    }
    if (mode === "drafts") {
      return filteredDraftFiles;
    }
    if (mode === "recents") {
      // Recents are displayed in the sidebar section directly, not as tracked FileMetadata.
      return [];
    }
    return filteredFiles;
  }, [filteredArchiveFiles, filteredDraftFiles, filteredFiles, filteredPinnedFiles, filteredProjectFiles, mode]);
  const attachedAgentOptions = useMemo(
    () => attachedSessions.map(attachmentToAgentSession),
    [attachedSessions]
  );
  // All live MCP sessions that can receive an artifact send (backend auto-attaches if needed).
  const liveMcpSessions = useMemo(
    () => sessions.filter((session) => session.is_live),
    [sessions]
  );
  // For a single-artifact send: prefer live MCP sessions (they support direct delivery).
  // For multi-select or no artifact: fall back to all sessions (clipboard route).
  const sendRouteSessions = artifact && !multiSelectActive && liveMcpSessions.length > 0
    ? liveMcpSessions
    : sessions;
  // isMcpSend is true when the chosen route will go through sendBackToSession.
  const isMcpSend = artifact && !multiSelectActive && liveMcpSessions.length > 0;
  const sendButtonLabel = useMemo(
    () => sendLabelForSessions(
      sendRouteSessions,
      attachedAgentOptions[0]?.id ?? defaultAgentId,
      multiSelectActive ? selectedPaths.size : undefined,
      Boolean(isMcpSend)
    ),
    [artifact, attachedAgentOptions, defaultAgentId, isMcpSend, multiSelectActive, selectedPaths.size, sendRouteSessions]
  );
  const defaultAgent = useMemo(
    () => sessions.find((session) => session.id === defaultAgentId) ?? null,
    [defaultAgentId, sessions]
  );
  const manualSessions = useMemo(
    () => sessions.filter((session) => session.source === "manual"),
    [sessions]
  );
  const personaColors = useMemo(
    () => new Map((personas?.personas ?? []).map((persona) => [persona.name, persona.color])),
    [personas]
  );
  const parsedJson = useMemo(() => {
    if (artifact?.kind !== "json") {
      return null;
    }
    try {
      return JSON.parse(artifact.source) as JsonValue;
    } catch {
      return null;
    }
  }, [artifact]);

  useEffect(() => {
    let disposed = false;
    void getProjectDefaultAgent(currentProjectKey)
      .then((sessionId) => {
        if (!disposed) {
          setDefaultAgentId(sessionId);
        }
      })
      .catch((caught) => setError(caught instanceof Error ? caught.message : String(caught)));
    return () => {
      disposed = true;
    };
  }, [currentProjectKey]);

  const openArtifact = useCallback(async (file: FileMetadata) => {
    setSelectedPath(file.path);
    setSelectedPaths(new Set([file.path]));
    setIsOpening(true);
    setConflict(false);
    setError(null);
    setSavedAt(null);
    setComments([]);

    try {
      if (pngExtension(file.extension) || pdfExtension(file.extension)) {
        const opened = await readBinaryArtifact(file.path);
        setArtifact({
          path: file.path,
          source: "",
          baseHash: [],
          blocks: [],
          dirty: false,
          kind: opened.kind,
          dataUrl: opened.data_url,
          size: opened.size,
          mime: opened.mime
        });
        setEditMode(false);
        setSourceMode(false);
        setJsonViewMode("source");
        await loadCommentsForPath(file.path, setComments);
        if (file.review_state === "unread") {
          markReviewStateLocally(file.path, "reviewed", setFiles, setProjectFiles, setArchiveFiles, setPinnedFiles);
          setInboxUnread((n) => Math.max(0, n - 1));
          void setReviewState(file.path, "reviewed").catch(() => { /* best-effort */ });
        }
        return;
      }

      if (
        !markdownExtension(file.extension) &&
        !htmlExtension(file.extension) &&
        !jsonExtension(file.extension) &&
        !txtExtension(file.extension)
      ) {
        setArtifact({
          path: file.path,
          source: "",
          baseHash: [],
          blocks: [],
          dirty: false,
          kind: "unsupported"
        });
        setEditMode(false);
        setSourceMode(false);
        setJsonViewMode("source");
        await loadCommentsForPath(file.path, setComments);
        if (file.review_state === "unread") {
          markReviewStateLocally(file.path, "reviewed", setFiles, setProjectFiles, setArchiveFiles, setPinnedFiles);
          setInboxUnread((n) => Math.max(0, n - 1));
          void setReviewState(file.path, "reviewed").catch(() => { /* best-effort */ });
        }
        return;
      }

      const opened = await openDocument(file.path);
      const kind: ArtifactKind = markdownExtension(file.extension)
        ? "md"
        : htmlExtension(file.extension)
          ? "html"
          : jsonExtension(file.extension)
            ? "json"
            : "txt";
      const blocks = kind === "md" ? await parseDocument(opened.source) : [];
      setArtifact({
        path: opened.path,
        source: opened.source,
        baseHash: opened.base_hash,
        blocks,
        dirty: false,
        kind
      });
      setEditMode(false);
      setSourceMode(false);
      if (kind === "json") {
        setJsonViewMode(jsonParses(opened.source) ? "tree" : "source");
      } else {
        setJsonViewMode("source");
      }
      await loadCommentsForPath(opened.path, setComments);
      if (file.review_state === "unread") {
        markReviewStateLocally(file.path, "reviewed", setFiles, setProjectFiles, setArchiveFiles, setPinnedFiles);
        setInboxUnread((n) => Math.max(0, n - 1));
        void setReviewState(file.path, "reviewed").catch(() => { /* best-effort */ });
      }
    } catch (caught) {
      // Even with extension check, openDocument can fail. Clear stale artifact so the
      // viewer doesn't continue showing the previously-opened file.
      setArtifact({
        path: file.path,
        source: "",
        baseHash: [],
        blocks: [],
        dirty: false,
        kind: "unsupported"
      });
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsOpening(false);
    }
  }, []);

  const openProject = useCallback(
    async (project: string) => {
      try {
        const nextFiles = await listProjectFiles(project);
        setMode("project");
        setCurrentProject(project);
        setSearchQuery("");
        setProjectFiles(nextFiles);
        if (nextFiles[0]) {
          await openArtifact(nextFiles[0]);
        } else {
          setArtifact(null);
          setSelectedPath(null);
          setSelectedPaths(new Set());
        }
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [openArtifact]
  );

  const openArchive = useCallback(async () => {
    try {
      const nextFiles = await listArchive();
      setMode("archive");
      setCurrentProject(null);
      setSearchQuery("");
      setArchiveFiles(nextFiles);
      if (nextFiles[0]) {
        await openArtifact(nextFiles[0]);
      } else {
        setArtifact(null);
        setSelectedPath(null);
        setSelectedPaths(new Set());
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [openArtifact]);

  const openPinned = useCallback(async () => {
    try {
      const nextFiles = await listPinned();
      setMode("pinned");
      setCurrentProject(null);
      setSearchQuery("");
      setPinnedFiles(nextFiles);
      if (nextFiles[0]) {
        await openArtifact(nextFiles[0]);
      } else {
        setArtifact(null);
        setSelectedPath(null);
        setSelectedPaths(new Set());
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [openArtifact]);

  const openInbox = useCallback(() => {
    setMode("inbox");
    setCurrentProject(null);
    setSearchQuery("");
  }, []);

  const openDrafts = useCallback(async () => {
    try {
      const nextFiles = await listDrafts();
      setMode("drafts");
      setCurrentProject(null);
      setSearchQuery("");
      setDraftFiles(nextFiles);
      if (nextFiles[0]) {
        await openArtifact(nextFiles[0]);
      } else {
        setArtifact(null);
        setSelectedPath(null);
        setSelectedPaths(new Set());
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [openArtifact]);

  // Open a file by absolute path (Slice 4+5). Routes through open_path which returns
  // tracked or ephemeral mode. For tracked files, locates the FileMetadata from existing
  // lists and opens via openArtifact (which handles review state). For ephemeral files,
  // synthesises a minimal FileMetadata (no review_state tracking) and opens directly.
  const openExternalPath = useCallback(async (filePath: string) => {
    try {
      const result = await openPath(filePath);

      if (result.mode === "tracked") {
        // Find the file in tracked lists and open it normally.
        await refresh();
        // After refresh, look up the file in all lists. openArtifact will handle
        // review state and unread marks.
        const allFiles = await Promise.all([
          listInbox(),
          listDrafts(),
          listPinned(),
          listArchive()
        ]);
        const flat = allFiles.flat();
        const found = flat.find((f) => f.path === result.path);
        if (found) {
          // Navigate to the right section.
          const inInbox = allFiles[0].some((f) => f.path === result.path);
          const inDrafts = allFiles[1].some((f) => f.path === result.path);
          const inPinned = allFiles[2].some((f) => f.path === result.path);
          const inArchive = allFiles[3].some((f) => f.path === result.path);
          if (inDrafts) {
            setMode("drafts");
            setCurrentProject(null);
            setDraftFiles(allFiles[1]);
          } else if (inPinned) {
            setMode("pinned");
            setCurrentProject(null);
            setPinnedFiles(allFiles[2]);
          } else if (inArchive) {
            setMode("archive");
            setCurrentProject(null);
            setArchiveFiles(allFiles[3]);
          } else if (inInbox) {
            setMode("inbox");
            setCurrentProject(null);
            setFiles(allFiles[0]);
          }
          await openArtifact(found);
        }
      } else {
        // Ephemeral: release any previous ephemeral watch, then open the new one.
        if (ephemeralPath && ephemeralPath !== result.path) {
          void closeEphemeralPath(ephemeralPath).catch(() => { /* best-effort */ });
        }
        setEphemeralPath(result.path);
        setMode("recents");
        setCurrentProject(null);
        // Refresh recents so the new entry appears.
        listRecents().then(setRecents).catch(() => { /* best-effort */ });

        // Synthesise a FileMetadata-compatible object for openArtifact.
        const syntheticFile: FileMetadata = {
          path: result.path,
          relative_path: result.relative_path,
          name: result.name,
          extension: result.extension,
          size: result.size,
          mtime: result.mtime,
          last_seen_hash: result.base_hash,
          pinned: false,
          archived: false,
          last_read_at: null,
          persona: "claude",
          review_state: "reviewed",
          comment_count: 0
        };
        await openArtifact(syntheticFile);
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [ephemeralPath, openArtifact, refresh]);

  const createNewFile = useCallback(async (name: string) => {
    try {
      const path = await createMyFile(name);
      // Refresh drafts list so the file appears.
      const nextDrafts = await listDrafts();
      setDraftFiles(nextDrafts);
      setMode("drafts");
      setCurrentProject(null);
      // Find and open the new file in edit mode.
      const newFile = nextDrafts.find((f) => f.path === path);
      if (newFile) {
        await openArtifact(newFile);
        setEditMode(true);
      }
      setNewFileDialogOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [openArtifact]);

  const selectFileFromList = useCallback(
    async (file: FileMetadata, list: FileMetadata[], event: ReactMouseEvent<HTMLButtonElement>) => {
      if (event.shiftKey) {
        event.preventDefault();
        const anchorIndex = list.findIndex((candidate) => candidate.path === selectedPath);
        const clickedIndex = list.findIndex((candidate) => candidate.path === file.path);
        if (clickedIndex === -1) {
          return;
        }
        const start = anchorIndex === -1 ? clickedIndex : Math.min(anchorIndex, clickedIndex);
        const end = anchorIndex === -1 ? clickedIndex : Math.max(anchorIndex, clickedIndex);
        const range = list.slice(start, end + 1).map((candidate) => candidate.path);
        setSelectedPath(file.path);
        setSelectedPaths((current) => new Set([...current, ...range]));
        return;
      }

      if (event.metaKey || event.ctrlKey) {
        event.preventDefault();
        setSelectedPath(file.path);
        setSelectedPaths((current) => {
          const next = new Set(current);
          if (next.has(file.path)) {
            next.delete(file.path);
          } else {
            next.add(file.path);
          }
          return next;
        });
        return;
      }

      await openArtifact(file);
    },
    [openArtifact, selectedPath]
  );

  const reloadOpenArtifact = useCallback(async () => {
    if (!artifact || artifact.dirty) {
      return;
    }

    try {
      if (artifact.kind === "png" || artifact.kind === "pdf") {
        const opened = await readBinaryArtifact(artifact.path);
        setArtifact({
          ...artifact,
          kind: opened.kind,
          dataUrl: opened.data_url,
          size: opened.size,
          mime: opened.mime,
          dirty: false
        });
        return;
      }

      const opened = await openDocument(artifact.path);
      const blocks = artifact.kind === "md" ? await parseDocument(opened.source) : [];
      setArtifact({
        ...artifact,
        source: opened.source,
        baseHash: opened.base_hash,
        blocks,
        dirty: false
      });
      if (artifact.kind === "json") {
        setJsonViewMode(jsonParses(opened.source) ? "tree" : "source");
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact]);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<FsEventPayload>("agentcanvas://fs-event", (event) => {
      if (disposed) {
        return;
      }
      const path = event.payload.path;
      if (path && event.payload.kind === "created") {
        setArrivedPaths((current) => new Set([...current, path]));
        window.setTimeout(() => {
          setArrivedPaths((current) => {
            const next = new Set(current);
            next.delete(path);
            return next;
          });
        }, 2500);
      }
      void refresh();
      void reloadOpenArtifact();
    });

    return () => {
      disposed = true;
      void unlisten.then((dispose) => dispose());
    };
  }, [refresh, reloadOpenArtifact]);

  useEffect(() => {
    let disposed = false;
    const unlistenFocus = listen<FocusAndOpenPayload>("agentcanvas://focus-and-open", async (event) => {
      if (disposed) {
        return;
      }
      const path = event.payload.path;
      setArrivedPaths((current) => new Set([...current, path]));
      window.setTimeout(() => {
        setArrivedPaths((current) => {
          const next = new Set(current);
          next.delete(path);
          return next;
        });
      }, 2500);
      const [nextFiles, nextDrafts, nextPinned, nextArchive, nextProjects, nextUnread] = await Promise.all([
        listInbox(),
        listDrafts(),
        listPinned(),
        listArchive(),
        listProjects(),
        inboxUnreadCount()
      ]);
      const nextProjectEntries = await Promise.all(
        nextProjects.map(async (project) => [project, await listProjectFiles(project)] as const)
      );
      if (disposed) {
        return;
      }
      setFiles(nextFiles);
      setDraftFiles(nextDrafts);
      setPinnedFiles(nextPinned);
      setArchiveFiles(nextArchive);
      setInboxUnread(nextUnread);
      const projectEntry = nextProjectEntries.find(([, projectFiles]) =>
        projectFiles.some((candidate) => candidate.path === path)
      );
      const file =
        nextFiles.find((candidate) => candidate.path === path) ??
        nextDrafts.find((candidate) => candidate.path === path) ??
        projectEntry?.[1].find((candidate) => candidate.path === path) ??
        nextPinned.find((candidate) => candidate.path === path) ??
        nextArchive.find((candidate) => candidate.path === path);
      if (nextDrafts.some((candidate) => candidate.path === path)) {
        setMode("drafts");
        setCurrentProject(null);
      } else if (projectEntry) {
        setMode("project");
        setCurrentProject(projectEntry[0]);
        setProjectFiles(projectEntry[1]);
      } else if (nextPinned.some((candidate) => candidate.path === path)) {
        setMode("pinned");
        setCurrentProject(null);
      } else if (nextArchive.some((candidate) => candidate.path === path)) {
        setMode("archive");
        setCurrentProject(null);
      } else {
        setMode("inbox");
        setCurrentProject(null);
      }
      if (file) {
        await openArtifact(file);
      }
      window.focus();
    });
    const unlistenComments = listen<FocusAndOpenPayload>("agentcanvas://comments-changed", (event) => {
      if (disposed || artifact?.path !== event.payload.path) {
        return;
      }
      void loadCommentsForPath(event.payload.path, setComments);
      setCommentsOpen(true);
    });
    return () => {
      disposed = true;
      void unlistenFocus.then((dispose) => dispose());
      void unlistenComments.then((dispose) => dispose());
    };
  }, [artifact?.path, openArtifact]);

  // notify-user toast: kept in a stable effect (no artifact dep) so the listener
  // is never torn down while a notification is in-flight on artifact change.
  useEffect(() => {
    const unlistenNotify = listen<NotifyUserPayload>("agentcanvas://notify-user", (event) => {
      setHandoffToast(event.payload.message);
      setHandoffToastBody(event.payload.severity === "info" ? null : event.payload.severity.toUpperCase());
      setHandoffToastAction(event.payload.action ?? null);
      window.setTimeout(() => {
        setHandoffToast((current) => (current === event.payload.message ? null : current));
        setHandoffToastBody(null);
        setHandoffToastAction(null);
      }, 4500);
    });
    return () => {
      void unlistenNotify.then((dispose) => dispose());
    };
  }, []); // no deps — listener is stable for the lifetime of the app

  // sessions-changed: refresh sessions list when an agent connects or disconnects,
  // so the panel and send picker stay in sync without waiting for window focus.
  useEffect(() => {
    const unlistenSessionsChanged = listen("agentcanvas://sessions-changed", () => {
      void listAgentSessions()
        .then(setSessions)
        .catch(() => { /* best-effort */ });
    });
    return () => {
      void unlistenSessionsChanged.then((dispose) => dispose());
    };
  }, []); // no deps — stable for app lifetime

  useEffect(() => {
    function handleFocus() {
      void refresh();
      void reloadOpenArtifact();
    }

    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [refresh, reloadOpenArtifact]);

  useEffect(() => {
    function selectionFromIframeRange(range: NonNullable<IframeBridgeMessage["range"]>): HtmlAnnotationSelection | null {
      if (
        typeof range.startOffset !== "number" ||
        typeof range.endOffset !== "number" ||
        typeof range.text !== "string" ||
        range.text.length === 0
      ) {
        return null;
      }
      return {
        kind: "html",
        rect: new DOMRect(window.innerWidth / 2, 96, 1, 1),
        startOffset: range.startOffset,
        endOffset: range.endOffset,
        text: range.text
      };
    }

    function handleIframeMessage(event: MessageEvent<IframeBridgeMessage>) {
      if (!iframeRef.current?.contentWindow || event.source !== iframeRef.current.contentWindow) {
        return;
      }
      const message = event.data;
      if (!message || typeof message.type !== "string" || !message.type.startsWith("agentcanvas:")) {
        return;
      }
      if (message.type === "agentcanvas:selection") {
        const selection = message.range ? selectionFromIframeRange(message.range) : null;
        if (!selection) {
          return;
        }
        setAnnotationSelection(selection);
        return;
      }
      if (message.type === "agentcanvas:comment_shortcut") {
        const selection = message.range ? selectionFromIframeRange(message.range) : null;
        if (!selection) {
          return;
        }
        setAnnotationSelection(selection);
        setCommentDialog(selection);
        return;
      }
      if (message.type === "agentcanvas:console") {
        const level = typeof message.level === "string" ? message.level : "log";
        const text = typeof message.message === "string" ? message.message : String(message.message ?? "");
        setError(`[iframe ${level}] ${text}`);
        return;
      }
      if (message.type === "agentcanvas:send_back") {
        console.info("AgentCanvas iframe send-back received", message.payload);
        const toast = "Send-back received";
        setHandoffToast(toast);
        setHandoffToastBody("Slice 6 will wire this to agent sessions.");
        window.setTimeout(() => {
          setHandoffToast((current) => (current === toast ? null : current));
          setHandoffToastBody(null);
        }, 3000);
      }
    }

    window.addEventListener("message", handleIframeMessage);
    return () => window.removeEventListener("message", handleIframeMessage);
  }, []);

  // Tauri 2 macOS WebView shows the native context menu by default, which
  // overlays React's custom menu. Suppress globally for non-text-input targets
  // so onContextMenu handlers on file rows + agent cards actually render.
  useEffect(() => {
    function handleContextMenu(event: MouseEvent) {
      if (isTextInput(event.target)) {
        return; // allow native menu on inputs/textareas for copy/paste UX
      }
      event.preventDefault();
    }
    document.addEventListener("contextmenu", handleContextMenu);
    return () => document.removeEventListener("contextmenu", handleContextMenu);
  }, []);

  const saveArtifact = useCallback(async () => {
    if (!artifact) {
      return;
    }
    setIsSaving(true);
    setConflict(false);
    setError(null);
    try {
      if (!isEditableArtifact(artifact.kind)) {
        return;
      }
      const result = await writeDocument(artifact.path, artifact.source, artifact.baseHash);
      const blocks = artifact.kind === "md" ? await parseDocument(artifact.source) : [];
      setArtifact({ ...artifact, baseHash: result.new_hash, blocks, dirty: false });
      setMergeConflict(null);
      const stamp = currentTime();
      setSavedAt(stamp);
      window.setTimeout(() => setSavedAt((current) => (current === stamp ? null : current)), 3000);
      await refresh();
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      if (message.startsWith("CONFLICT:")) {
        setConflict(true);
        try {
          const [sidecar, disk] = await Promise.all([
            loadSidecar(artifact.path).catch(() => null),
            openDocument(artifact.path)
          ]);
          setMergeConflict({
            path: artifact.path,
            filename: fileName(artifact.path),
            draftSource: artifact.source,
            baseSnapshot: sidecar?.base_snapshot ?? null,
            diskSource: disk.source,
            diskHash: disk.base_hash
          });
        } catch (conflictCaught) {
          setError(conflictCaught instanceof Error ? conflictCaught.message : String(conflictCaught));
        }
      } else {
        setError(message);
      }
    } finally {
      setIsSaving(false);
    }
  }, [artifact, refresh]);

  function updateSource(next: string) {
    setArtifact((current) => (current ? { ...current, source: next, dirty: true } : current));
    setConflict(false);
    setSavedAt(null);
  }

  // Reset previewBlocks when the open file changes so we don't flash a previous
  // file's blocks while the new parse is in-flight.
  useEffect(() => {
    setPreviewBlocks(null);
    latestPreviewSourceRef.current = null;
  }, [artifact?.path]);

  // Debounced live-preview: re-parse artifact.source ~200 ms after each keystroke.
  // Stale-guard: latestPreviewSourceRef tracks the most-recently-requested source so
  // out-of-order async responses are discarded.
  useEffect(() => {
    if (!artifact || artifact.kind !== "md") {
      return;
    }
    const source = artifact.source;
    const timer = window.setTimeout(async () => {
      latestPreviewSourceRef.current = source;
      try {
        const blocks = await parseDocument(source);
        // Discard if a newer request has already fired.
        if (latestPreviewSourceRef.current === source) {
          setPreviewBlocks(blocks);
        }
      } catch {
        // Parse errors are non-fatal; leave previewBlocks unchanged.
      }
    }, 200);
    return () => window.clearTimeout(timer);
  }, [artifact?.source, artifact?.kind]);

  const applyAnnotationFormat = useCallback((format: SourceFormat) => {
    sourceViewRef.current?.applyFormat(format);
  }, []);

  const openCommentDialog = useCallback((selection: NonNullable<AnnotationSelection>) => {
    setCommentDialog(selection);
  }, []);

  const saveComment = useCallback(async (body: string) => {
    if (!artifact || !commentDialog) {
      return;
    }
    const anchor = commentDialog.kind === "html"
      ? {
        kind: "html_selection" as const,
        start_offset: commentDialog.startOffset,
        end_offset: commentDialog.endOffset,
        snapshot_text: commentDialog.text
      }
      : {
        block_id: null,
        start_offset: commentDialog.startOffset,
        end_offset: commentDialog.endOffset
      };
    const nextComments = [
      ...comments,
      {
        id: crypto.randomUUID(),
        author: "jesse",
        created_at: Math.floor(Date.now() / 1000),
        anchor,
        body,
        resolved: false
      }
    ];
    try {
      await updateSidecarComments(artifact.path, nextComments);
      setComments(nextComments);
      setCommentsOpen(true);
      setCommentDialog(null);
      setAnnotationSelection(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact, commentDialog, comments]);

  const saveFileLevelComment = useCallback(async (body: string) => {
    if (!artifact) {
      return;
    }
    const anchor: CommentAnchor = { kind: "file_level" };
    const nextComments = [
      ...comments,
      {
        id: crypto.randomUUID(),
        author: "jesse",
        created_at: Math.floor(Date.now() / 1000),
        anchor,
        body,
        resolved: false
      }
    ];
    try {
      await updateSidecarComments(artifact.path, nextComments);
      setComments(nextComments);
      setCommentsOpen(true);
      setFileLevelDialogOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact, comments]);

  const resolveComment = useCallback(async (commentId: string) => {
    if (!artifact) {
      return;
    }
    const nextComments = comments.map((comment) =>
      comment.id === commentId ? { ...comment, resolved: true } : comment
    );
    try {
      await updateSidecarComments(artifact.path, nextComments);
      setComments(nextComments);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact, comments]);

  const revealComment = useCallback((comment: Comment) => {
    if (comment.anchor.kind === "file_level") {
      setHoveredCommentId(comment.id);
      return;
    }
    if (artifact?.kind === "html" && comment.anchor.kind === "html_selection") {
      iframeRef.current?.contentWindow?.postMessage(
        { type: "agentcanvas:scroll_to", text: comment.anchor.snapshot_text },
        "*"
      );
      return;
    }
    if (comment.anchor.kind === "html_selection") {
      return;
    }
    sourceViewRef.current?.revealRange(comment.anchor.start_offset, comment.anchor.end_offset);
  }, [artifact?.kind]);

  const keepMineFromMerge = useCallback(async () => {
    if (!mergeConflict || !artifact) {
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      const result = await writeDocument(mergeConflict.path, mergeConflict.draftSource, mergeConflict.diskHash);
      const blocks = artifact.kind === "md" ? await parseDocument(mergeConflict.draftSource) : [];
      setArtifact({
        ...artifact,
        source: mergeConflict.draftSource,
        baseHash: result.new_hash,
        blocks,
        dirty: false
      });
      setConflict(false);
      setMergeConflict(null);
      await refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsSaving(false);
    }
  }, [artifact, mergeConflict, refresh]);

  const keepTheirsFromMerge = useCallback(async () => {
    if (!mergeConflict || !artifact) {
      return;
    }
    const blocks = artifact.kind === "md" ? await parseDocument(mergeConflict.diskSource) : [];
    setArtifact({
      ...artifact,
      source: mergeConflict.diskSource,
      baseHash: mergeConflict.diskHash,
      blocks,
      dirty: false
    });
    setConflict(false);
    setMergeConflict(null);
  }, [artifact, mergeConflict]);

  const cancelMergeAndReload = useCallback(async () => {
    if (!mergeConflict || !artifact) {
      return;
    }
    const disk = await openDocument(mergeConflict.path);
    const blocks = artifact.kind === "md" ? await parseDocument(disk.source) : [];
    setArtifact({
      ...artifact,
      source: disk.source,
      baseHash: disk.base_hash,
      blocks,
      dirty: false
    });
    setConflict(false);
    setMergeConflict(null);
  }, [artifact, mergeConflict]);

  const openSendPopover = useCallback((forceAgentPicker = false) => {
    if (!artifact && selectedPaths.size <= 1) {
      return;
    }
    // Refresh sessions and attachments so the picker reflects reality at the moment
    // the popover opens (agents may have connected or disconnected since last refresh).
    void Promise.all([
      listAgentSessions().then(setSessions).catch(() => { /* best-effort */ }),
      artifact && !multiSelectActive
        ? sessionAttachmentsForPath(artifact.path).then(setAttachedSessions).catch(() => { /* best-effort */ })
        : Promise.resolve()
    ]);
    const defaultIsPreset = ACTION_VERBS.includes(defaultActionVerb as (typeof ACTION_VERBS)[number]);
    // Compute route using the current (pre-refresh) sessions; the picker will re-render
    // when the state updates from the refresh above.
    const currentLiveMcp = sessions.filter((s) => s.is_live);
    const routeSessions = artifact && selectedPaths.size <= 1 && currentLiveMcp.length > 0
      ? currentLiveMcp
      : sessions;
    const defaultSession = routeSessions.find((session) => session.id === defaultAgentId) ?? routeSessions[0] ?? null;
    const nextSelectedAgent = defaultSession?.id ?? (routeSessions.length === 1 ? routeSessions[0]?.id ?? null : null);
    setSelectedAgentId(nextSelectedAgent);
    setShowAgentPicker(routeSessions.length > 0 && (forceAgentPicker || routeSessions.length > 1));
    setSendActionVerb(defaultIsPreset ? defaultActionVerb : "Custom");
    setCustomActionVerb(defaultIsPreset ? "" : defaultActionVerb);
    setSendNote("");
    setSendPopoverOpen(true);
  }, [artifact, defaultActionVerb, defaultAgentId, multiSelectActive, selectedPaths.size, sessions]);

  const sendCurrentArtifact = useCallback(async (actionVerb: string, note: string) => {
    if (!artifact && selectedPaths.size <= 1) {
      return;
    }
    const verb = actionVerb.trim() || "Review";
    try {
      const currentLiveMcp = sessions.filter((s) => s.is_live);
      const routeSessions = artifact && selectedPaths.size <= 1 && currentLiveMcp.length > 0
        ? currentLiveMcp
        : sessions;
      const routeDefaultAgent = routeSessions.find((session) => session.id === selectedAgentId) ?? routeSessions[0] ?? defaultAgent;
      if (routeSessions.length > 1) {
        const targetAgent = routeSessions.find((session) => session.id === selectedAgentId) ?? routeDefaultAgent;
        if (!targetAgent) {
          setShowAgentPicker(true);
          return;
        }
      }
      const agent = routeSessions.find((session) => session.id === selectedAgentId) ?? routeDefaultAgent;
      if (selectedPaths.size > 1) {
        const payloads = await Promise.all(
          [...selectedPaths].map(async (path) => {
            const doc = await openDocument(path);
            return {
              path,
              contents: doc.source,
              note: note.trim() ? note : null,
              action_verb: verb
            };
          })
        );
        await sendMultiToClipboard(payloads);
        setSelectedPaths(selectedPath ? new Set([selectedPath]) : new Set());
        const message = `Copied ${payloads.length} files to clipboard for ${agent ? agentSessionLabel(agent) : "Agent"}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3500);
      } else if (artifact) {
        // Route to sendBackToSession for any live MCP session (backend auto-attaches if needed).
        // Fall back to clipboard for manual sessions or when no live sessions are available.
        if (agent?.is_live) {
          await sendBackToSession(artifact.path, agent.id, note.trim() ? note : null, verb);
          const message = `Sent back to ${agentSessionLabel(agent)}`;
          setHandoffToast(message);
          window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3500);
        } else {
          await sendToClipboard({
            path: artifact.path,
            contents: artifact.source,
            note: note.trim() ? note : null,
            action_verb: verb
          });
          const message = "Copied to clipboard — paste into your Claude / Codex session";
          setHandoffToast(message);
          window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3500);
        }
      }
      if (verb === "Revise" || verb === "Critique") {
        const pathsToMark = selectedPaths.size > 1 ? [...selectedPaths] : artifact ? [artifact.path] : [];
        await Promise.all(pathsToMark.map((path) => setReviewState(path, "needs-work")));
        pathsToMark.forEach((path) =>
          markReviewStateLocally(path, "needs-work", setFiles, setProjectFiles, setArchiveFiles, setPinnedFiles)
        );
      }
      await setDefaultActionVerb(verb);
      setDefaultActionVerbState(verb);
      setSendPopoverOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact, defaultAgent, selectedAgentId, selectedPath, selectedPaths, sessions]);

  const setDefaultAgentForProject = useCallback(
    async (session: AgentSession) => {
      try {
        await setProjectDefaultAgent(currentProjectKey, session.id);
        setDefaultAgentId(session.id);
        setAgentMenu(null);
        const message = `Default agent set to ${agentSessionLabel(session)} for ${currentProjectKey}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3000);
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [currentProjectKey]
  );

  const switchAgentDefault = useCallback(async () => {
    if (manualSessions.length === 0) {
      setShowSessionForm(true);
      return;
    }
    setAgentPickerOpen(true);
  }, [manualSessions.length]);

  const installMcpClient = useCallback(async (client: "Claude Code" | "Codex" | "Cursor") => {
    try {
      const result = client === "Claude Code"
        ? await installMcpForClaudeCode()
        : client === "Codex"
          ? await installMcpForCodex()
          : await installMcpForCursor();
      const action = result.action === "noop" ? "already configured" : result.action;
      const message = `${client} MCP ${action}`;
      setHandoffToast(message);
      setHandoffToastBody(result.config_path);
      window.setTimeout(() => {
        setHandoffToast((current) => (current === message ? null : current));
        setHandoffToastBody((current) => (current === result.config_path ? null : current));
      }, 3500);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const openConflictDialog = useCallback(
    (filename: string, target: string): Promise<ConflictStrategy> => {
      return new Promise((resolve) => {
        setConflictDialog({ filename, target, resolve });
      });
    },
    []
  );

  const openConfirmDialog = useCallback(
    (opts: { title: string; body: string; confirmLabel?: string; destructive?: boolean }): Promise<boolean> => {
      return new Promise((resolve) => {
        setConfirmDialog({
          title: opts.title,
          body: opts.body,
          confirmLabel: opts.confirmLabel ?? "Confirm",
          destructive: opts.destructive ?? false,
          resolve
        });
      });
    },
    []
  );

  const renameSelectedFile = useCallback(
    async (newName: string) => {
      if (!renamingFile) return;
      try {
        const updated = await renameFile(renamingFile.path, newName);
        setRenamingFile(null);
        if (artifact?.path === renamingFile.path) {
          setArtifact((current) => (current ? { ...current, path: updated.path } : current));
        }
        if (selectedPath === renamingFile.path) {
          setSelectedPath(updated.path);
        }
        setSelectedPaths((current) => {
          if (!current.has(renamingFile.path)) {
            return current;
          }
          const next = new Set(current);
          next.delete(renamingFile.path);
          next.add(updated.path);
          return next;
        });
        const message = `Renamed → ${updated.name}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact, refresh, renamingFile, selectedPath]
  );

  const toggleCurrentPin = useCallback(async () => {
    if (!artifact) {
      return;
    }
    await togglePin(artifact.path);
    await refresh();
  }, [artifact, refresh]);

  const archiveCurrent = useCallback(async () => {
    if (!artifact) {
      return;
    }
    await archiveFile(artifact.path);
    setArtifact(null);
    setSelectedPath(null);
    setSelectedPaths(new Set());
    await refresh();
  }, [artifact, refresh]);

  const ingestDroppedPaths = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) {
        return;
      }
      try {
        const tracked = await trackPathsInInbox(paths);
        setArrivedPaths((current) => new Set([...current, ...tracked.map((file) => file.path)]));
        const first = tracked[0];
        if (first) {
          const message = `+ ${first.name}`;
          setHandoffToast(message);
          window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        }
        await refresh();
        // Auto-select + open the first dropped file so the content pane updates
        // without an extra click. If multiple files dropped, collapse selection
        // onto the first; the user can still ⌘-click to multi-select.
        if (first) {
          setMode("inbox");
          setCurrentProject(null);
          void openArtifact(first);
        }
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [refresh, openArtifact]
  );

  const openFileDialog = useCallback(async () => {
    try {
      const selection = await open({ multiple: true, directory: false });
      const paths = Array.isArray(selection) ? selection : selection ? [selection] : [];
      await ingestDroppedPaths(paths);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [ingestDroppedPaths]);

  const moveInboxFileToProject = useCallback(
    async (path: string, project: string) => {
      const file = files.find((candidate) => candidate.path === path);
      if (!file) {
        return;
      }
      try {
        const strategy = await conflictStrategyForTarget("project", file.name, project, openConflictDialog);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToProject(path, project, strategy);
        const message = `Tagged ${moved.name} → ${project}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        setArtifact(null);
        setSelectedPath(null);
        setSelectedPaths(new Set());
        await refresh();
        if (currentProject === project) {
          setProjectFiles(await listProjectFiles(project));
        }
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [currentProject, files, refresh]
  );

  const moveKnownFileToProject = useCallback(
    async (file: FileMetadata, project: string) => {
      try {
        const strategy = await conflictStrategyForTarget("project", file.name, project, openConflictDialog);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToProject(file.path, project, strategy);
        const message = `Tagged ${moved.name} → ${project}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        if (artifact?.path === file.path) {
          setArtifact(null);
          setSelectedPath(null);
          setSelectedPaths(new Set());
        }
        setFileMenu(null);
        await refresh();
        if (currentProject === project) {
          setProjectFiles(await listProjectFiles(project));
        }
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact?.path, currentProject, refresh]
  );

  const moveInboxFileToArchive = useCallback(
    async (path: string) => {
      const file = files.find((candidate) => candidate.path === path);
      if (!file) {
        return;
      }
      try {
        const strategy = await conflictStrategyForTarget("archive", file.name, undefined, openConflictDialog);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToArchive(path, strategy);
        const message = `Tagged ${moved.name} → Archive`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        setArtifact(null);
        setSelectedPath(null);
        setSelectedPaths(new Set());
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [files, refresh]
  );

  const moveKnownFileToArchive = useCallback(
    async (file: FileMetadata) => {
      try {
        const strategy = await conflictStrategyForTarget("archive", file.name, undefined, openConflictDialog);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToArchive(file.path, strategy);
        const message = `Tagged ${moved.name} → Archive`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        setArtifact(null);
        setSelectedPath(null);
        setSelectedPaths(new Set());
        setFileMenu(null);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact?.path, refresh]
  );

  const archiveSelectedFiles = useCallback(async () => {
    if (selectedPaths.size === 0) {
      return;
    }
    try {
      const paths = [...selectedPaths];
      await Promise.all(paths.map((path) => moveFileToArchive(path, "keep_both")));
      setArtifact(null);
      setSelectedPath(null);
      setSelectedPaths(new Set());
      const message = `Archived ${paths.length} files`;
      setHandoffToast(message);
      window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
      await refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [refresh, selectedPaths]);

  const exportKnownFile = useCallback(async (file: FileMetadata) => {
    try {
      setFileMenu(null);
      const targetPath = await save({ defaultPath: file.name });
      if (!targetPath) {
        return;
      }
      await exportFileTo(file.path, targetPath);
      const message = `Exported ${file.name} → ${directoryName(targetPath)}`;
      setHandoffToast(message);
      window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3000);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const removeFromCanvas = useCallback(
    async (file: FileMetadata) => {
      if (confirmBeforeRemove) {
        const ok = await openConfirmDialog({
          title: `Remove ${file.name}?`,
          body:
            `This removes the file from AgentCanvas tracking. The file at ${file.path} stays on disk.`,
          confirmLabel: "Remove",
          destructive: false
        });
        if (!ok) return;
      }
      try {
        await untrackFile(file.path);
        if (artifact?.path === file.path) {
          setArtifact(null);
          setSelectedPath(null);
          setSelectedPaths(new Set());
        }
        setFileMenu(null);
        const message = `Removed ${file.name}`;
        const body = `Untracked. File at ${file.path} still on disk.`;
        setHandoffToast(message);
        setHandoffToastBody(body);
        window.setTimeout(() => {
          setHandoffToast((current) => (current === message ? null : current));
          setHandoffToastBody((current) => (current === body ? null : current));
        }, 2500);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact?.path, refresh, openConfirmDialog, confirmBeforeRemove]
  );

  const deleteArtifact = useCallback(
    async (file: FileMetadata) => {
      const ok = await openConfirmDialog({
        title: `Delete ${file.name} from disk?`,
        body: `This permanently deletes ${file.path} from disk and removes it from AgentCanvas.`,
        confirmLabel: "Delete from disk",
        destructive: true
      });
      if (!ok) return;
      try {
        await deleteFileFromDisk(file.path);
        if (artifact?.path === file.path) {
          setArtifact(null);
          setSelectedPath(null);
          setSelectedPaths(new Set());
        }
        setFileMenu(null);
        const message = `Removed ${file.name}`;
        setHandoffToast(message);
        setHandoffToastBody(null);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact?.path, refresh, openConfirmDialog]
  );

  const markFileReviewState = useCallback(async (file: FileMetadata, reviewState: FileMetadata["review_state"]) => {
    try {
      await setReviewState(file.path, reviewState);
      markReviewStateLocally(file.path, reviewState, setFiles, setProjectFiles, setArchiveFiles, setPinnedFiles);
      setFileMenu(null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const reloadPersonas = useCallback(async () => {
    try {
      const nextPersonas = await reloadPersonaRegistry();
      setPersonas(nextPersonas);
      const message = "Persona registry reloaded";
      setHandoffToast(message);
      window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const saveActionTemplates = useCallback(async (templates: ActionTemplate[]) => {
    try {
      await setActionTemplates(templates);
      setActionTemplatesState(templates);
      setActionTemplatesOpen(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const resetActionTemplatesToDefaults = useCallback(async () => {
    try {
      const templates = await resetActionTemplates();
      setActionTemplatesState(templates);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, []);

  const submitProjectRename = useCallback(
    async (oldName: string, newName: string) => {
      try {
        await renameProject(oldName, newName);
        setRenamingProject(null);
        if (currentProject === oldName) {
          setCurrentProject(newName);
          setProjectFiles(await listProjectFiles(newName));
        }
        const message = `Renamed ${oldName} to ${newName}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [currentProject, refresh]
  );

  const confirmProjectDelete = useCallback(
    async (name: string) => {
      try {
        await deleteProjectIfEmpty(name);
        setDeletingProject(null);
        if (currentProject === name) {
          setCurrentProject(null);
          setMode("inbox");
          setProjectFiles([]);
          setArtifact(null);
          setSelectedPath(null);
          setSelectedPaths(new Set());
        }
        const message = `Deleted ${name}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        await refresh();
      } catch (caught) {
        const message = caught instanceof Error ? caught.message : String(caught);
        setError(message);
        if (message === "Move files out before deleting project") {
          setHandoffToast(message);
          window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3000);
        }
      }
    },
    [currentProject, refresh]
  );

  const sendFileFromMenu = useCallback(
    async (file: FileMetadata) => {
      setFileMenu(null);
      if (artifact?.path !== file.path) {
        setPendingSendPath(file.path);
        await openArtifact(file);
        return;
      }
      openSendPopover(true);
    },
    [artifact?.path, openArtifact, openSendPopover]
  );

  useEffect(() => {
    if (pendingSendPath && artifact?.path === pendingSendPath) {
      setPendingSendPath(null);
      openSendPopover(true);
    }
  }, [artifact?.path, openSendPopover, pendingSendPath]);

  useEffect(() => {
    let disposed = false;
    const unlisten = listen<TauriDragDropPayload>(TauriEvent.DRAG_DROP, (event) => {
      if (disposed) {
        return;
      }
      void ingestDroppedPaths(event.payload.paths ?? []);
    });
    return () => {
      disposed = true;
      void unlisten.then((dispose) => dispose());
    };
  }, [ingestDroppedPaths]);

  useEffect(() => {
    function handleWindowDragOver(event: DragEvent) {
      if (event.dataTransfer?.types.includes("Files")) {
        event.preventDefault();
      }
    }
    function handleWindowDrop(event: DragEvent) {
      const paths = pathsFromDataTransfer(event.dataTransfer);
      if (paths.length > 0) {
        event.preventDefault();
        void ingestDroppedPaths(paths);
      }
    }
    window.addEventListener("dragover", handleWindowDragOver);
    window.addEventListener("drop", handleWindowDrop);
    return () => {
      window.removeEventListener("dragover", handleWindowDragOver);
      window.removeEventListener("drop", handleWindowDrop);
    };
  }, [ingestDroppedPaths]);

  const paletteItems = useMemo(() => {
    const actions = [
      { section: "ACTIONS", label: "New File (⌘N)", run: () => setNewFileDialogOpen(true) },
      { section: "ACTIONS", label: sendButtonLabel, run: openSendPopover },
      { section: "ACTIONS", label: "Toggle Pin", run: toggleCurrentPin },
      { section: "ACTIONS", label: "Archive", run: archiveCurrent },
      { section: "ACTIONS", label: "Switch Agent Default...", run: switchAgentDefault },
      { section: "COMMANDS", label: "Reload Persona Registry", run: reloadPersonas },
      { section: "COMMANDS", label: "Install for Claude Code", run: () => void installMcpClient("Claude Code") },
      { section: "COMMANDS", label: "Install for Codex", run: () => void installMcpClient("Codex") },
      { section: "COMMANDS", label: "Install for Cursor", run: () => void installMcpClient("Cursor") },
      { section: "COMMANDS", label: "Edit Action Templates...", run: () => setActionTemplatesOpen(true) },
      {
        section: "COMMANDS",
        label: confirmBeforeRemove
          ? "Remove confirmation: on (click to turn off)"
          : "Remove confirmation: off (click to turn on)",
        run: toggleConfirmBeforeRemove
      }
    ];
    const projectItems = projects.map((project) => ({
      section: "PROJECTS",
      label: `Open: ${project}`,
      run: () => void openProject(project)
    }));
    const fileItems = files.map((file) => ({
      section: "FILES",
      label: file.name,
      run: () => void openArtifact(file)
    }));
    const allItems = [...actions, ...projectItems, ...fileItems];
    const query = paletteQuery.trim().toLowerCase();
    return query ? allItems.filter((item) => item.label.toLowerCase().includes(query)) : allItems;
  }, [archiveCurrent, confirmBeforeRemove, files, installMcpClient, openArtifact, openProject, openSendPopover, paletteQuery, projects, reloadPersonas, sendButtonLabel, switchAgentDefault, toggleConfirmBeforeRemove, toggleCurrentPin]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "n") {
        event.preventDefault();
        setNewFileDialogOpen(true);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "m" && annotationSelection) {
        event.preventDefault();
        openCommentDialog(annotationSelection);
        return;
      }
      if (isTextInput(event.target)) {
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setSearchQuery("");
        setSelectedPaths(selectedPath ? new Set([selectedPath]) : new Set());
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault();
        openSendPopover(event.shiftKey);
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen(true);
      }
      if (event.key === "j") {
        event.preventDefault();
        moveSelection(1, visibleFiles, selectedPath, setSelectedPath);
      }
      if (event.key === "k") {
        event.preventDefault();
        moveSelection(-1, visibleFiles, selectedPath, setSelectedPath);
      }
      if (event.key === "Enter" && selectedFile) {
        event.preventDefault();
        void openArtifact(selectedFile);
      }
      if (event.key === "e") {
        event.preventDefault();
        setEditMode((current) => !current);
      }
      if (event.key === "s") {
        event.preventDefault();
        openSendPopover();
      }
      if (event.key === "p") {
        event.preventDefault();
        void toggleCurrentPin();
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "Backspace") {
        event.preventDefault();
        void archiveCurrent();
      }
      if (event.key === "/") {
        event.preventDefault();
        searchRef.current?.focus();
      }
      if (event.key === "F2" && selectedFile) {
        event.preventDefault();
        setRenamingFile(selectedFile);
      }
      if (event.key === "?") {
        event.preventDefault();
        setShortcutsOpen((current) => !current);
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [
    archiveCurrent,
    openArtifact,
    annotationSelection,
    openCommentDialog,
    selectedFile,
    selectedPath,
    openSendPopover,
    toggleCurrentPin,
    visibleFiles
  ]);

  useEffect(() => {
    if (paletteIndex >= paletteItems.length) {
      setPaletteIndex(0);
    }
  }, [paletteIndex, paletteItems.length]);

  return (
    <main className="main-shell" aria-label="AgentCanvas">
          <aside className="sidebar">
            <div className="sidebar-header">
              <label className="search">
                <span>Search</span>
                <input
                  ref={searchRef}
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      event.preventDefault();
                      setSearchQuery("");
                      setSelectedPaths(selectedPath ? new Set([selectedPath]) : new Set());
                    }
                  }}
                  placeholder="Search artifacts"
                />
              </label>
            </div>
            <button
              className={`section-header section-button ${mode === "inbox" ? "selected" : ""}`}
              type="button"
              onClick={() => openInbox()}
            >
              <span className="section-label">Inbox</span>
              <span className="count">{files.length}</span>
              {inboxUnread > 0 ? (
                <span className="unread-badge" title={`${inboxUnread} unread`}>{inboxUnread}</span>
              ) : null}
            </button>
            <div
              className={`file-list ${dropTarget === "inbox" ? "drop-target" : ""}`}
              onDragEnter={() => setDropTarget("inbox")}
              onDragLeave={() => setDropTarget((current) => (current === "inbox" ? null : current))}
            >
              {filteredFiles.length === 0 ? (
                <div className="empty-list">
                  {searchQuery && mode === "inbox" ? (
                    "No matching artifacts"
                  ) : (
                    <>
                      <strong>Empty inbox</strong>
                      <span>Drag files here or use ⌘N</span>
                      <span>{bootstrap?.inbox_dir ?? "~/iCloud/AgentCanvas/Inbox"}</span>
                    </>
                  )}
                </div>
              ) : (
                filteredFiles.map((file) => (
                  <button
                    className={`file-row ${file.path === selectedPath ? "selected" : ""} ${
                      arrivedPaths.has(file.path) ? "just-arrived" : ""
                    } ${file.pinned ? "pinned" : ""} ${
                      selectedPaths.has(file.path) && selectedPaths.size > 1 ? "multi-selected" : ""
                    }`}
                    key={file.path}
                    type="button"
                    draggable
                    onDragStart={(event) => {
                      event.dataTransfer.setData("text/agentcanvas-path", file.path);
                      event.dataTransfer.effectAllowed = "move";
                    }}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setFileMenu({ x: event.clientX, y: event.clientY, file });
                    }}
                    onClick={(event) => {
                      setMode("inbox");
                      setCurrentProject(null);
                      void selectFileFromList(file, filteredFiles, event);
                    }}
                  >
                    <span className={`arrival-dot review-dot review-${file.review_state}`} title={reviewStateLabel(file.review_state)} />
                    <span className="file-name">
                      {file.pinned ? <span className="pin-star" title="Pinned">★ </span> : null}
                      {file.name}
                      {file.comment_count > 0 ? (
                        <span className="file-comment-count" title={`${file.comment_count} open comment${file.comment_count === 1 ? "" : "s"}`}>
                          {" "}💬 {file.comment_count}
                        </span>
                      ) : null}
                    </span>
                    <span
                      className="badge persona-badge"
                      style={{ color: personaColors.get(file.persona) ?? fallbackPersonaColor(file.persona) }}
                    >
                      {labelForPersona(file.persona)}
                    </span>
                    <span className="file-time" title={formatTimeTooltip(file.mtime)}>{formatTime(file.mtime)}</span>
                    <span
                      className="file-row-trash"
                      role="button"
                      tabIndex={-1}
                      aria-label={`Remove ${file.name} from AgentCanvas`}
                      title={`Remove ${file.name} from AgentCanvas`}
                      onClick={(event) => {
                        event.stopPropagation();
                        void removeFromCanvas(file);
                      }}
                      onMouseDown={(event) => event.stopPropagation()}
                    >
                      ×
                    </span>
                  </button>
                ))
              )}
            </div>
            <div className="section-header drafts-section-header">
              <button
                className={`section-header-label-btn ${mode === "drafts" ? "selected" : ""}`}
                type="button"
                onClick={() => void openDrafts()}
              >
                <span className="section-label">Drafts</span>
                <span className="count">{draftFiles.length}</span>
              </button>
              <button
                className="new-file-btn"
                type="button"
                title="New file (⌘N)"
                aria-label="New file"
                onClick={() => setNewFileDialogOpen(true)}
              >
                +
              </button>
            </div>
            {mode === "drafts" ? (
              <div className="file-list">
                {filteredDraftFiles.length === 0 ? (
                  <div className="empty-list">
                    {searchQuery ? (
                      "No matching drafts"
                    ) : (
                      <>
                        <strong>No drafts yet</strong>
                        <span>Press ⌘N to create a file</span>
                      </>
                    )}
                  </div>
                ) : (
                  filteredDraftFiles.map((file) => (
                    <button
                      className={`file-row ${file.path === selectedPath ? "selected" : ""} ${
                        file.pinned ? "pinned" : ""
                      } ${selectedPaths.has(file.path) && selectedPaths.size > 1 ? "multi-selected" : ""}`}
                      key={file.path}
                      type="button"
                      onContextMenu={(event) => {
                        event.preventDefault();
                        setFileMenu({ x: event.clientX, y: event.clientY, file });
                      }}
                      onClick={(event) => {
                        setMode("drafts");
                        setCurrentProject(null);
                        void selectFileFromList(file, filteredDraftFiles, event);
                      }}
                    >
                      <span className="arrival-dot review-dot review-reviewed" title="Draft" />
                      <span className="file-name">
                        {file.pinned ? <span className="pin-star" title="Pinned">★ </span> : null}
                        {file.name}
                      </span>
                      <span className="file-time" title={formatTimeTooltip(file.mtime)}>{formatTime(file.mtime)}</span>
                      <span
                        className="file-row-trash"
                        role="button"
                        tabIndex={-1}
                        aria-label={`Remove ${file.name} from AgentCanvas`}
                        title={`Remove ${file.name} from AgentCanvas`}
                        onClick={(event) => {
                          event.stopPropagation();
                          void removeFromCanvas(file);
                        }}
                        onMouseDown={(event) => event.stopPropagation()}
                      >
                        ×
                      </span>
                    </button>
                  ))
                )}
              </div>
            ) : null}
            <button
              className={`section-header section-button ${mode === "pinned" ? "selected" : ""}`}
              type="button"
              onClick={() => void openPinned()}
            >
              <span className="section-label">★ Pinned</span>
              <span className="count">{pinnedFiles.length}</span>
            </button>
            <div className="section-header projects-header">
              <span className="section-label">Projects</span>
              <span className="count">{projects.length}</span>
            </div>
            {projects.map((project) => (
              <button
                className={`project-row ${project === currentProject ? "selected" : ""} ${
                  dropTarget === `project:${project}` ? "drop-target" : ""
                }`}
                key={project}
                type="button"
                onDragOver={(event) => {
                  if (event.dataTransfer.types.includes("text/agentcanvas-path")) {
                    event.preventDefault();
                    setDropTarget(`project:${project}`);
                  }
                }}
                onDragLeave={() => setDropTarget((current) => (current === `project:${project}` ? null : current))}
                onDrop={(event) => {
                  const path = event.dataTransfer.getData("text/agentcanvas-path");
                  if (path) {
                    event.preventDefault();
                    setDropTarget(null);
                    void moveInboxFileToProject(path, project);
                  }
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  setProjectMenu({ x: event.clientX, y: event.clientY, project });
                }}
                onClick={() => void openProject(project)}
              >
                <span>{project}</span>
                <span className="file-time">{projectCounts.get(project) ?? 0}</span>
              </button>
            ))}
            <button
              className={`project-row archive-row ${mode === "archive" ? "selected" : ""} ${dropTarget === "archive" ? "drop-target" : ""}`}
              type="button"
              onDragOver={(event) => {
                if (event.dataTransfer.types.includes("text/agentcanvas-path")) {
                  event.preventDefault();
                  setDropTarget("archive");
                }
              }}
              onDragLeave={() => setDropTarget((current) => (current === "archive" ? null : current))}
              onDrop={(event) => {
                const path = event.dataTransfer.getData("text/agentcanvas-path");
                if (path) {
                  event.preventDefault();
                  setDropTarget(null);
                  void moveInboxFileToArchive(path);
                }
              }}
              onClick={() => void openArchive()}
            >
              <span>Archive</span>
              <span className="count">{archiveFiles.length}</span>
            </button>
            {recents.length > 0 ? (
              <>
                <button
                  className={`section-header section-button recents-header ${mode === "recents" ? "selected" : ""}`}
                  type="button"
                  onClick={() => {
                    setMode("recents");
                    setCurrentProject(null);
                    setSearchQuery("");
                  }}
                >
                  <span className="section-label">Recents</span>
                  <span className="count">{recents.length}</span>
                </button>
                {mode === "recents" ? (
                  <div className="file-list">
                    {recents.map((entry) => {
                      const name = entry.title || entry.path.split("/").pop() || entry.path;
                      return (
                        <button
                          className={`file-row recents-row ${entry.path === selectedPath ? "selected" : ""}`}
                          key={entry.path}
                          type="button"
                          title={entry.path}
                          onClick={() => {
                            void openExternalPath(entry.path);
                          }}
                        >
                          <span className="arrival-dot review-dot review-reviewed" title="External" />
                          <span className="file-name">{name}</span>
                          <span className="file-time" title={new Date(entry.last_opened * 1000).toLocaleString()}>
                            {formatTime(entry.last_opened)}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                ) : null}
              </>
            ) : null}
          </aside>
          {mode === "project" || mode === "archive" || mode === "pinned" ? (
            <aside className="middle">
              <div className="middle-header">
                <div className="middle-project-name">
                  {mode === "archive" ? "Archive" : mode === "pinned" ? "★ Pinned" : currentProject}
                </div>
                <div className="middle-project-meta">
                  {visibleFiles.length} artifacts
                </div>
              </div>
              <div className="middle-list">
                {visibleFiles.length === 0 ? (
                  <div className="empty-list">
                    {searchQuery ? "No matching artifacts" : emptyStateForMode(mode)}
                  </div>
                ) : visibleFiles.map((file) => (
                  <button
                    className={`middle-file ${file.path === selectedPath ? "selected" : ""} ${
                      file.pinned ? "pinned" : ""
                    } ${selectedPaths.has(file.path) && selectedPaths.size > 1 ? "multi-selected" : ""}`}
                    key={file.path}
                    type="button"
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setFileMenu({ x: event.clientX, y: event.clientY, file });
                    }}
                    onClick={(event) => void selectFileFromList(file, visibleFiles, event)}
                  >
                    <span className={`arrival-dot review-dot review-${file.review_state}`} title={reviewStateLabel(file.review_state)} />
                    <span>
                      {file.pinned ? <span className="pin-star" title="Pinned">★ </span> : null}
                      {file.name}
                      {file.comment_count > 0 ? (
                        <span className="file-comment-count" title={`${file.comment_count} open comment${file.comment_count === 1 ? "" : "s"}`}>
                          {" "}💬 {file.comment_count}
                        </span>
                      ) : null}
                    </span>
                    <small title={formatTimeTooltip(file.mtime)}>{formatTime(file.mtime)}</small>
                    <span
                      className="file-row-trash"
                      role="button"
                      tabIndex={-1}
                      aria-label={`Remove ${file.name} from AgentCanvas`}
                      title={`Remove ${file.name} from AgentCanvas`}
                      onClick={(event) => {
                        event.stopPropagation();
                        void removeFromCanvas(file);
                      }}
                      onMouseDown={(event) => event.stopPropagation()}
                    >
                      ×
                    </span>
                  </button>
                ))}
              </div>
            </aside>
          ) : null}
          <section className="content-pane">
            <div className="toolbar">
              <div className="toolbar-global-actions">
                <button type="button" onClick={refresh} disabled={isLoading}>
                  {isLoading ? "Scanning" : "Rescan"}
                </button>
                <button type="button" onClick={() => setNewFileDialogOpen(true)} title="New file (⌘N)" aria-label="New file">
                  New
                </button>
                <button type="button" onClick={() => void openFileDialog()} aria-label="Open file">
                  +
                </button>
              </div>
              <div className="breadcrumb">
                {mode === "archive" ? "Archive" : mode === "pinned" ? "★ Pinned" : mode === "project" ? (currentProject ?? "Project") : mode === "drafts" ? "Drafts" : mode === "recents" ? "Recents" : "Inbox"}
                <span>/</span> <strong>{selectedFile?.name ?? (artifact ? fileName(artifact.path) : null) ?? "Select a file"}</strong>
              </div>
              <div className="toolbar-actions">
                {artifact && isEditableArtifact(artifact.kind) ? (
                  <button
                    type="button"
                    onClick={() =>
                      artifact.kind === "html" || artifact.kind === "json"
                        ? setSourceMode((current) => !current)
                        : setEditMode((current) => !current)
                    }
                  >
                    {artifact.kind === "html" || artifact.kind === "json"
                      ? sourceMode
                        ? "Render"
                        : "View Source"
                      : editMode
                        ? "Preview"
                        : "Edit"}
                  </button>
                ) : null}
                {artifact?.kind === "json" ? (
                  <div className="segmented-control" aria-label="JSON view mode">
                    <button type="button" className={jsonViewMode === "tree" ? "active" : ""} onClick={() => setJsonViewMode("tree")} disabled={!parsedJson}>
                      Tree
                    </button>
                    <button type="button" className={jsonViewMode === "source" ? "active" : ""} onClick={() => setJsonViewMode("source")}>
                      Source
                    </button>
                  </div>
                ) : null}
                <button className="primary" type="button" onClick={() => openSendPopover()} disabled={!artifact && !multiSelectActive}>
                  {sendButtonLabel}
                </button>
                <button type="button" onClick={() => setCommentsOpen((current) => !current)} disabled={!artifact}>
                  Comments {comments.filter((comment) => !comment.resolved).length}
                </button>
                {artifact && isEditableArtifact(artifact.kind) ? (
                  <button type="button" onClick={() => void saveArtifact()} disabled={!artifact.dirty || isSaving}>
                    {isSaving ? "Saving" : "Save"}
                  </button>
                ) : null}
              </div>
            </div>
            {conflict ? (
              <div className="conflict-banner" role="alert">
                {fileName(artifact?.path ?? "File")} changed on disk since open. Resolve the merge dialog to continue.
              </div>
            ) : null}
            {personas?.warning ? <div className="registry-warning">{personas.warning}</div> : null}
            {savedAt ? <div className="saved-toast">Saved {savedAt}</div> : null}
            {handoffToast ? (
              <div className="handoff-toast">
                <strong>{handoffToast}</strong>
                {handoffToastBody ? <span>{handoffToastBody}</span> : null}
                {handoffToastAction ? (
                  <button
                    type="button"
                    onClick={async () => {
                      const [nextFiles, nextPinned, nextArchive, nextProjects] = await Promise.all([
                        listInbox(),
                        listPinned(),
                        listArchive(),
                        listProjects()
                      ]);
                      const nextProjectEntries = await Promise.all(
                        nextProjects.map(async (project) => [project, await listProjectFiles(project)] as const)
                      );
                      setFiles(nextFiles);
                      setPinnedFiles(nextPinned);
                      setArchiveFiles(nextArchive);
                      const projectEntry = nextProjectEntries.find(([, projectFiles]) =>
                        projectFiles.some((candidate) => candidate.path === handoffToastAction.artifact_path)
                      );
                      const file =
                        nextFiles.find((candidate) => candidate.path === handoffToastAction.artifact_path) ??
                        projectEntry?.[1].find((candidate) => candidate.path === handoffToastAction.artifact_path) ??
                        nextPinned.find((candidate) => candidate.path === handoffToastAction.artifact_path) ??
                        nextArchive.find((candidate) => candidate.path === handoffToastAction.artifact_path);
                      if (projectEntry) {
                        setMode("project");
                        setCurrentProject(projectEntry[0]);
                        setProjectFiles(projectEntry[1]);
                      }
                      if (file) {
                        await openArtifact(file);
                      }
                      setHandoffToast(null);
                      setHandoffToastBody(null);
                      setHandoffToastAction(null);
                    }}
                  >
                    {handoffToastAction.label}
                  </button>
                ) : null}
              </div>
            ) : null}
            {multiSelectActive ? (
              <MultiSelectPlaceholder
                files={selectedFileMetadatas}
                count={selectedPaths.size}
                onSend={() => openSendPopover()}
                onArchive={() => void archiveSelectedFiles()}
                onClear={() => setSelectedPaths(selectedPath ? new Set([selectedPath]) : new Set())}
              />
            ) : artifact ? (
              editMode || sourceMode || (artifact.kind === "json" && (jsonViewMode === "source" || !parsedJson)) ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="source-panel" aria-label="Source editor">
                    <SourceView
                      ref={sourceViewRef}
                      key={artifact.kind}
                      language={sourceLanguageForArtifact(artifact.kind)}
                      value={artifact.source}
                      onChange={updateSource}
                      onSave={saveArtifact}
                      onSelectionBoundsChange={(selection) => setAnnotationSelection(selectionFromSource(selection))}
                    />
                  </section>
                </div>
              ) : artifact.kind === "md" ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="rendered-panel" aria-label="Rendered Markdown">
                    <RenderedView blocks={previewBlocks ?? artifact.blocks} />
                  </section>
                </div>
              ) : artifact.kind === "html" ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="html-panel" aria-label="Rendered HTML">
                    <iframe
                      title={fileName(artifact.path)}
                      sandbox="allow-scripts allow-forms allow-popups allow-downloads"
                      srcDoc={injectBootstrap(artifact.source)}
                      ref={iframeRef}
                    />
                  </section>
                </div>
              ) : artifact.kind === "json" && parsedJson ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="json-panel" aria-label="JSON tree">
                    <JsonTree value={parsedJson} name={fileName(artifact.path)} />
                  </section>
                </div>
              ) : artifact.kind === "txt" ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="source-panel" aria-label="Text source">
                    <SourceView
                      ref={sourceViewRef}
                      key={artifact.kind}
                      language="plaintext"
                      value={artifact.source}
                      onChange={updateSource}
                      onSave={saveArtifact}
                      onSelectionBoundsChange={(selection) => setAnnotationSelection(selectionFromSource(selection))}
                    />
                  </section>
                </div>
              ) : artifact.kind === "png" ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="image-panel" aria-label="PNG image">
                    <div className="image-frame">
                      <img src={artifact.dataUrl} alt={fileName(artifact.path)} />
                      <p>{formatBytes(artifact.size ?? 0)}</p>
                    </div>
                  </section>
                </div>
              ) : artifact.kind === "pdf" ? (
                <div className="viewer-shell">
                  <div className="viewer-toolbar">
                    <FileLevelCommentButton onClick={() => setFileLevelDialogOpen(true)} count={fileLevelOpenCount} />
                  </div>
                  <section className="pdf-panel" aria-label="PDF document">
                    <object data={artifact.dataUrl} type="application/pdf" aria-label={fileName(artifact.path)}>
                      <div className="pdf-fallback">
                        <p>This PDF can't be previewed inline.</p>
                        <a href={artifact.dataUrl} download={fileName(artifact.path)} className="primary">
                          Download {fileName(artifact.path)}
                        </a>
                      </div>
                    </object>
                  </section>
                </div>
              ) : (
                <article className="document placeholder-document">
                  <p className="eyebrow">Unsupported artifact</p>
                  <h1>{fileName(artifact.path)}</h1>
                  <p>This viewer supports Markdown, HTML, PNG, JSON, TXT, and PDF artifacts.</p>
                </article>
              )
            ) : (
              <article className="document placeholder-document">
                <p className="eyebrow">Ready</p>
                <h1>{isOpening ? "Opening..." : "Select a file."}</h1>
                <p>Drop Markdown or HTML artifacts into the AgentCanvas inbox and rescan.</p>
              </article>
            )}
            {error ? <p className="error-banner">{error}</p> : null}
            {annotationSelection?.kind === "text" && artifact?.kind === "md" && editMode ? (
              <AnnotationToolbar selection={annotationSelection} onFormat={applyAnnotationFormat} onComment={openCommentDialog} />
            ) : null}
            {commentDialog ? (
              <CommentDialog
                title="Comment on selection"
                onCancel={() => setCommentDialog(null)}
                onSave={(body) => void saveComment(body)}
              />
            ) : null}
            {fileLevelDialogOpen ? (
              <CommentDialog
                title="Comment on file"
                onCancel={() => setFileLevelDialogOpen(false)}
                onSave={(body) => void saveFileLevelComment(body)}
              />
            ) : null}
            {sendPopoverOpen ? (
              <SendPopover
                label={sendButtonLabel}
                actionVerb={sendActionVerb}
                customActionVerb={customActionVerb}
                note={sendNote}
                onActionVerbChange={setSendActionVerb}
                onCustomActionVerbChange={(value) => {
                  setCustomActionVerb(value);
                  setSendActionVerb("Custom");
                }}
                onNoteChange={setSendNote}
                sessions={sendRouteSessions}
                showAgentPicker={showAgentPicker}
                selectedAgentId={selectedAgentId}
                onSelectedAgentChange={setSelectedAgentId}
                onCancel={() => setSendPopoverOpen(false)}
                onSend={() => void sendCurrentArtifact(sendActionVerb === "Custom" ? customActionVerb : sendActionVerb, sendNote)}
              />
            ) : null}
          </section>
          {commentsOpen ? (
            <CommentsPanel
              comments={comments}
              hoveredCommentId={hoveredCommentId}
              onHover={setHoveredCommentId}
              onSelect={revealComment}
              onResolve={(commentId) => void resolveComment(commentId)}
            />
          ) : (
            <aside className="comments-gutter">
              <button type="button" onClick={() => setCommentsOpen(true)} disabled={!artifact}>
                Comments
              </button>
            </aside>
          )}
          {sessions.length === 0 && !showSessionForm ? (
            <aside className="agent-gutter">
              <button type="button" onClick={() => setShowSessionForm(true)}>
                + Connect
              </button>
            </aside>
          ) : (
            <aside className="agent-panel">
              <div className="agent-panel-header">
                <span>Agent Sessions</span>
                <button type="button" onClick={() => setShowSessionForm((current) => !current)}>
                  +
                </button>
              </div>
              {showSessionForm ? (
                <form
                  className="session-form"
                  onSubmit={(event) => {
                    event.preventDefault();
                    void addSession();
                  }}
                >
                  <select value={sessionPersona} onChange={(event) => setSessionPersona(event.target.value)}>
                    {(personas?.personas ?? []).map((persona) => (
                      <option key={persona.name} value={persona.name}>
                        {persona.display_label}
                      </option>
                    ))}
                  </select>
                  <select value={sessionBackbone} onChange={(event) => setSessionBackbone(event.target.value)}>
                    <option value="claude">claude</option>
                    <option value="codex">codex</option>
                    <option value="other">other</option>
                  </select>
                  <input
                    value={sessionContext}
                    onChange={(event) => setSessionContext(event.target.value)}
                    placeholder="[context]"
                  />
                  <button type="submit">Add session</button>
                </form>
              ) : null}
              <div className="agent-session-list">
                {sessions.map((session) => (
                  <article
                    className={`agent-card ${session.id === defaultAgentId ? "default-agent" : ""}`}
                    data-source={session.source}
                    key={session.id}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      if (session.source === "manual") {
                        setAgentMenu({ x: event.clientX, y: event.clientY, session });
                      }
                    }}
                  >
                    <div className="agent-card-top">
                      <span className={`status-dot ${session.is_live ? "connected" : "offline"}`} aria-label={session.is_live ? "Connected" : "Offline"} />
                      <span
                        className="persona-chip"
                        style={{ color: personaColors.get(session.persona) ?? fallbackPersonaColor(session.persona) }}
                      >
                        {session.persona}·{session.agent}
                      </span>
                      <button
                        className="agent-session-action"
                        type="button"
                        onClick={() => {
                          if (session.source === "mcp") {
                            void disconnectLiveSession(session.id);
                          } else {
                            void removeManualSession(session.id);
                          }
                        }}
                      >
                        {session.source === "mcp" ? "Disconnect" : "Remove"}
                      </button>
                    </div>
                    <div className="agent-context">{session.project || "current"}</div>
                    {session.attached_paths.length > 0 ? (
                      <ul className="attached-list">
                        {session.attached_paths.map((path) => (
                          <li key={path}>{fileName(path)}</li>
                        ))}
                      </ul>
                    ) : null}
                    {session.id === defaultAgentId ? <div className="agent-default">default for {currentProjectKey}</div> : null}
                  </article>
                ))}
              </div>
            </aside>
          )}
        {paletteOpen ? (
          <div className="palette-backdrop" onMouseDown={() => setPaletteOpen(false)}>
            <section
              ref={paletteRef}
              className="palette"
              role="dialog"
              aria-modal="true"
              aria-label="Command palette"
              onMouseDown={(event) => event.stopPropagation()}
            >
              <div className="palette-search">
                <input
                  autoFocus
                  value={paletteQuery}
                  onChange={(event) => {
                    setPaletteQuery(event.target.value);
                    setPaletteIndex(0);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      setPaletteOpen(false);
                    }
                    if (event.key === "ArrowDown") {
                      event.preventDefault();
                      setPaletteIndex((current) => Math.min(current + 1, Math.max(0, paletteItems.length - 1)));
                    }
                    if (event.key === "ArrowUp") {
                      event.preventDefault();
                      setPaletteIndex((current) => Math.max(0, current - 1));
                    }
                    if (event.key === "Enter") {
                      event.preventDefault();
                      const item = paletteItems[paletteIndex];
                      if (item) {
                        void item.run();
                        setPaletteOpen(false);
                      }
                    }
                  }}
                  placeholder="Search actions, files, commands"
                />
                <span>Esc</span>
              </div>
              <div className="palette-results">
                {paletteItems.map((item, index) => (
                  <button
                    className={`palette-row ${index === paletteIndex ? "active" : ""}`}
                    key={`${item.section}-${item.label}`}
                    type="button"
                    onClick={() => {
                      void item.run();
                      setPaletteOpen(false);
                    }}
                  >
                    <span>{item.section}</span>
                    <strong>{item.label}</strong>
                  </button>
                ))}
              </div>
            </section>
          </div>
        ) : null}
        {shortcutsOpen ? (
          <div className="shortcuts-overlay">
            <div className="shortcuts-card">
              <strong>Keyboard</strong>
              <span>j/k nav</span>
              <span>Enter open</span>
              <span>e edit</span>
              <span>s send</span>
              <span>p pin</span>
              <span>Cmd+Backspace archive</span>
              <span>/ search</span>
              <span>Cmd+K palette</span>
            </div>
          </div>
        ) : null}
        {agentMenu ? (
          <div className="context-menu-backdrop" onMouseDown={() => setAgentMenu(null)}>
            <div
              className="context-menu"
              style={{ left: agentMenu.x, top: agentMenu.y }}
              onMouseDown={(event) => event.stopPropagation()}
            >
              <button type="button" onClick={() => void setDefaultAgentForProject(agentMenu.session)}>
                Set as default for {currentProjectKey}
              </button>
            </div>
          </div>
        ) : null}
        {fileMenu ? (
          <div className="context-menu-backdrop" onMouseDown={() => setFileMenu(null)}>
            <div
              className="context-menu file-context-menu"
              style={{ left: fileMenu.x, top: fileMenu.y }}
              onMouseDown={(event) => event.stopPropagation()}
            >
              <button type="button" onClick={() => { setFileMenu(null); void openArtifact(fileMenu.file); }}>
                Open
              </button>
              <button type="button" onClick={() => { setFileMenu(null); void togglePin(fileMenu.file.path).then(refresh); }}>
                Toggle Pin (⌘P)
              </button>
              <button type="button" onClick={() => { setRenamingFile(fileMenu.file); setFileMenu(null); }}>
                Rename... (F2)
              </button>
              <div className="context-menu-label">Mark as...</div>
              {REVIEW_STATES.map((state) => (
                <button key={state} type="button" onClick={() => void markFileReviewState(fileMenu.file, state)}>
                  {reviewStateLabel(state)}
                </button>
              ))}
              <div className="context-menu-label">File to Project</div>
              {projects.map((project) => (
                <button key={project} type="button" onClick={() => void moveKnownFileToProject(fileMenu.file, project)}>
                  {project}
                </button>
              ))}
              <button type="button" onClick={() => void moveKnownFileToArchive(fileMenu.file)}>
                Archive (⌘⌫)
              </button>
              <button type="button" onClick={() => void sendFileFromMenu(fileMenu.file)}>
                Send to Agent... (⌘⏎)
              </button>
              <button type="button" onClick={() => void exportKnownFile(fileMenu.file)}>
                Export to...
              </button>
              <button type="button" onClick={() => { setFileMenu(null); void revealInFinder(fileMenu.file.path); }}>
                Reveal in Finder
              </button>
              <button
                type="button"
                onClick={() => {
                  const file = fileMenu.file;
                  const message = `Copied ${file.path}`;
                  void copyTextToClipboard(file.path);
                  setHandoffToast(message);
                  setFileMenu(null);
                  window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
                }}
              >
                Copy Path
              </button>
              <button
                type="button"
                onClick={() => {
                  const file = fileMenu.file;
                  const message = `Copied ${file.relative_path}`;
                  void copyTextToClipboard(file.relative_path);
                  setHandoffToast(message);
                  setFileMenu(null);
                  window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
                }}
              >
                Copy Relative Path
              </button>
              <button type="button" onClick={() => void removeFromCanvas(fileMenu.file)}>
                Remove from AgentCanvas
              </button>
              <button className="danger-item" type="button" onClick={() => void deleteArtifact(fileMenu.file)}>
                Delete file from disk...
              </button>
            </div>
          </div>
        ) : null}
        {projectMenu ? (
          <div className="context-menu-backdrop" onMouseDown={() => setProjectMenu(null)}>
            <div
              className="context-menu"
              style={{ left: projectMenu.x, top: projectMenu.y }}
              onMouseDown={(event) => event.stopPropagation()}
            >
              <button type="button" onClick={() => { const project = projectMenu.project; setProjectMenu(null); void openProject(project); }}>
                Open
              </button>
              <button type="button" onClick={() => { setRenamingProject(projectMenu.project); setProjectMenu(null); }}>
                Rename...
              </button>
              <button className="danger-item" type="button" onClick={() => { setDeletingProject(projectMenu.project); setProjectMenu(null); }}>
                Delete...
              </button>
            </div>
          </div>
        ) : null}
        {renamingProject ? (
          <ProjectRenameDialog
            project={renamingProject}
            onCancel={() => setRenamingProject(null)}
            onRename={(nextName) => void submitProjectRename(renamingProject, nextName)}
          />
        ) : null}
        {deletingProject ? (
          <ProjectDeleteDialog
            project={deletingProject}
            onCancel={() => setDeletingProject(null)}
            onDelete={() => void confirmProjectDelete(deletingProject)}
          />
        ) : null}
        {renamingFile ? (
          <RenameFileDialog
            file={renamingFile}
            onCancel={() => setRenamingFile(null)}
            onRename={(nextName) => void renameSelectedFile(nextName)}
          />
        ) : null}
        {conflictDialog ? (
          <ConflictDialog
            filename={conflictDialog.filename}
            target={conflictDialog.target}
            onResolve={(strategy) => {
              conflictDialog.resolve(strategy);
              setConflictDialog(null);
            }}
          />
        ) : null}
        {confirmDialog ? (
          <ConfirmDialog
            title={confirmDialog.title}
            body={confirmDialog.body}
            confirmLabel={confirmDialog.confirmLabel}
            destructive={confirmDialog.destructive}
            onResolve={(ok) => {
              confirmDialog.resolve(ok);
              setConfirmDialog(null);
            }}
          />
        ) : null}
        {agentPickerOpen ? (
          <AgentPickerDialog
            project={currentProjectKey}
            sessions={manualSessions}
            defaultAgentId={defaultAgentId}
            onCancel={() => setAgentPickerOpen(false)}
            onConfirm={(session) => {
              setAgentPickerOpen(false);
              void setDefaultAgentForProject(session);
            }}
          />
        ) : null}
        {mergeConflict ? (
          <ConflictMergeDialog
            conflict={mergeConflict}
            isSaving={isSaving}
            onKeepMine={() => void keepMineFromMerge()}
            onKeepTheirs={() => void keepTheirsFromMerge()}
            onCancel={() => void cancelMergeAndReload()}
          />
        ) : null}
        {actionTemplatesOpen ? (
          <ActionTemplatesDialog
            templates={actionTemplates}
            onCancel={() => setActionTemplatesOpen(false)}
            onSave={(templates) => void saveActionTemplates(templates)}
            onReset={() => void resetActionTemplatesToDefaults()}
          />
        ) : null}
        {newFileDialogOpen ? (
          <NewFileDialog
            onCancel={() => setNewFileDialogOpen(false)}
            onCreate={(name) => void createNewFile(name)}
          />
        ) : null}
    </main>
  );
}

type MultiSelectPlaceholderProps = {
  files: FileMetadata[];
  count: number;
  onSend: () => void;
  onArchive: () => void;
  onClear: () => void;
};

type FileLevelCommentButtonProps = {
  onClick: () => void;
  count: number;
};

function FileLevelCommentButton({ onClick, count }: FileLevelCommentButtonProps) {
  return (
    <button type="button" className="file-comment-button" onClick={onClick}>
      {count > 0 ? `Comments (${count}) — add another` : "Add comment about this file"}
    </button>
  );
}

type CommentsPanelProps = {
  comments: Comment[];
  hoveredCommentId: string | null;
  onHover: (id: string | null) => void;
  onSelect: (comment: Comment) => void;
  onResolve: (commentId: string) => void;
};

function CommentsPanel({ comments, hoveredCommentId, onHover, onSelect, onResolve }: CommentsPanelProps) {
  const openComments = comments.filter((comment) => !comment.resolved);
  const selectionComments = openComments.filter((comment) => comment.anchor.kind !== "file_level");
  const fileLevelComments = openComments.filter((comment) => comment.anchor.kind === "file_level");
  const renderCard = (comment: Comment) => (
    <article
      className={`comment-card ${hoveredCommentId === comment.id ? "active" : ""}`}
      key={comment.id}
      onMouseEnter={() => {
        onHover(comment.id);
        onSelect(comment);
      }}
      onMouseLeave={() => onHover(null)}
    >
      <button type="button" className="comment-body" onClick={() => onSelect(comment)}>
        <span>{comment.author} · {formatTime(comment.created_at)}</span>
        <strong>{comment.body}</strong>
      </button>
      <button type="button" onClick={() => onResolve(comment.id)}>Resolve</button>
    </article>
  );

  return (
    <aside className="comments-panel">
      <div className="agent-panel-header">
        <span>Comments</span>
        <span className="count">{openComments.length}</span>
      </div>
      <div className="comments-list">
        {openComments.length === 0 ? (
          <div className="empty-list">No comments</div>
        ) : (
          <>
            {selectionComments.length > 0 ? (
              <>
                <div className="comments-section-label">Selections</div>
                {selectionComments.map(renderCard)}
              </>
            ) : null}
            {fileLevelComments.length > 0 ? (
              <>
                <div className="comments-section-label">About this file</div>
                {fileLevelComments.map(renderCard)}
              </>
            ) : null}
          </>
        )}
      </div>
    </aside>
  );
}

type CommentDialogProps = {
  title?: string;
  onCancel: () => void;
  onSave: (body: string) => void;
};

function CommentDialog({ title = "Comment", onCancel, onSave }: CommentDialogProps) {
  const [body, setBody] = useState("");
  const dialogRef = useRef<HTMLFormElement | null>(null);
  useFocusTrap(dialogRef, onCancel);
  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <form
        ref={dialogRef}
        className="send-popover project-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="comment-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          if (body.trim()) {
            onSave(body.trim());
          }
        }}
      >
        <header id="comment-dialog-title">{title}</header>
        <label className="send-note">
          <span>Body</span>
          <textarea value={body} onChange={(event) => setBody(event.target.value)} rows={5} />
        </label>
        <footer>
          <button type="button" onClick={onCancel}>Cancel</button>
          <button className="primary" type="submit" disabled={!body.trim()}>Save</button>
        </footer>
      </form>
    </div>
  );
}

type ActionTemplatesDialogProps = {
  templates: ActionTemplate[];
  onCancel: () => void;
  onSave: (templates: ActionTemplate[]) => void;
  onReset: () => void;
};

function ActionTemplatesDialog({ templates, onCancel, onSave, onReset }: ActionTemplatesDialogProps) {
  const [drafts, setDrafts] = useState(templates);
  const dialogRef = useRef<HTMLFormElement | null>(null);
  useFocusTrap(dialogRef, onCancel);
  useEffect(() => setDrafts(templates), [templates]);
  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <form
        ref={dialogRef}
        className="send-popover action-templates-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="action-templates-title"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          onSave(drafts);
        }}
      >
        <header id="action-templates-title">Action Templates</header>
        <div className="template-list">
          {drafts.map((template, index) => (
            <label key={template.verb} className="template-row">
              <span>{template.verb}</span>
              <textarea
                value={template.template}
                onChange={(event) => {
                  const next = [...drafts];
                  next[index] = { ...template, template: event.target.value };
                  setDrafts(next);
                }}
                rows={3}
              />
            </label>
          ))}
        </div>
        <footer>
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" onClick={onReset}>Reset to defaults</button>
          <button className="primary" type="submit">Save</button>
        </footer>
      </form>
    </div>
  );
}

function MultiSelectPlaceholder({ files, count, onSend, onArchive, onClear }: MultiSelectPlaceholderProps) {
  return (
    <article className="document multi-select-placeholder">
      <p className="eyebrow">Selection</p>
      <h1>{count} files selected</h1>
      <ul>
        {files.map((file) => (
          <li key={file.path}>{file.name}</li>
        ))}
      </ul>
      <div className="multi-select-actions">
        <button className="primary" type="button" onClick={onSend}>
          Send to Agent (⌘⏎)
        </button>
        <button type="button" onClick={onArchive}>
          Archive
        </button>
        <button type="button" onClick={onClear}>
          Clear (Esc)
        </button>
      </div>
    </article>
  );
}

type AnnotationToolbarProps = {
  selection: TextAnnotationSelection;
  onFormat: (format: SourceFormat) => void;
  onComment: (selection: TextAnnotationSelection) => void;
};

function AnnotationToolbar({ selection, onFormat, onComment }: AnnotationToolbarProps) {
  const left = selection.rect.left + selection.rect.width / 2;
  const top = Math.max(8, selection.rect.top - 44);
  return (
    <div className="annotation-toolbar" style={{ left, top }} role="toolbar" aria-label="Annotation toolbar">
      <button type="button" title="Bold (⌘B)" onMouseDown={(event) => event.preventDefault()} onClick={() => onFormat("bold")}>
        <strong>B</strong>
      </button>
      <button type="button" title="Italic (⌘I)" onMouseDown={(event) => event.preventDefault()} onClick={() => onFormat("italic")}>
        <em>I</em>
      </button>
      <button type="button" title="Strikethrough (⌘⇧X)" onMouseDown={(event) => event.preventDefault()} onClick={() => onFormat("strike")}>
        <span className="strike-icon">S</span>
      </button>
      <button type="button" title="Code (`)" onMouseDown={(event) => event.preventDefault()} onClick={() => onFormat("code")}>
        <code>`</code>
      </button>
      <button type="button" title="Mark for Revision" onMouseDown={(event) => event.preventDefault()} onClick={() => onFormat("revision")}>
        Mark
      </button>
      <button type="button" title="Comment (⌘⇧M)" onMouseDown={(event) => event.preventDefault()} onClick={() => onComment(selection)}>
        Comment
      </button>
    </div>
  );
}

type ConflictMergeDialogProps = {
  conflict: NonNullable<MergeConflict>;
  isSaving: boolean;
  onKeepMine: () => void;
  onKeepTheirs: () => void;
  onCancel: () => void;
};

function ConflictMergeDialog({ conflict, isSaving, onKeepMine, onKeepTheirs, onCancel }: ConflictMergeDialogProps) {
  const dialogRef = useRef<HTMLElement | null>(null);
  useFocusTrap(dialogRef, onCancel);
  return (
    <div className="palette-backdrop" onMouseDown={onCancel}>
      <section
        className="merge-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="merge-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
      >
        <header>
          <div>
            <p className="eyebrow">Save conflict</p>
            <h2 id="merge-dialog-title">{conflict.filename} changed on disk</h2>
          </div>
          <button type="button" onClick={onCancel}>Cancel</button>
        </header>
        <div className="merge-columns">
          <MergeColumn title="Your draft" source={conflict.draftSource} />
          <MergeColumn
            title="Common ancestor"
            source={conflict.baseSnapshot?.source ?? "No common ancestor available"}
            muted={!conflict.baseSnapshot}
          />
          <MergeColumn title="On disk now" source={conflict.diskSource} />
        </div>
        <footer>
          <button type="button" className="primary" onClick={onKeepMine} disabled={isSaving}>
            {isSaving ? "Writing" : "Keep mine"}
          </button>
          <button type="button" onClick={onKeepTheirs} disabled={isSaving}>
            Keep theirs
          </button>
          <button type="button" onClick={onCancel} disabled={isSaving}>
            Cancel
          </button>
        </footer>
      </section>
    </div>
  );
}

function MergeColumn({ title, source, muted = false }: { title: string; source: string; muted?: boolean }) {
  return (
    <section className={`merge-column ${muted ? "muted" : ""}`}>
      <h3>{title}</h3>
      <pre>
        <code>{source}</code>
      </pre>
    </section>
  );
}

type AgentPickerDialogProps = {
  project: string;
  sessions: AgentSession[];
  defaultAgentId: string | null;
  onCancel: () => void;
  onConfirm: (session: AgentSession) => void;
};

function AgentPickerDialog({ project, sessions, defaultAgentId, onCancel, onConfirm }: AgentPickerDialogProps) {
  const [selected, setSelected] = useState(defaultAgentId ?? sessions[0]?.id ?? "");
  const dialogRef = useRef<HTMLElement | null>(null);
  const selectedSession = sessions.find((session) => session.id === selected) ?? null;

  useFocusTrap(dialogRef, onCancel);

  const confirm = () => {
    if (selectedSession) {
      onConfirm(selectedSession);
    }
  };

  return (
    <div className="palette-backdrop" onMouseDown={onCancel}>
      <section
        className="rename-dialog agent-picker-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={`Default Agent for ${project}`}
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          } else if (event.key === "Enter") {
            event.preventDefault();
            confirm();
          }
        }}
      >
        <header>Default Agent for {project}</header>
        <fieldset>
          {sessions.map((session, index) => (
            <label key={session.id}>
              <input
                type="radio"
                name="default-agent"
                value={session.id}
                checked={selected === session.id}
                onChange={() => setSelected(session.id)}
              />
              <span>{agentSessionLabel(session)}</span>
              <small>{session.project || "current"}</small>
            </label>
          ))}
        </fieldset>
        <footer>
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" className="primary" onClick={confirm} disabled={!selectedSession}>OK</button>
        </footer>
      </section>
    </div>
  );
}

type RenameFileDialogProps = {
  file: FileMetadata;
  onCancel: () => void;
  onRename: (newName: string) => void;
};

function RenameFileDialog({ file, onCancel, onRename }: RenameFileDialogProps) {
  const [value, setValue] = useState(file.name);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  useFocusTrap(dialogRef, onCancel);

  useEffect(() => {
    inputRef.current?.focus();
    const lastDot = file.name.lastIndexOf(".");
    if (lastDot > 0) {
      inputRef.current?.setSelectionRange(0, lastDot);
    } else {
      inputRef.current?.select();
    }
  }, [file.name]);

  const submit = () => {
    const trimmed = value.trim();
    if (trimmed && trimmed !== file.name) {
      onRename(trimmed);
    } else {
      onCancel();
    }
  };

  return (
    <div className="palette-backdrop" onMouseDown={onCancel}>
      <div
        ref={dialogRef}
        className="rename-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`Rename ${file.name}`}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>Rename {file.name}</header>
        <input
          ref={inputRef}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              submit();
            } else if (event.key === "Escape") {
              event.preventDefault();
              onCancel();
            }
          }}
        />
        <footer>
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" className="primary" onClick={submit}>Rename</button>
        </footer>
      </div>
    </div>
  );
}

type ConflictDialogProps = {
  filename: string;
  target: string;
  onResolve: (strategy: ConflictStrategy) => void;
};

function ConflictDialog({ filename, target, onResolve }: ConflictDialogProps) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  useFocusTrap(dialogRef, () => onResolve("cancel"));
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onResolve("cancel");
      } else if (event.key === "Enter") {
        event.preventDefault();
        onResolve("keep_both");
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onResolve]);

  return (
    <div className="palette-backdrop" onMouseDown={() => onResolve("cancel")}>
      <div
        ref={dialogRef}
        className="rename-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={`Replace ${filename}`}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>Replace {filename}?</header>
        <p style={{ margin: "0 0 4px", color: "var(--text-secondary)", fontSize: 13 }}>
          A file with this name already exists in {target}.
        </p>
        <footer style={{ flexWrap: "wrap", gap: 8 }}>
          <button type="button" onClick={() => onResolve("cancel")}>Cancel</button>
          <button type="button" onClick={() => onResolve("replace")} style={{ borderColor: "var(--diff-rem-strong)", color: "var(--diff-rem-strong)" }}>
            Replace
          </button>
          <button type="button" className="primary" onClick={() => onResolve("keep_both")}>
            Keep Both
          </button>
        </footer>
      </div>
    </div>
  );
}

type ConfirmDialogProps = {
  title: string;
  body: string;
  confirmLabel: string;
  destructive: boolean;
  onResolve: (ok: boolean) => void;
};

function ConfirmDialog({ title, body, confirmLabel, destructive, onResolve }: ConfirmDialogProps) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  useFocusTrap(dialogRef, () => onResolve(false));
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onResolve(false);
      } else if (event.key === "Enter") {
        event.preventDefault();
        onResolve(true);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onResolve]);

  return (
    <div className="palette-backdrop" onMouseDown={() => onResolve(false)}>
      <div
        ref={dialogRef}
        className="rename-dialog"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>{title}</header>
        <p style={{ margin: "0 0 4px", color: "var(--text-secondary)", fontSize: 13 }}>{body}</p>
        <footer style={{ flexWrap: "wrap", gap: 8 }}>
          <button type="button" onClick={() => onResolve(false)}>Cancel</button>
          <button
            type="button"
            className={destructive ? undefined : "primary"}
            onClick={() => onResolve(true)}
            style={destructive ? { borderColor: "var(--diff-rem-strong)", color: "var(--diff-rem-strong)" } : undefined}
          >
            {confirmLabel}
          </button>
        </footer>
      </div>
    </div>
  );
}

type SendPopoverProps = {
  label: string;
  actionVerb: string;
  customActionVerb: string;
  note: string;
  onActionVerbChange: (verb: string) => void;
  onCustomActionVerbChange: (verb: string) => void;
  onNoteChange: (note: string) => void;
  sessions: AgentSession[];
  showAgentPicker: boolean;
  selectedAgentId: string | null;
  onSelectedAgentChange: (sessionId: string) => void;
  onCancel: () => void;
  onSend: () => void;
};

function SendPopover({
  label,
  actionVerb,
  customActionVerb,
  note,
  onActionVerbChange,
  onCustomActionVerbChange,
  onNoteChange,
  sessions,
  showAgentPicker,
  selectedAgentId,
  onSelectedAgentChange,
  onCancel,
  onSend
}: SendPopoverProps) {
  const noteRef = useRef<HTMLTextAreaElement | null>(null);
  const popoverRef = useRef<HTMLFormElement | null>(null);
  useFocusTrap(popoverRef, onCancel);

  useEffect(() => {
    noteRef.current?.focus();
  }, []);

  return (
    <div className="send-popover-backdrop" onMouseDown={onCancel}>
      <form
        ref={popoverRef}
        className="send-popover"
        role="dialog"
        aria-modal="true"
        aria-label={label}
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          onSend();
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
          if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault();
            onSend();
          }
        }}
      >
        <header>{label}</header>
        {showAgentPicker ? (
          <label className="agent-picker">
            <span>Agent</span>
            <select value={selectedAgentId ?? ""} onChange={(event) => onSelectedAgentChange(event.target.value)}>
              <option value="" disabled>
                Choose agent
              </option>
              {sessions.map((session) => (
                <option key={session.id} value={session.id}>
                  {agentSessionLabel(session)}
                </option>
              ))}
            </select>
          </label>
        ) : null}
        <fieldset>
          <legend>Action</legend>
          <div className="action-grid">
            {ACTION_VERBS.map((verb) => (
              <label key={verb}>
                <input
                  type="radio"
                  name="send-action"
                  value={verb}
                  checked={actionVerb === verb}
                  onChange={() => onActionVerbChange(verb)}
                />
                <span>{verb}</span>
              </label>
            ))}
            <label className="custom-action">
              <input
                type="radio"
                name="send-action"
                value="Custom"
                checked={actionVerb === "Custom"}
                onChange={() => onActionVerbChange("Custom")}
              />
              <span>Custom:</span>
              <input
                value={customActionVerb}
                onChange={(event) => onCustomActionVerbChange(event.target.value)}
                onFocus={() => onActionVerbChange("Custom")}
                placeholder="Action verb"
              />
            </label>
          </div>
        </fieldset>
        <label className="send-note">
          <span>Note (optional)</span>
          <textarea ref={noteRef} value={note} onChange={(event) => onNoteChange(event.target.value)} rows={3} />
        </label>
        <footer>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button className="primary" type="submit">
            Send ↵
          </button>
        </footer>
      </form>
    </div>
  );
}

type ProjectRenameDialogProps = {
  project: string;
  onCancel: () => void;
  onRename: (name: string) => void;
};

function ProjectRenameDialog({ project, onCancel, onRename }: ProjectRenameDialogProps) {
  const [name, setName] = useState(project);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const dialogRef = useRef<HTMLFormElement | null>(null);
  useFocusTrap(dialogRef, onCancel);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <form
        className="send-popover project-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="Rename Project"
        onMouseDown={(event) => event.stopPropagation()}
        onSubmit={(event) => {
          event.preventDefault();
          const nextName = name.trim();
          if (nextName && nextName !== project) {
            onRename(nextName);
          } else {
            onCancel();
          }
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
      >
        <header>Rename Project</header>
        <label className="send-note">
          <span>Name</span>
          <input ref={inputRef} value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <footer>
          <button type="button" onClick={onCancel}>
            Cancel
          </button>
          <button className="primary" type="submit" disabled={!name.trim() || name.trim() === project}>
            Rename
          </button>
        </footer>
      </form>
    </div>
  );
}

type ProjectDeleteDialogProps = {
  project: string;
  onCancel: () => void;
  onDelete: () => void;
};

function ProjectDeleteDialog({ project, onCancel, onDelete }: ProjectDeleteDialogProps) {
  const cancelRef = useRef<HTMLButtonElement | null>(null);
  const dialogRef = useRef<HTMLElement | null>(null);
  useFocusTrap(dialogRef, onCancel);

  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <section
        className="send-popover project-dialog"
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label="Delete Project"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
        }}
      >
        <header>Delete Project</header>
        <div className="dialog-copy">
          <strong>{project}</strong>
          <span>Only empty projects can be deleted. Move files out before deleting.</span>
        </div>
        <footer>
          <button ref={cancelRef} type="button" onClick={onCancel}>
            Cancel
          </button>
          <button className="danger-item" type="button" onClick={onDelete}>
            Delete
          </button>
        </footer>
      </section>
    </div>
  );
}

function JsonTree({ value, name }: { value: JsonValue; name: string }) {
  return (
    <div className="json-tree">
      <JsonNode name={name} value={value} defaultOpen />
    </div>
  );
}

function JsonNode({ name, value, defaultOpen = false }: { name: string; value: JsonValue; defaultOpen?: boolean }) {
  if (Array.isArray(value)) {
    return (
      <details open={defaultOpen}>
        <summary>
          <span>{name}</span>
          <code>[{value.length}]</code>
        </summary>
        <div className="json-children">
          {value.map((item, index) => (
            <JsonNode key={index} name={String(index)} value={item} />
          ))}
        </div>
      </details>
    );
  }

  if (value && typeof value === "object") {
    const entries = Object.entries(value);
    return (
      <details open={defaultOpen}>
        <summary>
          <span>{name}</span>
          <code>{`{${entries.length}}`}</code>
        </summary>
        <div className="json-children">
          {entries.map(([key, item]) => (
            <JsonNode key={key} name={key} value={item} />
          ))}
        </div>
      </details>
    );
  }

  return (
    <div className="json-leaf">
      <span>{name}</span>
      <code>{JSON.stringify(value)}</code>
    </div>
  );
}

function markdownExtension(extension: string): boolean {
  const ext = extension.toLowerCase();
  return ext === "md" || ext === "markdown";
}

function htmlExtension(extension: string): boolean {
  const ext = extension.toLowerCase();
  return ext === "html" || ext === "htm";
}

function pngExtension(extension: string): boolean {
  return extension.toLowerCase() === "png";
}

function pdfExtension(extension: string): boolean {
  return extension.toLowerCase() === "pdf";
}

function jsonExtension(extension: string): boolean {
  return extension.toLowerCase() === "json";
}

function txtExtension(extension: string): boolean {
  return extension.toLowerCase() === "txt";
}

function jsonParses(source: string): boolean {
  try {
    JSON.parse(source);
    return true;
  } catch {
    return false;
  }
}

function isEditableArtifact(kind: ArtifactKind): boolean {
  return kind === "md" || kind === "html" || kind === "json" || kind === "txt";
}

function sourceLanguageForArtifact(kind: ArtifactKind): "markdown" | "json" | "plaintext" {
  if (kind === "json") {
    return "json";
  }
  if (kind === "md") {
    return "markdown";
  }
  return "plaintext";
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let size = bytes / 1024;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${size.toFixed(size >= 10 ? 1 : 2)} ${units[unitIndex]}`;
}

function labelForPersona(persona: string): string {
  if (persona === "agf-architect") {
    return "AGF";
  }
  return persona;
}

function fallbackPersonaColor(persona: string): string {
  if (persona === "claude") {
    return "var(--persona-claude)";
  }
  if (persona === "codex") {
    return "var(--persona-codex)";
  }
  return "var(--text-secondary)";
}

function sendLabelForSessions(
  sessions: AgentSession[],
  defaultSessionId: string | null,
  fileCount?: number,
  isMcpSendBack = false
): string {
  const prefix = fileCount && fileCount > 1 ? `Send ${fileCount} files to` : isMcpSendBack ? "Send back to" : "Send to";
  if (sessions.length === 0) {
    return `${prefix} Agent`;
  }
  if (sessions.length === 1) {
    return `${prefix} ${agentSessionLabel(sessions[0])}`;
  }
  const defaultSession = sessions.find((session) => session.id === defaultSessionId);
  return defaultSession ? `${prefix} ${agentSessionLabel(defaultSession)}` : `${prefix}...`;
}

function agentSessionLabel(session: AgentSession): string {
  return `${session.persona}·${session.agent}`;
}

function attachmentToAgentSession(attachment: SessionAttachment): AgentSession {
  return {
    id: attachment.session_id,
    source: "mcp",
    persona: attachment.persona,
    agent: attachment.agent,
    project: attachment.project,
    connected_at: attachment.attached_at,
    last_active: attachment.attached_at,
    is_live: true,
    attached_paths: []
  };
}

function pathsFromDataTransfer(dataTransfer: DataTransfer | null): string[] {
  if (!dataTransfer) {
    return [];
  }
  return Array.from(dataTransfer.files)
    .map((file) => (file as File & { path?: string }).path)
    .filter((path): path is string => Boolean(path));
}

async function conflictStrategyForTarget(
  target: "project" | "archive",
  filename: string,
  project: string | undefined,
  openConflictDialog: (filename: string, target: string) => Promise<ConflictStrategy>
): Promise<ConflictStrategy> {
  const exists = await targetFileExists(target, filename, project);
  if (!exists) {
    return "keep_both";
  }
  const targetLabel = target === "archive" ? "Archive" : (project ?? "the project");
  return openConflictDialog(filename, targetLabel);
}

function moveSelection(
  direction: 1 | -1,
  files: FileMetadata[],
  selectedPath: string | null,
  setSelectedPath: (path: string | null) => void
) {
  if (files.length === 0) {
    return;
  }
  const current = files.findIndex((file) => file.path === selectedPath);
  const next = current === -1 ? 0 : Math.min(files.length - 1, Math.max(0, current + direction));
  setSelectedPath(files[next]?.path ?? null);
}

function filterFilesByQuery(files: FileMetadata[], query: string): FileMetadata[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) {
    return files;
  }
  return files.filter((file) => file.name.toLowerCase().includes(normalized));
}

function selectionFromSource(selection: SourceSelection | null): AnnotationSelection {
  return selection
    ? {
      kind: "text",
      rect: selection.bounds,
      startOffset: selection.startOffset,
      endOffset: selection.endOffset
    }
    : null;
}

async function loadCommentsForPath(path: string, setComments: (comments: Comment[]) => void) {
  try {
    const sidecar = await loadSidecar(path);
    setComments(sidecar.comments ?? []);
  } catch {
    setComments([]);
  }
}

function markReviewStateLocally(
  path: string,
  reviewState: FileMetadata["review_state"],
  setFiles: Dispatch<SetStateAction<FileMetadata[]>>,
  setProjectFiles: Dispatch<SetStateAction<FileMetadata[]>>,
  setArchiveFiles: Dispatch<SetStateAction<FileMetadata[]>>,
  setPinnedFiles: Dispatch<SetStateAction<FileMetadata[]>>
) {
  const update = (files: FileMetadata[]) =>
    files.map((file) => (file.path === path ? { ...file, review_state: reviewState } : file));
  setFiles(update);
  setProjectFiles(update);
  setArchiveFiles(update);
  setPinnedFiles(update);
}

const REVIEW_STATES: FileMetadata["review_state"][] = ["unread", "reviewed", "needs-work", "approved"];

function reviewStateLabel(state: FileMetadata["review_state"]): string {
  if (state === "needs-work") {
    return "Needs work";
  }
  return state.slice(0, 1).toUpperCase() + state.slice(1);
}

function emptyStateForMode(mode: "inbox" | "project" | "archive" | "pinned" | "recents"): string {
  if (mode === "pinned") {
    return "No pinned artifacts\n⌘P on any file to pin";
  }
  if (mode === "archive") {
    return "Empty archive\nMove artifacts here when you're done";
  }
  if (mode === "project") {
    return "Empty project\nDrag inbox files to this project";
  }
  return "Empty inbox";
}

function isTextInput(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false;
  }
  return (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT" ||
    target.isContentEditable ||
    Boolean(target.closest(".cm-editor"))
  );
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function directoryName(path: string): string {
  return path.split(/[\\/]/).slice(0, -1).join("/") || ".";
}

function formatTime(epochSeconds: number): string {
  if (!epochSeconds) {
    return "--:--";
  }
  const date = new Date(epochSeconds * 1000);
  return `${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}`;
}

function formatTimeTooltip(epochSeconds: number): string {
  if (!epochSeconds) {
    return "Unknown modified time";
  }
  const date = new Date(epochSeconds * 1000);
  return `Modified ${date.toLocaleString()}`;
}

function currentTime(): string {
  const date = new Date();
  return `${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}:${date
    .getSeconds()
    .toString()
    .padStart(2, "0")}`;
}

type NewFileDialogProps = {
  onCancel: () => void;
  onCreate: (name: string) => void;
};

function NewFileDialog({ onCancel, onCreate }: NewFileDialogProps) {
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  useFocusTrap(dialogRef, onCancel);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const submit = () => {
    const trimmed = value.trim();
    if (trimmed) {
      onCreate(trimmed);
    }
  };

  return (
    <div className="palette-backdrop" onMouseDown={onCancel}>
      <div
        ref={dialogRef}
        className="rename-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="New file"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>New file</header>
        <input
          ref={inputRef}
          value={value}
          placeholder="File name"
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              submit();
            } else if (event.key === "Escape") {
              event.preventDefault();
              onCancel();
            }
          }}
        />
        <p style={{ margin: "4px 0 0", fontSize: 12, color: "var(--text-secondary)" }}>
          Saved as <code>.md</code> in Drafts
        </p>
        <footer>
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" className="primary" disabled={!value.trim()} onClick={submit}>Create</button>
        </footer>
      </div>
    </div>
  );
}
