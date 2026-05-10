import { markdown } from "@codemirror/lang-markdown";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { useEffect, useRef } from "react";

type SourceViewProps = {
  value: string;
  onChange: (next: string) => void;
};

export function SourceView({ value, onChange }: SourceViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const valueRef = useRef(value);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    valueRef.current = value;
  }, [value]);

  useEffect(() => {
    const parent = containerRef.current;
    if (!parent) {
      return;
    }

    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc: valueRef.current,
        extensions: [
          basicSetup,
          markdown(),
          EditorView.lineWrapping,
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) {
              return;
            }

            const next = update.state.doc.toString();
            valueRef.current = next;
            onChangeRef.current(next);
          })
        ]
      })
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, []);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }

    const current = view.state.doc.toString();
    if (value === current) {
      return;
    }

    view.dispatch({
      changes: { from: 0, to: current.length, insert: value }
    });
  }, [value]);

  return <div ref={containerRef} className="source-view" aria-label="Markdown source" />;
}
