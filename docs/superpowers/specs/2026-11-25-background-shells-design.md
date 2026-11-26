# Chore 16: background shells, beside the tasks tab (design, 2026-11-25)

Your words: "we now dont have shells. I cannot see what shells are running. where do you think we should put
it" → mockups → "love the popover, but is it good to put it beside tasks?" → "make it: :terminalicon x/n just like
tasks" → "make it a chore and build it". Recorded under note 93 ("Chores after M11ar"), built on master.

## Why

An agent's background shells (Claude's `run_in_background` Bash: a release watcher, a dev server, a long build)
run unseen in fuxx today. You can't tell what is running, how far along it is, or stop one.

## What (the approved mockup, v3)

- **A shells tab** in the agent box's top row, beside the plan tab (`☰ 4/6`), in the same look: Lucide's
  terminal icon and **done/all** for this conversation's shells since your last message (`▣ 1/3`). Peach while
  any runs; green `3/3 ✓` once all finished (stays until your next message, as the plan's `6/6 ✓` does); red with
  ✕ when one failed, until you open the popover. Shown only once a shell started. With the plan tab it sits on
  the plan tab's left, the two flush right as one group (the plan tab's mirrored step moves to the group's left);
  alone, it takes the corner.
- **Its popover** (pointing at it, like the plan's: over the chat, its right edge on the box's): header
  "1 of 3 done · 2 running"; a row per shell: a status mark (the running arc / ✓ / ✕), Claude's description,
  elapsed time (`4:12`, ticking), the command in small mono, its **last output line, live**, and ■ to stop a
  running one. Finished rows dim and drop off 2 minutes after they end. A click on a row opens its output file
  in a file tab (file tabs already follow a file that grows). Footer: "Click a shell to follow its output".
- **In the chat**, the Bash step that started it reads "running in the background · 4:12" while it runs.
- **A banner** when a shell finishes while you're not looking at the tab, as a finished command's: "<description>
  · done in 6:40" (failed: "· failed").

## What fuxx already gets (Claude's stream-json)

- The `Bash` tool_use with `input.run_in_background: true` and `input.command`; its tool_result names the shell.
- `system/task_started` with `task_type: "local_bash"`, `task_id`, `tool_use_id`, `description`.
- `system/task_notification` with `task_id`, `status` (`completed` / `failed` / `killed` / `stopped`), `summary`,
  `output_file`. Today `claude.rs` maps these to subagent lanes only (they're dropped for `local_bash`).
- Stopping: the same control request fuxx sends to stop a subagent's task (check `StopSubagent`'s encoding).

## Non-goals

- No sidebar badge (the mockup's v1 idea) in this chore. No agy support unless agy reports background shells the
  same way (check its stream; if not, Claude only, said in note 93). No shells started by you in terminal tabs.

## Built as

Provider-neutral events in `providers/mod.rs` (`ShellStarted { id, call, description, command }`,
`ShellEnded { id, status, summary, output }`), read in `claude.rs` (provider-specific code stays there); state in
the conversation; the tab and popover beside the plan tab's code in `agent_box.rs` / `agent_box_menus.rs` (reuse
`menu_frame_at`, `MenuPlace::Right`, the plan tab's shell and step); the last line read off the UI thread from the
output file while the popover is open. Tests first; ui-shots scenes for the tab (with and without a plan, done,
failed) and the popover.
