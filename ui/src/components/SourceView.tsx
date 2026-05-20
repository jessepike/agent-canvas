import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { useEffect, useRef } from "react";

type SourceViewProps = {
  value: string;
  language?: "markdown" | "json" | "plaintext";
  onChange: (next: string) => void;
  onOpen?: () => void;
  onSave?: () => void;
};

export function SourceView({ value, language = "markdown", onChange, onOpen, onSave }: SourceViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onOpenRef = useRef(onOpen);
  const onSaveRef = useRef(onSave);
  const valueRef = useRef(value);
  const isApplyingExternalValueRef = useRef(false);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    onOpenRef.current = onOpen;
  }, [onOpen]);

  useEffect(() => {
    onSaveRef.current = onSave;
  }, [onSave]);

  useEffect(() => {
    valueRef.current = value;
  }, [value]);

  useEffect(() => {
    const parent = containerRef.current;
    if (!parent) {
      return;
    }

    const languageExtension = language === "json" ? json() : language === "markdown" ? markdown() : [];
    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc: valueRef.current,
        extensions: [
          Prec.high(
            keymap.of([
              {
                key: "Mod-o",
                run: () => {
                  onOpenRef.current?.();
                  return true;
                }
              },
              {
                key: "Mod-s",
                run: () => {
                  onSaveRef.current?.();
                  return true;
                }
              }
            ])
          ),
          basicSetup,
          languageExtension,
          EditorView.lineWrapping,
          EditorView.updateListener.of((update) => {
            if (!update.docChanged) {
              return;
            }

            const next = update.state.doc.toString();
            valueRef.current = next;
            if (isApplyingExternalValueRef.current) {
              return;
            }
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
  }, [language]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) {
      return;
    }

    const current = view.state.doc.toString();
    if (value === current) {
      return;
    }

    isApplyingExternalValueRef.current = true;
    view.dispatch({
      changes: { from: 0, to: current.length, insert: value }
    });
    isApplyingExternalValueRef.current = false;
  }, [value]);

  return <div ref={containerRef} className="source-view" aria-label={`${language} source`} />;
}
