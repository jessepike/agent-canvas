import { parseDocument } from "yaml";
import { z } from "zod";

const LiveQueryYaml = z
  .object({
    version: z.union([z.number(), z.string()]).optional(),
    id: z.string().optional(),
    tool: z.string().optional(),
    args: z.unknown().optional(),
    render: z.string().optional(),
    cache: z.unknown().optional(),
    result_policy: z.string().optional()
  })
  .passthrough();

const ResultYaml = z
  .object({
    id: z.string().optional(),
    for_id: z.string().optional(),
    content_hash: z.string().optional(),
    result_hash: z.string().optional(),
    recipe_hash: z.string().optional(),
    frozen_at: z.string().optional(),
    captured_at: z.string().optional(),
    render: z.string().optional(),
    data: z.unknown().optional()
  })
  .passthrough();

type NodeAttrs = Record<string, unknown>;

function parseYaml(raw: string): { value: unknown } | { error: string } {
  try {
    const document = parseDocument(raw);
    if (document.errors.length > 0) {
      return { error: document.errors.map((error) => error.message).join("; ") };
    }
    return { value: document.toJSON() as unknown };
  } catch (error) {
    return { error: error instanceof Error ? error.message : "invalid YAML" };
  }
}

function invalidYamlAttrs(raw: string, error: string): NodeAttrs {
  return {
    tool: "invalid YAML",
    yaml_error: error,
    raw_yaml: raw
  };
}

export function liveQueryAttrs(raw: string): NodeAttrs {
  const parsed = parseYaml(raw);
  if ("error" in parsed) {
    return invalidYamlAttrs(raw, parsed.error);
  }

  const result = LiveQueryYaml.safeParse(parsed.value);
  if (!result.success) {
    return invalidYamlAttrs(raw, result.error.issues.map((issue) => issue.message).join("; "));
  }

  return {
    id: result.data.id ?? null,
    version: result.data.version ?? null,
    tool: result.data.tool ?? null,
    args: result.data.args ?? null,
    render: result.data.render ?? "json",
    cache: result.data.cache ?? null,
    result_policy: result.data.result_policy ?? "pinned",
    raw_yaml: raw
  };
}

export function resultAttrs(raw: string): NodeAttrs {
  const parsed = parseYaml(raw);
  if ("error" in parsed) {
    return { yaml_error: parsed.error, raw_yaml: raw };
  }

  const result = ResultYaml.safeParse(parsed.value);
  if (!result.success) {
    return {
      yaml_error: result.error.issues.map((issue) => issue.message).join("; "),
      raw_yaml: raw
    };
  }

  return {
    id: result.data.id ?? null,
    for_id: result.data.for_id ?? null,
    content_hash: result.data.content_hash ?? null,
    result_hash: result.data.result_hash ?? null,
    recipe_hash: result.data.recipe_hash ?? null,
    frozen_at: result.data.frozen_at ?? null,
    captured_at: result.data.captured_at ?? null,
    render: result.data.render ?? "json",
    data: result.data.data ?? null,
    raw_yaml: raw
  };
}
