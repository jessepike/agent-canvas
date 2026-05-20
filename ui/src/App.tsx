import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { listen, TauriEvent } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { RenderedView } from "./components/RenderedView";
import { SourceView } from "./components/SourceView";
import {
  addAgentSession,
  archiveFile,
  copyPathsToInbox,
  copyTextToClipboard,
  deleteFile,
  deleteProjectIfEmpty,
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
  moveFileToArchive,
  moveFileToProject,
  openDocument,
  parseDocument,
  revealInFinder,
  reloadPersonaRegistry,
  renameProject,
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
import "./styles.css";

type OpenArtifact = {
  path: string;
  source: string;
  baseHash: number[];
  blocks: Block[];
  dirty: boolean;
  kind: "md" | "html" | "unsupported";
};

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
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [artifact, setArtifact] = useState<OpenArtifact | null>(null);
  const [editMode, setEditMode] = useState(false);
  const [sourceMode, setSourceMode] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [handoffToast, setHandoffToast] = useState<string | null>(null);
  const [sendPopoverOpen, setSendPopoverOpen] = useState(false);
  const [showAgentPicker, setShowAgentPicker] = useState(false);
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
    () => sendLabelForSessions(sessions, defaultAgentId),
    [defaultAgentId, sessions]
  );
  const defaultAgent = useMemo(
    () => sessions.find((session) => session.id === defaultAgentId) ?? null,
    [defaultAgentId, sessions]
  );

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
    setIsOpening(true);
    setConflict(false);
    setError(null);
    setSavedAt(null);

    try {
      // Detect binary / unsupported extensions BEFORE attempting to read as UTF-8.
      // openDocument expects text content; PDFs, images, etc. will throw on read.
      if (isBinaryExtension(file.extension) || (!markdownExtension(file.extension) && !htmlExtension(file.extension))) {
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
        return;
      }

      const opened = await openDocument(file.path);
      const kind = markdownExtension(file.extension) ? "md" : "html";
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

  const reloadOpenArtifact = useCallback(async () => {
    if (!artifact || artifact.dirty) {
      return;
    }

    try {
      const opened = await openDocument(artifact.path);
      const blocks = artifact.kind === "md" ? await parseDocument(opened.source) : [];
      setArtifact({
        ...artifact,
        source: opened.source,
        baseHash: opened.base_hash,
        blocks,
        dirty: false
      });
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
      const result = await writeDocument(artifact.path, artifact.source, artifact.baseHash);
      const blocks = artifact.kind === "md" ? await parseDocument(artifact.source) : [];
      setArtifact({ ...artifact, baseHash: result.new_hash, blocks, dirty: false });
      const stamp = currentTime();
      setSavedAt(stamp);
      window.setTimeout(() => setSavedAt((current) => (current === stamp ? null : current)), 3000);
      await refresh();
    } catch (caught) {
      const message = caught instanceof Error ? caught.message : String(caught);
      if (message.startsWith("CONFLICT:")) {
        setConflict(true);
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

  const openSendPopover = useCallback((forceAgentPicker = false) => {
    if (!artifact) {
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
  }, [artifact, defaultActionVerb, defaultAgentId, sessions]);

  const sendCurrentArtifact = useCallback(async (actionVerb: string, note: string) => {
    if (!artifact) {
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
      await sendToClipboard({
        path: artifact.path,
        contents: artifact.source,
        note: note.trim() ? note : null,
        action_verb: verb
      });
      await setDefaultActionVerb(verb);
      setDefaultActionVerbState(verb);
      setSendPopoverOpen(false);
      const message = "Copied to clipboard — paste into your Claude / Codex session";
      setHandoffToast(message);
      window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3500);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact, defaultAgent, selectedAgentId, sessions]);

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
    const options = sessions.map((session, index) => `${index + 1}. ${agentSessionLabel(session)}`).join("\n");
    const answer = window.prompt(`Switch default agent for ${currentProjectKey}:\n${options}`, "1");
    const index = answer ? Number.parseInt(answer, 10) - 1 : Number.NaN;
    const session = sessions[index];
    if (session) {
      await setDefaultAgentForProject(session);
    }
  }, [currentProjectKey, sessions, setDefaultAgentForProject]);

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
        const strategy = await conflictStrategyForTarget("project", file.name, project);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToProject(path, project, strategy);
        const message = `Moved ${moved.name} → ${project}`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        setArtifact(null);
        setSelectedPath(null);
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
        const strategy = await conflictStrategyForTarget("project", file.name, project);
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
        const strategy = await conflictStrategyForTarget("archive", file.name);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToArchive(path, strategy);
        const message = `Moved ${moved.name} → Archive`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        setArtifact(null);
        setSelectedPath(null);
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
        const strategy = await conflictStrategyForTarget("archive", file.name);
        if (strategy === "cancel") {
          return;
        }
        const moved = await moveFileToArchive(file.path, strategy);
        const message = `Moved ${moved.name} → Archive`;
        setHandoffToast(message);
        window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 2500);
        setArtifact(null);
        setSelectedPath(null);
        setFileMenu(null);
        await refresh();
      } catch (caught) {
        setError(caught instanceof Error ? caught.message : String(caught));
      }
    },
    [artifact?.path, refresh]
  );

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
        setSelectedPath(null);
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
                      setSelectedPath(null);
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
                    } ${file.pinned ? "pinned" : ""}`}
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
                    onClick={() => {
                      setMode("inbox");
                      setCurrentProject(null);
                      void openArtifact(file);
                    }}
                  >
                    <span className="arrival-dot" />
                    <span className="file-name">
                      {file.pinned ? <span className="pin-star" title="Pinned">★ </span> : null}
                      {file.name}
                    </span>
                    <span className={`badge persona-badge badge-${file.persona}`}>{labelForPersona(file.persona)}</span>
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
                    className={`middle-file ${file.path === selectedPath ? "selected" : ""} ${file.pinned ? "pinned" : ""}`}
                    key={file.path}
                    type="button"
                    onContextMenu={(event) => {
                      event.preventDefault();
                      setFileMenu({ x: event.clientX, y: event.clientY, file });
                    }}
                    onClick={() => void openArtifact(file)}
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
                <button
                  type="button"
                  onClick={() =>
                    artifact?.kind === "html"
                      ? setSourceMode((current) => !current)
                      : setEditMode((current) => !current)
                  }
                  disabled={!artifact}
                >
                  {artifact?.kind === "html" ? (sourceMode ? "Render" : "View Source") : editMode ? "Preview" : "Edit"}
                </button>
                <button className="primary" type="button" onClick={() => openSendPopover()} disabled={!artifact}>
                  {sendButtonLabel}
                </button>
                <button type="button" onClick={() => void saveArtifact()} disabled={!artifact?.dirty || isSaving}>
                  {isSaving ? "Saving" : "Save"}
                </button>
              </div>
            </div>
            {conflict ? (
              <div className="conflict-banner" role="alert">
                {fileName(artifact?.path ?? "File")} changed on disk since open. Save aborted — reload or copy your edit
                elsewhere.
              </div>
            ) : null}
            {personas?.warning ? <div className="registry-warning">{personas.warning}</div> : null}
            {savedAt ? <div className="saved-toast">Saved {savedAt}</div> : null}
            {handoffToast ? <div className="handoff-toast">{handoffToast}</div> : null}
            {artifact ? (
              editMode || sourceMode ? (
                <section className="source-panel" aria-label="Source editor">
                  <SourceView value={artifact.source} onChange={updateSource} onSave={saveArtifact} />
                </section>
              ) : artifact.kind === "md" ? (
                <section className="rendered-panel" aria-label="Rendered Markdown">
                  <RenderedView blocks={artifact.blocks} />
                </section>
              ) : artifact.kind === "html" ? (
                <section className="html-panel" aria-label="Rendered HTML">
                  <iframe title={fileName(artifact.path)} sandbox="allow-same-origin" srcDoc={artifact.source} />
                </section>
              ) : (
                <article className="document placeholder-document">
                  <p className="eyebrow">Unsupported artifact</p>
                  <h1>{fileName(artifact.path)}</h1>
                  <p>This v0 viewer supports Markdown and HTML only.</p>
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
                      <span className={`badge persona-badge badge-${session.persona}`}>{labelForPersona(session.persona)}</span>
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
    </main>
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

function markdownExtension(extension: string): boolean {
  return extension === "md" || extension === "markdown";
}

function htmlExtension(extension: string): boolean {
  return extension === "html" || extension === "htm";
}

function isBinaryExtension(extension: string): boolean {
  const ext = extension.toLowerCase();
  return (
    ext === "pdf" ||
    ext === "png" ||
    ext === "jpg" ||
    ext === "jpeg" ||
    ext === "gif" ||
    ext === "webp" ||
    ext === "svg" ||
    ext === "ico" ||
    ext === "zip" ||
    ext === "tar" ||
    ext === "gz" ||
    ext === "mp3" ||
    ext === "mp4" ||
    ext === "mov" ||
    ext === "wav"
  );
}

function labelForPersona(persona: string): string {
  if (persona === "agf-architect") {
    return "AGF";
  }
  return persona;
}

function sendLabelForSessions(sessions: AgentSession[], defaultSessionId: string | null): string {
  if (sessions.length === 0) {
    return "Send to Agent";
  }
  if (sessions.length === 1) {
    return `Send to ${agentSessionLabel(sessions[0])}`;
  }
  const defaultSession = sessions.find((session) => session.id === defaultSessionId);
  return defaultSession ? `Send to ${agentSessionLabel(defaultSession)}` : "Send to Agent";
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
  project?: string
): Promise<ConflictStrategy> {
  const exists = await targetFileExists(target, filename, project);
  if (!exists) {
    return "keep_both";
  }
  const answer = window.prompt(`Replace ${filename}? Type "replace", "keep", or "cancel".`, "keep");
  if (answer?.toLowerCase() === "replace") {
    return "replace";
  }
  if (answer?.toLowerCase() === "keep") {
    return "keep_both";
  }
  return "cancel";
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
