import { open, save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { RenderedView } from "./components/RenderedView";
import { SourceView } from "./components/SourceView";
import { openDocument, parseDocument, writeDocument } from "./ipc";
import type { Block } from "./types/blocks";

const sampleSource = `# Vellum IPC proof

Plain Markdown stays plain.

\`\`\`vellum:live-query
version: 1
tool: github.list_issues
args:
  repo: jessepike/vellum
  state: open
\`\`\`
`;

export default function App() {
  const [source, setSource] = useState(sampleSource);
  const [blocks, setBlocks] = useState<Block[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [conflict, setConflict] = useState(false);
  const [docPath, setDocPath] = useState<string | null>(null);
  const [baseHash, setBaseHash] = useState<number[] | null>(null);
  const [dirty, setDirty] = useState(false);
  const [isParsing, setIsParsing] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [isOpening, setIsOpening] = useState(false);

  const openPath = useCallback(async (path: string) => {
    setIsOpening(true);
    setError(null);
    setConflict(false);

    try {
      const opened = await openDocument(path);
      setDocPath(opened.path);
      setBaseHash(opened.base_hash);
      setSource(opened.source);
      setBlocks([]);
      setDirty(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsOpening(false);
    }
  }, []);

  const handleOpen = useCallback(async () => {
    if (dirty && !window.confirm("Discard unsaved changes and open another document?")) {
      return;
    }

    setError(null);
    const selected = await open({
      multiple: false,
      filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }]
    });

    if (!selected || Array.isArray(selected)) {
      return;
    }

    await openPath(selected);
  }, [dirty, openPath]);

  const handleReload = useCallback(async () => {
    if (!docPath) {
      return;
    }

    await openPath(docPath);
  }, [docPath, openPath]);

  async function handleParse() {
    setIsParsing(true);
    setError(null);

    try {
      const parsedBlocks = await parseDocument(source);
      setBlocks(parsedBlocks);
    } catch (caught) {
      setBlocks([]);
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setIsParsing(false);
    }
  }

  const handleSave = useCallback(async () => {
    setError(null);
    setConflict(false);

    let targetPath = docPath;
    let hash = baseHash;
    if (!targetPath) {
      const selected = await save({
        defaultPath: "Untitled.md",
        filters: [{ name: "Markdown", extensions: ["md", "markdown", "txt"] }]
      });
      if (!selected) {
        return;
      }

      targetPath = selected;
      hash = new Array<number>(32).fill(0);
    }

    if (!hash) {
      setError("Cannot save without a base hash; reload the document and try again.");
      return;
    }

    setIsSaving(true);
    try {
      const result = await writeDocument(targetPath, source, hash);
      setDocPath(targetPath);
      setBaseHash(result.new_hash);
      setDirty(false);
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
  }, [baseHash, docPath, source]);

  function handleSourceChange(next: string) {
    setSource(next);
    setDirty(true);
    setConflict(false);
  }

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const isModified = event.metaKey || event.ctrlKey;
      if (!isModified || event.altKey || event.shiftKey) {
        return;
      }

      if (event.key.toLowerCase() === "s") {
        event.preventDefault();
        void handleSave();
      }

      if (event.key.toLowerCase() === "o") {
        event.preventDefault();
        void handleOpen();
      }
    }

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [handleOpen, handleSave]);

  const fileName = docPath ? basename(docPath) : "Untitled.md";

  return (
    <main className="app">
      <h1>Vellum</h1>
      <div className="toolbar">
        <button type="button" onClick={handleOpen} disabled={isOpening}>
          {isOpening ? "Opening..." : "Open .md..."}
        </button>
        <button type="button" onClick={handleSave} disabled={isSaving}>
          {isSaving ? "Saving..." : "Save"}
        </button>
        <button type="button" onClick={handleParse} disabled={isParsing}>
          {isParsing ? "Parsing..." : "Parse"}
        </button>
        <span className="document-status" aria-label="Document status">
          {fileName} {dirty ? "•" : "✓"}
        </span>
      </div>
      {conflict ? (
        <div className="conflict-banner" role="alert">
          <span>File changed on disk since open. Save aborted — reload or open three-way merge.</span>
          <button type="button" onClick={handleReload} disabled={!docPath || isOpening}>
            Reload from disk
          </button>
        </div>
      ) : null}
      <div className="editor-stack">
        <section className="editor-panel source-panel" aria-label="Source editor panel">
          <SourceView value={source} onChange={handleSourceChange} onOpen={handleOpen} onSave={handleSave} />
        </section>
        <section className="editor-panel rendered-panel" aria-label="Rendered preview panel">
          <RenderedView blocks={blocks} />
        </section>
      </div>
      {error ? <p className="error">{error}</p> : null}
    </main>
  );
}

function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}
