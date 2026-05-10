export type BlockId = string;

export type BlockIdentity = {
  id: BlockId;
  byte_range_start: number;
  kind: string;
};

export type IdentityMap = {
  source_hash: number[];
  block_ids: BlockIdentity[];
};

/*
 * TODO(30B-02-followup): wire to Rust IPC `load_sidecar`/`save_sidecar`.
 *
 * The Rust sidecar implementation already owns load/save/migration. This UI
 * bridge is intentionally stubbed until the Tauri commands land.
 */
export async function loadSidecar(_docPath: string): Promise<IdentityMap> {
  return { source_hash: [], block_ids: [] };
}

export async function saveSidecar(_docPath: string, _map: IdentityMap): Promise<void> {
  return Promise.resolve();
}
