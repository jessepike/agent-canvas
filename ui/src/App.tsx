import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { RenderedView } from "./components/RenderedView";
import { SourceView } from "./components/SourceView";
import {
  getBootstrapInfo,
  listInbox,
  listPersonas,
  listProjects,
  openDocument,
  parseDocument,
  writeDocument,
  type BootstrapInfo,
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
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [artifact, setArtifact] = useState<OpenArtifact | null>(null);
  const [editMode, setEditMode] = useState(false);
  const [sourceMode, setSourceMode] = useState(false);
  const [conflict, setConflict] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedAt, setSavedAt] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isOpening, setIsOpening] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [arrivedPaths, setArrivedPaths] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextBootstrap, nextFiles, nextProjects, nextPersonas] = await Promise.all([
        getBootstrapInfo(),
        listInbox(),
        listProjects(),
        listPersonas()
      ]);
      setBootstrap(nextBootstrap);
      setFiles(nextFiles);
      setProjects(nextProjects);
      setPersonas(nextPersonas);
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
                <button className="primary" type="button" disabled={!artifact}>
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
          <aside className="agent-gutter">
            <button type="button">+ Connect</button>
          </aside>
        </div>
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
