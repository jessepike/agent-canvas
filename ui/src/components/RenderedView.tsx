import { EditorState } from "prosemirror-state";
import { EditorView } from "prosemirror-view";
import { useEffect, useRef } from "react";
import { blockIdsPlugin } from "../pm/blockIdsPlugin";
import { blocksToDoc, vellumSchema } from "../pm/schema";
import { frontmatterNodeView, liveQueryNodeView, resultNodeView } from "../pm/nodeviews";
import type { Block } from "../types/blocks";

type RenderedViewProps = {
  blocks: Block[];
};

export function RenderedView({ blocks }: RenderedViewProps) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const viewRef = useRef<EditorView | null>(null);

  useEffect(() => {
    const parent = containerRef.current;
    if (!parent) {
      return;
    }

    const view = new EditorView(parent, {
      state: EditorState.create({
        schema: vellumSchema,
        doc: blocksToDoc(blocks),
        plugins: [blockIdsPlugin()]
      }),
      editable: () => false,
      nodeViews: {
        vellum_live_query: liveQueryNodeView,
        vellum_result: resultNodeView,
        frontmatter: frontmatterNodeView
      }
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

    view.updateState(
      EditorState.create({
        schema: vellumSchema,
        doc: blocksToDoc(blocks),
        plugins: [blockIdsPlugin()]
      })
    );
  }, [blocks]);

  return <div ref={containerRef} className="rendered-view" aria-label="Rendered Markdown" />;
}
