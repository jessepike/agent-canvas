import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { BlockSchema } from "./types/blocks";
import type { Block } from "./types/blocks";

export async function parseDocument(source: string): Promise<Block[]> {
  try {
    const result = await invoke<unknown>("parse_document", { source });
    return z.array(BlockSchema).parse(result);
  } catch (caught) {
    if (caught instanceof z.ZodError) {
      throw new Error(`IPC contract drift: parse_document returned an invalid Block[]: ${caught.message}`);
    }

    if (caught instanceof Error) {
      throw caught;
    }

    throw new Error(String(caught));
  }
}
