import "./styles.css";

const previewFiles = [
  { name: "agf-positioning-v3.md", kind: "md", persona: "cto", time: "09:41" },
  { name: "competitive-analysis.html", kind: "html", persona: "claude", time: "09:18" },
  { name: "grant-runner-plan.md", kind: "md", persona: "codex", time: "08:52" }
];

export default function App() {
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
        </header>
        <div className="main-shell">
          <aside className="sidebar">
            <div className="sidebar-header">
              <label className="search">
                <span>Search</span>
                <input placeholder="Search artifacts" />
              </label>
            </div>
            <div className="section-label">Inbox</div>
            <div className="file-list">
              {previewFiles.map((file, index) => (
                <button className={`file-row ${index === 0 ? "selected" : ""}`} key={file.name} type="button">
                  <span className="arrival-dot" />
                  <span className="file-name">{file.name}</span>
                  <span className={`badge badge-${file.persona}`}>{file.persona}</span>
                  <span className="file-time">{file.time}</span>
                </button>
              ))}
            </div>
            <div className="section-label">Projects</div>
            <button className="project-row" type="button">
              <span>Default</span>
              <span className="file-time">0</span>
            </button>
          </aside>
          <section className="content-pane">
            <div className="toolbar">
              <div className="breadcrumb">
                Inbox <span>/</span> <strong>agf-positioning-v3.md</strong>
              </div>
              <div className="toolbar-actions">
                <button type="button">Edit</button>
                <button className="primary" type="button">
                  Send to Claude
                </button>
              </div>
            </div>
            <article className="document">
              <p className="eyebrow">Fresh AgentCanvas shell</p>
              <h1>Artifact inbox, not a writing app.</h1>
              <p>
                Slice 1 replaces the old Vellum editor surface with a read-first artifact workbench shell. The next
                slices wire this layout to iCloud, SQLite state, Markdown and HTML rendering, safe save, watcher
                invalidation, persona badges, and pasteboard handoff.
              </p>
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
