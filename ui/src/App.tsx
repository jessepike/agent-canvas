import { useState } from "react";
import { RenderedView } from "./components/RenderedView";
import { SourceView } from "./components/SourceView";
import { parseDocument } from "./ipc";
import type { ChangeEvent } from "react";
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
  const [isParsing, setIsParsing] = useState(false);

  async function handleFileChange(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }

    setError(null);
    setSource(await file.text());
    setBlocks([]);
  }

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

  return (
    <main className="app">
      <h1>Vellum</h1>
      <div className="toolbar">
        <label className="file-picker">
          Load .md
          <input type="file" accept=".md,text/markdown,text/plain" onChange={handleFileChange} />
        </label>
        <button type="button" onClick={handleParse} disabled={isParsing}>
          {isParsing ? "Parsing..." : "Parse"}
        </button>
      </div>
      <div className="editor-stack">
        <section className="editor-panel source-panel" aria-label="Source editor panel">
          <SourceView value={source} onChange={setSource} />
        </section>
        <section className="editor-panel rendered-panel" aria-label="Rendered preview panel">
          <RenderedView blocks={blocks} />
        </section>
      </div>
      {error ? <p className="error">{error}</p> : null}
    </main>
  );
}
