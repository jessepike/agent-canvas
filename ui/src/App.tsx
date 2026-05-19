import { useCallback, useEffect, useMemo, useState } from "react";
import { getBootstrapInfo, listInbox, listProjects, type BootstrapInfo, type FileMetadata } from "./ipc";
import "./styles.css";

export default function App() {
  const [bootstrap, setBootstrap] = useState<BootstrapInfo | null>(null);
  const [files, setFiles] = useState<FileMetadata[]>([]);
  const [projects, setProjects] = useState<string[]>([]);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const [nextBootstrap, nextFiles, nextProjects] = await Promise.all([
        getBootstrapInfo(),
        listInbox(),
        listProjects()
      ]);
      setBootstrap(nextBootstrap);
      setFiles(nextFiles);
      setProjects(nextProjects);
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
                    className={`file-row ${file.path === selectedPath ? "selected" : ""}`}
                    key={file.path}
                    type="button"
                    onClick={() => setSelectedPath(file.path)}
                  >
                    <span className="arrival-dot" />
                    <span className="file-name">{file.name}</span>
                    <span className={`badge badge-${file.extension}`}>{file.extension}</span>
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
            </div>
            <article className="document placeholder-document">
              {selectedFile ? (
                <>
                  <p className="eyebrow">{selectedFile.extension.toUpperCase()} artifact</p>
                  <h1>{selectedFile.name}</h1>
                  <dl className="metadata-grid">
                    <div>
                      <dt>Path</dt>
                      <dd>{selectedFile.relative_path}</dd>
                    </div>
                    <div>
                      <dt>Size</dt>
                      <dd>{formatBytes(selectedFile.size)}</dd>
                    </div>
                    <div>
                      <dt>Modified</dt>
                      <dd>{new Date(selectedFile.mtime * 1000).toLocaleString()}</dd>
                    </div>
                  </dl>
                </>
              ) : (
                <>
                  <p className="eyebrow">Ready</p>
                  <h1>Select a file.</h1>
                  <p>Drop Markdown or HTML artifacts into the AgentCanvas inbox and rescan.</p>
                </>
              )}
              {error ? <p className="error-banner">{error}</p> : null}
            </article>
          </section>
          <aside className="agent-gutter">
            <button type="button">+ Connect</button>
          </aside>
        </div>
      </section>
    </main>
  );
}

function formatTime(epochSeconds: number): string {
  if (!epochSeconds) {
    return "--:--";
  }
  const date = new Date(epochSeconds * 1000);
  return `${date.getHours().toString().padStart(2, "0")}:${date.getMinutes().toString().padStart(2, "0")}`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`;
  }
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
