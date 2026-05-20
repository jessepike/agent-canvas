import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, MouseEvent as ReactMouseEvent } from "react";
import { listen, TauriEvent } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { RenderedView } from "./components/RenderedView";
import { SourceView, type SourceFormat, type SourceViewHandle } from "./components/SourceView";
import {
  addAgentSession,
  archiveFile,
  copyPathsToInbox,
  copyTextToClipboard,
  deleteFile,
  deleteProjectIfEmpty,
  exportFileTo,
  getDefaultActionVerb,
  getBootstrapInfo,
  getProjectDefaultAgent,
  listAgentSessions,
  listArchive,
  listInbox,
  listPinned,
  listProjectFiles,
  listPersonas,
  listProjectCounts,
  listProjects,
  loadSidecar,
  moveFileToArchive,
  moveFileToProject,
  openDocument,
  parseDocument,
  readBinaryArtifact,
  renameFile,
  revealInFinder,
  reloadPersonaRegistry,
  renameProject,
  sendMultiToClipboard,
  sendToClipboard,
  setDefaultActionVerb,
  setProjectDefaultAgent,
  targetFileExists,
  togglePin,
  writeDocument,
  type BootstrapInfo,
  type AgentSession,
  type ConflictStrategy,
  type FileMetadata,
  type PersonaRegistry
} from "./ipc";
import type { Block } from "./types/blocks";
import type { BaseSnapshot } from "./types/blocks";
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

type AnnotationSelection = {
  rect: DOMRect;
} | null;

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
  const [mode, setMode] = useState<"inbox" | "project" | "archive" | "pinned">("inbox");
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
  const sourceViewRef = useRef<SourceViewHandle | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [artifact, setArtifact] = useState<OpenArtifact | null>(null);
  const [editMode, setEditMode] = useState(false);
  const [sourceMode, setSourceMode] = useState(false);
  const [jsonViewMode, setJsonViewMode] = useState<"source" | "tree">("source");
  const [conflict, setConflict] = useState(false);
  const [mergeConflict, setMergeConflict] = useState<MergeConflict>(null);
  const [annotationSelection, setAnnotationSelection] = useState<AnnotationSelection>(null);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [handoffToast, setHandoffToast] = useState<string | null>(null);
  const [sendPopoverOpen, setSendPopoverOpen] = useState(false);
  const [showAgentPicker, setShowAgentPicker] = useState(false);
  const [agentPickerOpen, setAgentPickerOpen] = useState(false);
  const [selectedAgentId, setSelectedAgentId] = useState<string | null>(null);
  const [defaultAgentId, setDefaultAgentId] = useState<string | null>(null);
  const [defaultActionVerb, setDefaultActionVerbState] = useState("Review");
  const [sendActionVerb, setSendActionVerb] = useState("Review");
  const [customActionVerb, setCustomActionVerb] = useState("");
  const [sendNote, setSendNote] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [isOpening, setIsOpening] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [arrivedPaths, setArrivedPaths] = useState<Set<string>>(new Set());
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
  const [pendingSendPath, setPendingSendPath] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const currentProjectKey = currentProject ?? "Inbox";

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [
        nextBootstrap,
        nextFiles,
        nextProjects,
        nextProjectCounts,
        nextPersonas,
        nextSessions,
        nextDefaultVerb,
        nextPinned,
        nextArchive
      ] = await Promise.all([
        getBootstrapInfo(),
        listInbox(),
        listProjects(),
        listProjectCounts(),
        listPersonas(),
        listAgentSessions(),
        getDefaultActionVerb(),
        listPinned(),
        listArchive()
      ]);
      setBootstrap(nextBootstrap);
      setFiles(nextFiles);
      setProjects(nextProjects);
      setProjectCounts(nextProjectCounts);
      setPersonas(nextPersonas);
      setSessions(nextSessions);
      setDefaultActionVerbState(nextDefaultVerb);
      setPinnedFiles(nextPinned);
      setArchiveFiles(nextArchive);
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

  const selectedFile = useMemo(
    () => [...files, ...projectFiles, ...archiveFiles, ...pinnedFiles].find((file) => file.path === selectedPath) ?? null,
    [archiveFiles, files, pinnedFiles, projectFiles, selectedPath]
  );
  const selectedFileMetadatas = useMemo(() => {
    const byPath = new Map([...files, ...projectFiles, ...archiveFiles, ...pinnedFiles].map((file) => [file.path, file]));
    return [...selectedPaths].map((path) => byPath.get(path)).filter((file): file is FileMetadata => Boolean(file));
  }, [archiveFiles, files, pinnedFiles, projectFiles, selectedPaths]);
  const multiSelectActive = selectedPaths.size > 1;
  const filteredFiles = useMemo(() => filterFilesByQuery(files, mode === "inbox" ? searchQuery : ""), [files, mode, searchQuery]);
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
    return filteredFiles;
  }, [filteredArchiveFiles, filteredFiles, filteredPinnedFiles, filteredProjectFiles, mode]);
  const sendButtonLabel = useMemo(
    () => sendLabelForSessions(sessions, defaultAgentId, multiSelectActive ? selectedPaths.size : undefined),
    [defaultAgentId, multiSelectActive, selectedPaths.size, sessions]
  );
  const defaultAgent = useMemo(
    () => sessions.find((session) => session.id === defaultAgentId) ?? null,
    [defaultAgentId, sessions]
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
    function handleFocus() {
      void refresh();
      void reloadOpenArtifact();
    }

    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, [refresh, reloadOpenArtifact]);

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

  const applyAnnotationFormat = useCallback((format: SourceFormat) => {
    sourceViewRef.current?.applyFormat(format);
  }, []);

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
    // Pasteboard handoff works regardless of declared agent sessions. The agent
    // panel is metadata for routing convenience, not a gate on copy-to-clipboard.
    const defaultIsPreset = ACTION_VERBS.includes(defaultActionVerb as (typeof ACTION_VERBS)[number]);
    const defaultSession = sessions.find((session) => session.id === defaultAgentId) ?? null;
    const nextSelectedAgent = defaultSession?.id ?? (sessions.length === 1 ? sessions[0]?.id ?? null : null);
    setSelectedAgentId(nextSelectedAgent);
    setShowAgentPicker(sessions.length > 0 && (forceAgentPicker || (sessions.length > 1 && !defaultSession)));
    setSendActionVerb(defaultIsPreset ? defaultActionVerb : "Custom");
    setCustomActionVerb(defaultIsPreset ? "" : defaultActionVerb);
    setSendNote("");
    setSendPopoverOpen(true);
  }, [artifact, defaultActionVerb, defaultAgentId, selectedPaths.size, sessions]);

  const sendCurrentArtifact = useCallback(async (actionVerb: string, note: string) => {
    if (!artifact && selectedPaths.size <= 1) {
      return;
    }
    const verb = actionVerb.trim() || "Review";
    try {
      // Only require agent picker when multiple sessions exist and none is targeted.
      // Zero sessions = pasteboard payload uses generic "Agent" framing; still copies.
      if (sessions.length > 1) {
        const targetAgent = sessions.find((session) => session.id === selectedAgentId) ?? defaultAgent;
        if (!targetAgent) {
          setShowAgentPicker(true);
          return;
        }
      }
      const agent = sessions.find((session) => session.id === selectedAgentId) ?? defaultAgent;
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
    if (sessions.length === 0) {
      setShowSessionForm(true);
      return;
    }
    setAgentPickerOpen(true);
  }, [sessions.length]);

  const openConflictDialog = useCallback(
    (filename: string, target: string): Promise<ConflictStrategy> => {
      return new Promise((resolve) => {
        setConflictDialog({ filename, target, resolve });
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
        const copied = await copyPathsToInbox(paths);
        setArrivedPaths((current) => new Set([...current, ...copied.map((file) => file.path)]));
        const first = copied[0];
        if (first) {
          const message = `+ ${first.name}`;
          setHandoffToast(message);
          window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        }
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [refresh]
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
        const message = `Moved ${moved.name} → ${project}`;
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
        const message = `Moved ${moved.name} → ${project}`;
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
        const message = `Moved ${moved.name} → Archive`;
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
        const message = `Moved ${moved.name} → Archive`;
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

  const deleteArtifact = useCallback(
    async (file: FileMetadata) => {
      if (!window.confirm(`Delete ${file.name}? This permanently removes the file from disk.`)) {
        return;
      }
      try {
        await deleteFile(file.path);
        if (artifact?.path === file.path) {
          setArtifact(null);
          setSelectedPath(null);
          setSelectedPaths(new Set());
        }
        setFileMenu(null);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact?.path, refresh]
  );

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
      { section: "ACTIONS", label: sendButtonLabel, run: openSendPopover },
      { section: "ACTIONS", label: "Toggle Pin", run: toggleCurrentPin },
      { section: "ACTIONS", label: "Archive", run: archiveCurrent },
      { section: "ACTIONS", label: "Switch Agent Default...", run: switchAgentDefault },
      { section: "COMMANDS", label: "Reload Persona Registry", run: reloadPersonas },
      { section: "COMMANDS", label: "Open Project", run: () => projects[0] && void openProject(projects[0]) }
    ];
    const fileItems = files.map((file) => ({
      section: "FILES",
      label: file.name,
      run: () => void openArtifact(file)
    }));
    const allItems = [...actions, ...fileItems];
    const query = paletteQuery.trim().toLowerCase();
    return query ? allItems.filter((item) => item.label.toLowerCase().includes(query)) : allItems;
  }, [archiveCurrent, files, openArtifact, openProject, openSendPopover, paletteQuery, projects, reloadPersonas, sendButtonLabel, switchAgentDefault, toggleCurrentPin]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
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
            </button>
            <div
              className={`file-list ${dropTarget === "inbox" ? "drop-target" : ""}`}
              onDragEnter={() => setDropTarget("inbox")}
              onDragLeave={() => setDropTarget((current) => (current === "inbox" ? null : current))}
            >
              {filteredFiles.length === 0 ? (
                <div className="empty-list">
                  {searchQuery && mode === "inbox" ? "No matching artifacts" : "Empty inbox"}
                  <span>{bootstrap?.inbox_dir ?? "~/iCloud/AgentCanvas/Inbox"}</span>
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
                    <span className="arrival-dot" />
                    <span className="file-name">
                      {file.pinned ? <span className="pin-star" title="Pinned">★ </span> : null}
                      {file.name}
                    </span>
                    <span
                      className="badge persona-badge"
                      style={{ color: personaColors.get(file.persona) ?? fallbackPersonaColor(file.persona) }}
                    >
                      {labelForPersona(file.persona)}
                    </span>
                    <span className="file-time">{formatTime(file.mtime)}</span>
                  </button>
                ))
              )}
            </div>
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
                {visibleFiles.map((file) => (
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
                    <span>
                      {file.pinned ? <span className="pin-star" title="Pinned">★ </span> : null}
                      {file.name}
                    </span>
                    <small>{formatTime(file.mtime)}</small>
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
                <button type="button" onClick={() => void openFileDialog()} aria-label="Open file">
                  +
                </button>
              </div>
              <div className="breadcrumb">
                {mode === "archive" ? "Archive" : mode === "pinned" ? "★ Pinned" : mode === "project" ? (currentProject ?? "Project") : "Inbox"}
                <span>/</span> <strong>{selectedFile?.name ?? "Select a file"}</strong>
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
            {editMode && artifact?.kind === "md" ? (
              <div className="edit-fallback-banner" role="status">
                Rendered-view editing lands in v0.3 — using source editor
              </div>
            ) : null}
            {personas?.warning ? <div className="registry-warning">{personas.warning}</div> : null}
            {savedAt ? <div className="saved-toast">Saved {savedAt}</div> : null}
            {handoffToast ? <div className="handoff-toast">{handoffToast}</div> : null}
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
                <section className="source-panel" aria-label="Source editor">
                  <SourceView
                    ref={sourceViewRef}
                    key={artifact.kind}
                    language={sourceLanguageForArtifact(artifact.kind)}
                    value={artifact.source}
                    onChange={updateSource}
                    onSave={saveArtifact}
                    onSelectionBoundsChange={(bounds) => setAnnotationSelection(bounds ? { rect: bounds } : null)}
                  />
                </section>
              ) : artifact.kind === "md" ? (
                <section className="rendered-panel" aria-label="Rendered Markdown">
                  <RenderedView blocks={artifact.blocks} />
                </section>
              ) : artifact.kind === "html" ? (
                <section className="html-panel" aria-label="Rendered HTML">
                  <iframe title={fileName(artifact.path)} sandbox="allow-same-origin" srcDoc={artifact.source} />
                </section>
              ) : artifact.kind === "json" && parsedJson ? (
                <section className="json-panel" aria-label="JSON tree">
                  <JsonTree value={parsedJson} name={fileName(artifact.path)} />
                </section>
              ) : artifact.kind === "txt" ? (
                <section className="source-panel" aria-label="Text source">
                  <SourceView
                    ref={sourceViewRef}
                    key={artifact.kind}
                    language="plaintext"
                    value={artifact.source}
                    onChange={updateSource}
                    onSave={saveArtifact}
                    onSelectionBoundsChange={(bounds) => setAnnotationSelection(bounds ? { rect: bounds } : null)}
                  />
                </section>
              ) : artifact.kind === "png" ? (
                <section className="image-panel" aria-label="PNG image">
                  <div className="image-frame">
                    <img src={artifact.dataUrl} alt={fileName(artifact.path)} />
                    <p>{formatBytes(artifact.size ?? 0)}</p>
                  </div>
                </section>
              ) : artifact.kind === "pdf" ? (
                <section className="pdf-panel" aria-label="PDF document">
                  <iframe title={fileName(artifact.path)} sandbox="allow-same-origin" src={artifact.dataUrl} />
                </section>
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
            {annotationSelection && artifact?.kind === "md" && editMode ? (
              <AnnotationToolbar selection={annotationSelection} onFormat={applyAnnotationFormat} />
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
                sessions={sessions}
                showAgentPicker={showAgentPicker}
                selectedAgentId={selectedAgentId}
                onSelectedAgentChange={setSelectedAgentId}
                onCancel={() => setSendPopoverOpen(false)}
                onSend={() => void sendCurrentArtifact(sendActionVerb === "Custom" ? customActionVerb : sendActionVerb, sendNote)}
              />
            ) : null}
          </section>
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
                    key={session.id}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setAgentMenu({ x: event.clientX, y: event.clientY, session });
                    }}
                  >
                    <div className="agent-card-top">
                      <span
                        className="badge persona-badge"
                        style={{ color: personaColors.get(session.persona) ?? fallbackPersonaColor(session.persona) }}
                      >
                        {labelForPersona(session.persona)}
                      </span>
                      <span className="backbone-tag">{session.backbone}</span>
                    </div>
                    <div className="agent-context">[{session.context || "current"}]</div>
                    {session.id === defaultAgentId ? <div className="agent-default">default for {currentProjectKey}</div> : null}
                  </article>
                ))}
              </div>
            </aside>
          )}
        {paletteOpen ? (
          <div className="palette-backdrop" onMouseDown={() => setPaletteOpen(false)}>
            <section className="palette" onMouseDown={(event) => event.stopPropagation()}>
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
              <button className="danger-item" type="button" onClick={() => void deleteArtifact(fileMenu.file)}>
                Delete...
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
        {agentPickerOpen ? (
          <AgentPickerDialog
            project={currentProjectKey}
            sessions={sessions}
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
  selection: { rect: DOMRect };
  onFormat: (format: SourceFormat) => void;
};

function AnnotationToolbar({ selection, onFormat }: AnnotationToolbarProps) {
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
  return (
    <div className="palette-backdrop" onMouseDown={onCancel}>
      <section
        className="merge-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="merge-dialog-title"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
          trapFocusWithin(event);
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
  const firstRadioRef = useRef<HTMLInputElement | null>(null);
  const selectedSession = sessions.find((session) => session.id === selected) ?? null;

  useEffect(() => {
    firstRadioRef.current?.focus();
  }, []);

  const confirm = () => {
    if (selectedSession) {
      onConfirm(selectedSession);
    }
  };

  return (
    <div className="palette-backdrop" onMouseDown={onCancel}>
      <section
        className="rename-dialog agent-picker-dialog"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          } else if (event.key === "Enter") {
            event.preventDefault();
            confirm();
          }
          trapFocusWithin(event);
        }}
      >
        <header>Default Agent for {project}</header>
        <fieldset>
          {sessions.map((session, index) => (
            <label key={session.id}>
              <input
                ref={index === 0 ? firstRadioRef : undefined}
                type="radio"
                name="default-agent"
                value={session.id}
                checked={selected === session.id}
                onChange={() => setSelected(session.id)}
              />
              <span>{agentSessionLabel(session)}</span>
              <small>[{session.context || "current"}]</small>
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
      <div className="rename-dialog" onMouseDown={(event) => event.stopPropagation()}>
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
      <div className="rename-dialog" onMouseDown={(event) => event.stopPropagation()}>
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

  useEffect(() => {
    noteRef.current?.focus();
  }, []);

  return (
    <div className="send-popover-backdrop" onMouseDown={onCancel}>
      <form
        className="send-popover"
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

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <form
        className="send-popover project-dialog"
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
          trapFocusWithin(event);
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

  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  return (
    <div className="dialog-backdrop" onMouseDown={onCancel}>
      <section
        className="send-popover project-dialog"
        onMouseDown={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            onCancel();
          }
          trapFocusWithin(event);
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

function sendLabelForSessions(sessions: AgentSession[], defaultSessionId: string | null, fileCount?: number): string {
  const prefix = fileCount && fileCount > 1 ? `Send ${fileCount} files to` : "Send to";
  if (sessions.length === 0) {
    return `${prefix} Agent`;
  }
  if (sessions.length === 1) {
    return `${prefix} ${agentSessionLabel(sessions[0])}`;
  }
  const defaultSession = sessions.find((session) => session.id === defaultSessionId);
  return defaultSession ? `${prefix} ${agentSessionLabel(defaultSession)}` : `${prefix} Agent`;
}

function agentSessionLabel(session: AgentSession): string {
  return `${session.persona}·${session.backbone}`;
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

function trapFocusWithin(event: ReactKeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") {
    return;
  }
  const focusable = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
    )
  );
  if (focusable.length === 0) {
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
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

function currentTime(): string {
  const date = new Date();
  return `${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}:${date
    .getSeconds()
    .toString()
    .padStart(2, "0")}`;
}
