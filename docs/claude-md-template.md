# AgentCanvas CLAUDE.md Snippet

Paste this into a project `CLAUDE.md` when AgentCanvas is installed.

```md
## AgentCanvas

If AgentCanvas MCP is available, use it to show reviewable artifacts to the user.

Call `open_artifact({ "path": "/absolute/path/to/file.md" })` after writing a Markdown or HTML artifact that the user should inspect in AgentCanvas. Use it for reports, specs, design drafts, implementation notes, and generated HTML previews.

Call `attach_artifact({ "path": "/absolute/path/to/file.md" })` when a file belongs to the current session but should not steal focus.

When you receive `notifications/artifact_updated` with `by: "user"`, re-read the file with normal file tools before continuing. Treat the notification note and action verb as the user's next instruction. Do not rely on stale in-memory content.

Use `add_comment` when annotating a user's artifact instead of writing inline criticism into the source. Keep comments specific and actionable.

Use `get_comments` before revising a file that may already have AgentCanvas feedback.

Use `get_current_focus` when the user says "this file", "the current artifact", or "what I am looking at" and the path is ambiguous.

Typical round trip:

1. Write `docs/active/review.md`.
2. Call `open_artifact` for that path.
3. Wait for the user to review or edit in AgentCanvas.
4. On `notifications/artifact_updated { by: "user" }`, re-read `docs/active/review.md`.
5. Apply the requested revision or add comments with `add_comment`.
6. Call `notify_user` when the next pass is ready.
```
