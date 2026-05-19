import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { RenderedView } from "./components/RenderedView";
import { SourceView } from "./components/SourceView";
import {
  addAgentSession,
  archiveFile,
  getBootstrapInfo,
  listAgentSessions,
  listInbox,
  listPersonas,
  listProjects,
  openDocument,
  parseDocument,
  sendToClipboard,
  togglePin,
  writeDocument,
  type BootstrapInfo,
  type AgentSession,
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

export default function App() {
  const [bootstrap, setBootstrap] = useState<BootstrapInfo | null>(null);
  const [files, setFiles] = useState<FileMetadata[]>([]);
  const [projects, setProjects] = useState<string[]>([]);
  const [personas, setPersonas] = useState<PersonaRegistry | null>(null);
  const [sessions, setSessions] = useState<AgentSession[]>([]);
  const [showSessionForm, setShowSessionForm] = useState(false);
  const [sessionPersona, setSessionPersona] = useState("cto");
  const [sessionBackbone, setSessionBackbone] = useState("claude");
  const [sessionContext, setSessionContext] = useState("AGRC");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [paletteQuery, setPaletteQuery] = useState("");
  const [paletteIndex, setPaletteIndex] = useState(0);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [artifact, setArtifact] = useState<OpenArtifact | null>(null);
  const [editMode, setEditMode] = useState(false);
  const [sourceMode, setSourceMode] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [handoffToast, setHandoffToast] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isOpening, setIsOpening] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [arrivedPaths, setArrivedPaths] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextBootstrap, nextFiles, nextProjects, nextPersonas, nextSessions] = await Promise.all([
        getBootstrapInfo(),
        listInbox(),
        listProjects(),
        listPersonas(),
        listAgentSessions()
      ]);
      setBootstrap(nextBootstrap);
      setFiles(nextFiles);
      setProjects(nextProjects);
      setPersonas(nextPersonas);
      setSessions(nextSessions);
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
    () => files.find((file) => file.path === selectedPath) ?? null,
    [files, selectedPath]
  );

  const openArtifact = useCallback(async (file: FileMetadata) => {
    setSelectedPath(file.path);
    setIsOpening(true);
    setConflict(false);
    setError(null);
    setSavedAt(null);

    try {
      const opened = await openDocument(file.path);
      const kind = markdownExtension(file.extension) ? "md" : htmlExtension(file.extension) ? "html" : "unsupported";
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
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsOpening(false);
    }
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

  const sendCurrentArtifact = useCallback(async () => {
    if (!artifact) {
      return;
    }
    const note = window.prompt("Optional note for Claude") ?? "";
    try {
      await sendToClipboard({
        path: artifact.path,
        project: projectForPath(artifact.path),
        persona: selectedFile?.persona ?? "claude",
        contents: artifact.source,
        note: note.trim() ? note : null
      });
      const message = "Copied to clipboard — paste into your Claude / Codex session";
      setHandoffToast(message);
      window.setTimeout(() => setHandoffToast((current) => (current === message ? null : current)), 3500);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }, [artifact, selectedFile]);

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

  const paletteItems = useMemo(() => {
    const actions = [
      { section: "ACTIONS", label: "Send to Claude", run: sendCurrentArtifact },
      { section: "ACTIONS", label: "Toggle Pin", run: toggleCurrentPin },
      { section: "ACTIONS", label: "Archive", run: archiveCurrent },
      { section: "COMMANDS", label: "Open Project", run: () => undefined }
    ];
    const fileItems = files.map((file) => ({
      section: "FILES",
      label: file.name,
      run: () => void openArtifact(file)
    }));
    const allItems = [...actions, ...fileItems];
    const query = paletteQuery.trim().toLowerCase();
    return query ? allItems.filter((item) => item.label.toLowerCase().includes(query)) : allItems;
  }, [archiveCurrent, files, openArtifact, paletteQuery, sendCurrentArtifact, toggleCurrentPin]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
        event.preventDefault();
        void sendCurrentArtifact();
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen(true);
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [sendCurrentArtifact]);

  useEffect(() => {
    if (paletteIndex >= paletteItems.length) {
      setPaletteIndex(0);
    }
  }, [paletteIndex, paletteItems.length]);

  return (
    <main className="desktop">
      <section className="window-shell" aria-label="AgentCanvas">
        <header className="titlebar">
          <div className="traffic-lights" aria-hidden="true">
            <span className="tl tl-close" />
            <span className="tl tl-min" />
            <span className="tl tl-max" />
          </div>
          <div className="titlebar-title">AgentCanvas</div>
          <button className="titlebar-action" type="button" onClick={refresh} disabled={isLoading}>
            {isLoading ? "Scanning" : "Rescan"}
          </button>
        </header>
        <div className="main-shell">
          <aside className="sidebar">
            <div className="sidebar-header">
              <label className="search">
                <span>Search</span>
                <input placeholder="Search artifacts" />
              </label>
            </div>
            <div className="section-header">
              <span className="section-label">Inbox</span>
              <span className="count">{files.length}</span>
            </div>
            <div className="file-list">
              {files.length === 0 ? (
                <div className="empty-list">
                  Empty inbox
                  <span>{bootstrap?.inbox_dir ?? "~/iCloud/AgentCanvas/Inbox"}</span>
                </div>
              ) : (
                files.map((file) => (
                  <button
                    className={`file-row ${file.path === selectedPath ? "selected" : ""} ${
                      arrivedPaths.has(file.path) ? "just-arrived" : ""
                    }`}
                    key={file.path}
                    type="button"
                    onClick={() => void openArtifact(file)}
                  >
                    <span className="arrival-dot" />
                    <span className="file-name">{file.name}</span>
                    <span className={`badge persona-badge badge-${file.persona}`}>{labelForPersona(file.persona)}</span>
                    <span className="file-time">{formatTime(file.mtime)}</span>
                  </button>
                ))
              )}
            </div>
            <div className="section-header projects-header">
              <span className="section-label">Projects</span>
              <span className="count">{projects.length}</span>
            </div>
            {projects.map((project) => (
              <button className="project-row" key={project} type="button">
                <span>{project}</span>
                <span className="file-time">0</span>
              </button>
            ))}
          </aside>
          <section className="content-pane">
            <div className="toolbar">
              <div className="breadcrumb">
                Inbox <span>/</span> <strong>{selectedFile?.name ?? "Select a file"}</strong>
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
                <button className="primary" type="button" onClick={() => void sendCurrentArtifact()} disabled={!artifact}>
                  Send to Claude
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
                  <article className="agent-card" key={session.id}>
                    <div className="agent-card-top">
                      <span className={`badge persona-badge badge-${session.persona}`}>{labelForPersona(session.persona)}</span>
                      <span className="backbone-tag">{session.backbone}</span>
                    </div>
                    <div className="agent-context">[{session.context || "current"}]</div>
                  </article>
                ))}
              </div>
            </aside>
          )}
        </div>
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
      </section>
    </main>
  );
}

function markdownExtension(extension: string): boolean {
  return extension === "md" || extension === "markdown";
}

function htmlExtension(extension: string): boolean {
  return extension === "html" || extension === "htm";
}

function projectForPath(path: string): string {
  const marker = "/Projects/";
  const index = path.indexOf(marker);
  if (index === -1) {
    return "Inbox";
  }
  return path.slice(index + marker.length).split(/[\\/]/)[0] || "Inbox";
}

function labelForPersona(persona: string): string {
  if (persona === "agf-architect") {
    return "AGF";
  }
  return persona;
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
