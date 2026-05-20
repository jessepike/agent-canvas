import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { EditorState, Prec } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";

export type SourceFormat = "bold" | "italic" | "strike" | "code" | "revision";

export type SourceViewHandle = {
  applyFormat: (format: SourceFormat) => void;
};

type SourceViewProps = {
  value: string;
  language?: "markdown" | "json" | "plaintext";
  onChange: (next: string) => void;
  onOpen?: () => void;
  onSave?: () => void;
  onSelectionBoundsChange?: (bounds: DOMRect | null) => void;
};

export const SourceView = forwardRef<SourceViewHandle, SourceViewProps>(function SourceView(
  { value, language = "markdown", onChange, onOpen, onSave, onSelectionBoundsChange },
  ref
) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onOpenRef = useRef(onOpen);
  const onSaveRef = useRef(onSave);
  const onSelectionBoundsChangeRef = useRef(onSelectionBoundsChange);
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
    onSelectionBoundsChangeRef.current = onSelectionBoundsChange;
  }, [onSelectionBoundsChange]);

  useEffect(() => {
    valueRef.current = value;
  }, [value]);

  useImperativeHandle(ref, () => ({
    applyFormat: (format) => {
      const view = viewRef.current;
      if (!view) {
        return;
      }
      const selection = view.state.selection.main;
      if (selection.empty) {
        return;
      }
      const selected = view.state.sliceDoc(selection.from, selection.to);
      const [prefix, suffix] = wrappersForFormat(format);
      view.dispatch({
        changes: { from: selection.from, to: selection.to, insert: `${prefix}${selected}${suffix}` },
        selection: {
          anchor: selection.from + prefix.length,
          head: selection.to + prefix.length
        },
        scrollIntoView: true
      });
      view.focus();
      updateSelectionBounds(view);
    }
  }));

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
              },
              {
                key: "Mod-b",
                run: (view) => applyFormatToView(view, "bold")
              },
              {
                key: "Mod-i",
                run: (view) => applyFormatToView(view, "italic")
              },
              {
                key: "Mod-Shift-x",
                run: (view) => applyFormatToView(view, "strike")
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
          }),
          EditorView.updateListener.of((update) => {
            if (update.selectionSet || update.focusChanged || update.docChanged || update.geometryChanged) {
              updateSelectionBounds(update.view);
            }
          })
        ]
      })
    });

    viewRef.current = view;
    selectionCallbacks.set(view, (bounds) => onSelectionBoundsChangeRef.current?.(bounds));

    return () => {
      selectionCallbacks.delete(view);
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
});

function wrappersForFormat(format: SourceFormat): [string, string] {
  if (format === "bold") {
    return ["**", "**"];
  }
  if (format === "italic") {
    return ["*", "*"];
  }
  if (format === "strike") {
    return ["~~", "~~"];
  }
  if (format === "code") {
    return ["`", "`"];
  }
  return ['<mark data-revision="true">', "</mark>"];
}

function applyFormatToView(view: EditorView, format: SourceFormat): boolean {
  const selection = view.state.selection.main;
  if (selection.empty) {
    return false;
  }
  const selected = view.state.sliceDoc(selection.from, selection.to);
  const [prefix, suffix] = wrappersForFormat(format);
  view.dispatch({
    changes: { from: selection.from, to: selection.to, insert: `${prefix}${selected}${suffix}` },
    selection: {
      anchor: selection.from + prefix.length,
      head: selection.to + prefix.length
    },
    scrollIntoView: true
  });
  updateSelectionBounds(view);
  return true;
}

function updateSelectionBounds(view: EditorView) {
  const callback = onSelectionBoundsChangeRefForView(view);
  if (!callback) {
    return;
  }
  const selection = view.state.selection.main;
  if (selection.empty || !view.hasFocus) {
    callback(null);
    return;
  }
  const start = view.coordsAtPos(selection.from);
  const end = view.coordsAtPos(selection.to);
  if (!start || !end) {
    callback(null);
    return;
  }
  const left = Math.min(start.left, end.left);
  const right = Math.max(start.right, end.right);
  const top = Math.min(start.top, end.top);
  const bottom = Math.max(start.bottom, end.bottom);
  callback(new DOMRect(left, top, Math.max(right - left, 1), Math.max(bottom - top, 1)));
}

const selectionCallbacks = new WeakMap<EditorView, (bounds: DOMRect | null) => void>();

function onSelectionBoundsChangeRefForView(view: EditorView) {
  return selectionCallbacks.get(view);
}
