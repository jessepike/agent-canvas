import { useState } from "react";
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
      <textarea
        aria-label="Markdown source"
        value={source}
        onChange={(event) => setSource(event.target.value)}
        spellCheck={false}
      />
      {error ? <p className="error">{error}</p> : null}
      <pre>{JSON.stringify(blocks, null, 2)}</pre>
    </main>
  );
}
