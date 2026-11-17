#!/usr/bin/env python3
"""Record a real headless agent session as a JSONL fixture (stdlib only).

Usage: python3 -I scripts/record-agent-fixture.py <claude|agy> <scenario> <workdir> <out> [session-id] [--no-trim]
       python3 -I scripts/record-agent-fixture.py --scrub-file <in> <out> <workdir>

Scenarios (claude): text, tools, permission-allow, permission-deny, permission-always,
  interrupt, interrupt-text, queue, resume (needs session-id), slash, bad-resume,
  initialize, set-model, set-effort, set-mode, image, move (M11ak Task 0)
Scenarios (agy): text, tools, resume (needs session-id = conversation id), bad-resume,
  restart-flags (5th argument: a model id; an optional 6th, an effort, adds `--effort <it>`),
  move, image
agy: `-p=` and {"event":"user","message":{"content":...}} input (see comments).
`move` runs in <workdir>, then resumes the same conversation from <workdir>/moved (made here).
`--scrub-file` puts a plain command's output (e.g. `claude -p "/usage"`) through the same scrub;
a .jsonl recording also gets the trim's neutral lists (for fixtures recorded before them).

Run each recording in a fresh temp dir, never in the repo. The user's own hooks run (nothing
is switched off). stdout lines are scrubbed and written to <out>; stderr, if any, goes to
<out>.stderr. The scrub is always on (workdir -> /tmp/fx, $HOME -> ~, username -> user, thinking
and signature_delta signatures blanked, `account` replaced, hook output removed). `--no-trim`
only keeps the long lists in the init lines; the default cuts them to 3 entries.
"""
import base64, copy, json, os, queue, re, signal, subprocess, sys, threading, time

TIMEOUT = 180
CLAUDE = ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
          "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio"]
# `-p` needs an argument in agy; `-p=` (empty prompt) selects print mode. A bare `-p` before
# --input-format makes agy swallow "--input-format" as the prompt (found by Task 0).
AGY = ["agy", "-p=", "--input-format", "stream-json", "--output-format", "stream-json",
       "--mode", "accept-edits"]
INIT = {"type": "control_request", "request_id": "fuxx-init",
        "request": {"subtype": "initialize", "hooks": None}}
STOP = {"type": "control_request", "request_id": "fuxx-stop", "request": {"subtype": "interrupt"}}
ZERO = "00000000-0000-0000-0000-000000000000"


IS_AGY = False
TRIM = True  # --trim (default) / --no-trim: shorten the huge list fields of init events
KEEP = 3


def scrub_event(e, scrub):
    """The one always-on privacy scrub (in place, returns e). `scrub` maps a str to a clean str.
    Strings: workdir/home/username. Opaque signatures blanked. `account` replaced. Hook output
    (the user's own hooks) removed."""
    def walk(o):
        if isinstance(o, dict):
            if o.get("type") == "thinking" and "signature" in o:
                o["signature"] = ""
            if o.get("type") == "signature_delta" and "signature" in o:
                o["signature"] = ""
            if isinstance(o.get("sources"), list) and isinstance(o.get("effective"), dict):
                # A get_settings answer: the user's whole settings (hooks, plugins, paths). Keep
                # only what applies now (M11ak Task 0: `applied` and the effective effort).
                o["sources"] = []
                o["effective"] = {k: v for k, v in o["effective"].items() if k == "effortLevel"}
            if isinstance(o.get("account"), dict):
                o["account"] = {k: "user@example.com" if k == "email" else "example"
                                for k in o["account"]}
            for k, v in o.items():
                o[k] = walk(v)
            return o
        if isinstance(o, list):
            return [walk(v) for v in o]
        return scrub(o) if isinstance(o, str) else o

    e = walk(e)
    if str(e.get("subtype", "")).startswith("hook_"):
        for k in ("output", "stdout", "stderr"):
            if e.get(k):
                e[k] = "<hook output removed>"
    return e


def init_box(e):
    """The dict holding the bulky lists of an init event: `system/init`, the initialize
    `control_response`, agy's `init`; None for any other event."""
    if e.get("type") == "system" and e.get("subtype") == "init":
        return e
    if e.get("type") == "control_response" and isinstance(e.get("response", {}).get("response"), dict):
        return e["response"]["response"]
    if e.get("event") == "init" and isinstance(e.get("init"), dict):  # agy
        return e["init"]
    return None


def neutral_init(e):
    """Always on (privacy, not trimming): the user's own installs in an init event replaced."""
    box = init_box(e)
    if box is not None:
        neutral_lists(box)
    return e


def trim_lists(e):
    """Optional (--no-trim skips it): cut every list longer than KEEP to its first KEEP entries
    in the bulky init events: `system/init`, the initialize `control_response`, agy's `init`."""
    box = init_box(e)
    if box is not None:
        for k, v in box.items():
            if isinstance(v, list) and len(v) > KEEP and k not in UNTRIMMED:
                box[k] = v[:KEEP]
    return e


# M11ak Task 0: Claude's command, skill, agent, plugin and MCP lists are the user's own installs.
# The trim keeps Claude's built-ins (a few) and puts one made-up entry where the user's would be,
# so the field shapes stay. `models` is Claude's own product list: kept whole.
BUILTIN_COMMANDS = ("clear", "compact", "effort", "model", "usage")
BUILTIN_AGENTS = ("general-purpose", "Explore", "Plan")
EXAMPLE_SKILL = {"name": "example-skill", "description": "An example skill. (user)", "argumentHint": ""}
EXAMPLE_AGENT = {"name": "example-agent", "description": "An example agent."}
EXAMPLE_PLUGIN = {"name": "example-plugin", "path": "~/.claude/plugins/cache/example/example-plugin/1.0.0",
                  "source": "example-plugin@example"}
EXAMPLE_MCP = {"name": "plugin:example-plugin:example", "status": "pending", "source": "plugin"}
UNTRIMMED = {"models", "commands", "slash_commands", "agents"}  # the last three: made neutral first


def neutral_lists(box):
    """Replace the user's own installs in an init box by built-ins plus one made-up entry."""
    def named(v):
        return v.get("name") if isinstance(v, dict) else v
    if isinstance(box.get("commands"), list):  # the initialize response: dicts
        box["commands"] = [c for c in box["commands"] if named(c) in BUILTIN_COMMANDS] + [EXAMPLE_SKILL]
    if isinstance(box.get("slash_commands"), list):  # system/init: names
        box["slash_commands"] = [c for c in box["slash_commands"] if c in BUILTIN_COMMANDS] + ["example-skill"]
    if isinstance(box.get("skills"), list) and not IS_AGY:
        box["skills"] = ["example-skill"]
    if isinstance(box.get("agents"), list) and not IS_AGY:
        dicts = any(isinstance(a, dict) for a in box["agents"])
        box["agents"] = [a for a in box["agents"] if named(a) in BUILTIN_AGENTS] or (
            [EXAMPLE_AGENT if dicts else EXAMPLE_AGENT["name"]] if box["agents"] else [])
    if isinstance(box.get("plugins"), list) and not IS_AGY:
        box["plugins"] = [EXAMPLE_PLUGIN]
    if isinstance(box.get("mcp_servers"), list) and not IS_AGY:
        box["mcp_servers"] = [EXAMPLE_MCP]
    if isinstance(box.get("tools"), list) and not IS_AGY:  # MCP tools carry server names
        box["tools"] = [t for t in box["tools"] if not str(t).startswith("mcp__")]


def rejoin_agy_text(origs, evs, scrub):
    """agy streams a step's text as `text_delta` fragments that can split a path (found in
    Task 12). Per step: join the raw fragments, scrub, and if that changed anything put the whole
    scrubbed text in the step's first fragment and "" in the rest (like Claude's partial_json)."""
    steps = {}  # (conversation, step_index) -> [(raw fragment, scrubbed step_update dict)]
    for orig, ev in zip(origs, evs):
        if not (isinstance(orig, dict) and orig.get("event") == "step_update"):
            continue
        su = orig.get("step_update") or {}
        if isinstance(su.get("text_delta"), str):
            key = (su.get("conversation_id"), su.get("step_index"))
            steps.setdefault(key, []).append((su["text_delta"], ev["step_update"]))
    for frags in steps.values():
        joined = "".join(raw for raw, _ in frags)
        if scrub(joined) != joined:
            for i, (_, d) in enumerate(frags):
                d["text_delta"] = scrub(joined) if i == 0 else ""


def tool_starts(r, name="Bash"):
    """Wait until `name` is actually running: a tool_use that the user's hooks did not block.
    (Hooks such as GateGuard bounce the first attempt with a tool_result "hook error".)"""
    for _ in range(4):
        r.pump(lambda e: e.get("type") == "assistant" and any(
            b.get("type") == "tool_use" and b.get("name") == name
            for b in e["message"]["content"] if isinstance(b, dict)))
        mark = len(r.lines)
        r.pump(None, 4)
        blocked = any(("hook error" in json.dumps(x) or "Blocked:" in json.dumps(x)) for x in r.lines[mark:] if isinstance(x, dict))
        if not blocked:
            return True
    raise RuntimeError(f"{name} never started (hooks kept blocking it); aborting the scenario")


def user(text):
    if IS_AGY:  # agy input events are {"event": ...}; "role" in the message is rejected
        return {"event": "user", "message": {"content": text}}
    return {"type": "user", "message": {"role": "user", "content": text}}


def mangle(path):
    """How Claude names a folder's entry in ~/.claude/projects: / and . turned into -."""
    return re.sub(r"[/.]", "-", path)


def scrub_text(s, workdirs):
    """The string scrub: the workdir (and its ~/.claude/projects name) -> /tmp/fx, $HOME -> ~,
    the username (any case) -> user, local UTC offsets -> Z, server error ids zeroed."""
    s = re.sub(r"-(private-)?(var-folders|tmp)-[A-Za-z0-9-]+", "-tmp-fx", s)  # mangled cwd in ~/.claude/projects names
    for w in sorted(set(workdirs), key=len, reverse=True):  # after: keeps `-tmp-fx-moved` whole
        s = s.replace(w, "/tmp/fx").replace(mangle(w), "-tmp-fx")
    s = s.replace(os.path.expanduser("~"), "~")
    # A local time's UTC offset tells the owner's timezone (agy's `Created At: …+hh:mm`): UTC.
    s = re.sub(r"(T\d\d:\d\d:\d\d(?:\.\d+)?)[+-]\d\d:\d\d", r"\1Z", s)
    # So does a zone's name (Claude's `/usage`: `resets Nov 17 at 8:10pm (Area/City)`): UTC.
    s = re.sub(r"\((?:Africa|America|Antarctica|Asia|Atlantic|Australia|Europe|Indian|Pacific)/[A-Za-z_/+-]+\)",
               "(UTC)", s)
    # A server's per-request error id (agy's AGY_ERROR line) traces back to the account.
    s = re.sub(r'("error_id"\s*:\s*")[^"]*"', r'\g<1>00000000-0000-0000-0000-000000000000"', s)
    name = os.path.basename(os.path.expanduser("~"))  # e.g. `ls -l` owners, a git author
    return re.sub(re.escape(name), "user", s, flags=re.IGNORECASE)


class Run:
    def __init__(self, argv, workdir, out, policy="allow", scrub_root=None, append=False):
        self.workdir, self.policy = os.path.realpath(scrub_root or workdir), policy
        self.raw_workdir = scrub_root or workdir
        self.append = append  # `move`: the second process's lines go after the first's
        self.out_path = out
        self.lines = []  # raw output: parsed dicts (or str for non-JSON lines); scrubbed in _write
        self.err_path = out + ".stderr"
        self.deadline = time.time() + TIMEOUT
        self.prompts = 0
        self.started = time.time()
        self.p = subprocess.Popen(argv, cwd=workdir, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, bufsize=1, start_new_session=True)
        self.q = queue.Queue()
        self.errbuf = []
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=lambda: self.errbuf.extend(self.p.stderr), daemon=True).start()

    def _read(self):
        for line in self.p.stdout:
            self.q.put(line)
        self.q.put(None)

    def scrub(self, s):
        return scrub_text(s, (self.workdir, self.raw_workdir))

    def _write(self):
        """Scrub every string. Streamed tool input (`partial_json`), text and thinking (found in
        M11ak Task 0) arrive as fragments that can split a path, so per content block: join the
        fragments, scrub, and if that changed anything put the whole scrubbed text in the first
        fragment and "" in the rest."""
        evs = [neutral_init(scrub_event(copy.deepcopy(x), self.scrub)) if isinstance(x, dict)
               else self.scrub(x) for x in self.lines]
        if TRIM:
            evs = [trim_lists(x) if isinstance(x, dict) else x for x in evs]
        rejoin_agy_text(self.lines, evs, self.scrub)
        fields = {"input_json_delta": "partial_json", "text_delta": "text", "thinking_delta": "thinking"}
        raws, deltas, field = [], [], None  # raw fragments, the matching scrubbed deltas, their key
        for orig, ev in zip(self.lines, evs):
            inner = orig.get("event") if isinstance(orig, dict) else None
            if not isinstance(inner, dict):  # agy's "event" is a string
                continue
            kind = inner.get("type")
            if kind == "content_block_start":
                raws, deltas = [], []
            elif inner.get("delta", {}).get("type") in fields:
                field = fields[inner["delta"]["type"]]
                raws.append(inner["delta"][field])
                deltas.append(ev["event"]["delta"])
            elif kind == "content_block_stop":
                joined = "".join(raws)
                if deltas and self.scrub(joined) != joined:
                    for i, d in enumerate(deltas):
                        d[field] = self.scrub(joined) if i == 0 else ""
                raws, deltas = [], []
        return evs

    def send(self, o):
        try:
            self.p.stdin.write(json.dumps(o) + "\n")
            self.p.stdin.flush()
        except (BrokenPipeError, ValueError):
            pass

    def pump(self, pred=None, secs=None):
        """Read events until pred(event) is true, EOF, `secs` elapse, or the overall deadline."""
        end = time.time() + secs if secs is not None else self.deadline
        while time.time() < min(end, self.deadline):
            try:
                line = self.q.get(timeout=0.5)
            except queue.Empty:
                continue
            if line is None:
                return None
            try:
                e = json.loads(line)
                if not isinstance(e, dict):
                    raise ValueError
            except ValueError:
                e = {}
                self.lines.append(line.rstrip("\n"))  # not JSON: kept as text, scrubbed on write
            else:
                self.lines.append(e)
            if e.get("type") == "control_request" and e.get("request", {}).get("subtype") == "can_use_tool":
                self.prompts += 1
                self.send(self.answer(e))
            if pred and pred(e):
                return e
        return None

    def answer(self, e):
        req = e["request"]
        if self.policy == "deny":
            resp = {"behavior": "deny", "message": "Not now"}
        else:
            resp = {"behavior": "allow", "updatedInput": req["input"]}
            if self.policy == "always":
                resp["updatedPermissions"] = req.get("permission_suggestions", [])
        return {"type": "control_response",
                "response": {"subtype": "success", "request_id": e["request_id"], "response": resp}}

    def result(self, secs=None):
        # Claude: {"type":"result"}; agy: {"event":"result"} (else agy waits on stdin until the deadline).
        return self.pump(lambda e: e.get("type") == "result" or e.get("event") == "result", secs)

    def expect_result(self, secs=None):
        e = self.result(secs)
        if e is None:
            raise RuntimeError("no `result` event arrived in time")
        return e

    def kill(self):
        """Kill the child's whole process group (it runs in its own session). Safe to repeat."""
        try:
            os.killpg(self.p.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError):
            pass

    def finish(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        try:
            self.p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
        self.kill()
        time.sleep(0.3)
        self.deadline = time.time() + 5  # the overall deadline may already have passed
        self.pump(None, 3)  # record anything printed during shutdown (returns at EOF)
        with open(self.out_path, "a" if self.append else "w") as f:
            for x in self._write():
                f.write((json.dumps(x, ensure_ascii=False) if isinstance(x, dict) else x) + "\n")
        err = self.scrub("".join(self.errbuf))
        if os.path.exists(self.err_path) and not self.append:
            os.remove(self.err_path)
        if err.strip():
            open(self.err_path, "a").write(err)
        return self.p.returncode


SCENARIOS = {
    "claude": {"text", "tools", "permission-allow", "permission-deny", "permission-always", "interrupt",
               "interrupt-text", "queue", "resume", "slash", "bad-resume",
               "initialize", "set-model", "set-effort", "set-mode", "image", "move"},
    "agy": {"text", "tools", "resume", "bad-resume", "restart-flags", "move", "image"},
}
TOOL_PROMPT = ("Do these four things in order, using the matching tools: 1) Read the file notes.txt "
               "2) Edit notes.txt, changing 'alpha' to 'beta' 3) run `ls` with Bash "
               "4) write a todo list with two items, using the TodoWrite tool (or the task-list tool, if "
               "TodoWrite does not exist). Then say done.")
WRITE_PROMPT = "Create a file named hello.txt containing the word hi, using the Write tool. Do nothing else."
HI = "Say hi in one word"
RECALL = "What was my first message?"
# A 1x1 red PNG (M11ak Task 0's image scenarios).
RED_PNG = ("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC")
SLEEP = "Run `python3 -c 'import time; time.sleep({n})'` with the Bash tool in the foreground (do not background it), then say done."


def play(r, scenario):
    """Send the scenario's messages and wait for what it needs. Raises on a stuck scenario."""
    if scenario in ("text", "bad-resume"):
        r.send(user("Say hello in one word"))
        r.result()
    elif scenario == "tools":
        r.send(user(TOOL_PROMPT))
        r.result()
    elif scenario in ("permission-allow", "permission-deny"):
        r.send(user(WRITE_PROMPT))
        r.result()
    elif scenario == "permission-always":
        r.send(user(WRITE_PROMPT))
        r.result()
        r.send(user("Now create hello2.txt containing the word hi, using the Write tool. Do nothing else."))
        r.result()
    elif scenario == "interrupt":
        r.send(user(SLEEP.format(n=60)))
        tool_starts(r)  # raises if the tool never really started
        r.send(STOP)
        r.expect_result(120)  # stdin stays open until the turn has ended
        # The interrupt's control_response may land after `result`: wait for it too.
        r.pump(lambda e: e.get("type") == "control_response"
               and e.get("response", {}).get("request_id") == "fuxx-stop", 10)
    elif scenario == "interrupt-text":  # interrupt while the model is streaming text
        r.send(user("Write a 500-word essay about trains. No tools."))
        r.pump(lambda e: e.get("type") == "stream_event"
               and e["event"].get("delta", {}).get("type") == "text_delta")
        r.send(STOP)
        r.expect_result(60)
        r.pump(lambda e: e.get("type") == "control_response"
               and e.get("response", {}).get("request_id") == "fuxx-stop", 10)
    elif scenario == "queue":
        r.send(user(SLEEP.format(n=20)))
        tool_starts(r)
        r.send(user("Say hello in one word"))
        r.result()
        r.result()
    elif scenario == "resume":
        r.send(user("What single word did I ask you to say earlier? Answer in one word."))
        r.result()
    elif scenario == "slash":
        r.send(user("Say hello in one word"))
        r.result()
        r.send(user("/compact"))
        r.result(90)
    elif scenario == "initialize":  # only the answer to `initialize` (sent by main), then quit
        answered(r, "fuxx-init", 60)
        print(f"initialize answered in {time.time() - r.started:.1f}s")
    elif scenario == "set-model":
        answered(r, "fuxx-init", 60)
        ask(r, "fuxx-set-1", {"subtype": "set_model", "model": "sonnet"})
        r.send(user(HI))
        r.expect_result()
        ask(r, "fuxx-set-2", {"subtype": "set_model", "model": "opus"})
        r.send(user(HI))
        r.expect_result()
    elif scenario == "set-effort":  # the SDK's applyFlagSettings({effortLevel}); get_settings shows it
        answered(r, "fuxx-init", 60)
        ask(r, "fuxx-settings-1", {"subtype": "get_settings"})
        ask(r, "fuxx-effort-1", {"subtype": "apply_flag_settings", "settings": {"effortLevel": "low"}})
        ask(r, "fuxx-settings-2", {"subtype": "get_settings"})
        r.send(user(HI))
        r.expect_result()
        ask(r, "fuxx-settings-3", {"subtype": "get_settings"})
        # Mid-conversation (after a `result`), as for set_model.
        ask(r, "fuxx-effort-2", {"subtype": "apply_flag_settings", "settings": {"effortLevel": "medium"}})
        ask(r, "fuxx-settings-4", {"subtype": "get_settings"})
        r.send(user(HI))
        r.expect_result()
        ask(r, "fuxx-settings-5", {"subtype": "get_settings"})
    elif scenario == "set-mode":
        answered(r, "fuxx-init", 60)
        ask(r, "fuxx-mode-1", {"subtype": "set_permission_mode", "mode": "plan"})
        r.send(user(WRITE_PROMPT))
        r.expect_result()
        # Mid-conversation (after a `result`): acceptEdits writes without a prompt (the recorder
        # denies every prompt here), so hello.txt appears only if the mode took.
        ask(r, "fuxx-mode-2", {"subtype": "set_permission_mode", "mode": "acceptEdits"})
        r.send(user(WRITE_PROMPT))
        r.expect_result()
    elif scenario == "image":
        if IS_AGY:  # agy takes no image blocks: the PNG's path in the text
            path = os.path.join(r.raw_workdir, "red.png")
            r.send(user(f"What colour is the image at {path}? One word."))
        else:
            r.send({"type": "user", "message": {"role": "user", "content": [
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": RED_PNG}},
                {"type": "text", "text": "What colour is this pixel? One word."}]}})
        r.expect_result()
    elif scenario in ("move", "restart-flags"):  # the second half (main started it resumed)
        r.send(user(RECALL))
        e = r.expect_result()
        refused = (e.get("result") or {}).get("status") == "ERROR"  # agy refused the flags
        if scenario == "restart-flags" and not refused:  # does plan mode take? (no hello.txt)
            r.send(user(WRITE_PROMPT))
            r.expect_result()


def answered(r, rid, secs=30):
    """Wait for the control_response to request `rid` (success or error); None if none came."""
    return r.pump(lambda e: e.get("type") == "control_response"
                  and e.get("response", {}).get("request_id") == rid, secs)


def ask(r, rid, request):
    """Send a control request and wait for its answer. No answer is a finding, not a failure."""
    r.send({"type": "control_request", "request_id": rid, "request": request})
    e = answered(r, rid)
    answer = json.dumps(scrub_event(copy.deepcopy(e), r.scrub).get("response"))[:300] if e else "NO ANSWER in 30s"
    print(f"{rid} {request}: {answer}")
    return e


def session_of(r):
    """The conversation id the recording reports: Claude's system/init, agy's init."""
    for x in r.lines:
        if isinstance(x, dict) and (x.get("type") == "system" and x.get("subtype") == "init"
                                    or x.get("event") == "init"):
            return x.get("session_id") or x.get("conversation_id")
    return None


# The transcript lines `history()` reads, and the fields of them it never needs.
TRANSCRIPT_KEEP = {"user", "assistant"}
TRANSCRIPT_DROP = ("requestId", "requestRef", "promptId", "gitBranch", "userType", "entrypoint")


def transcript_of(workdir, session_id):
    """Claude's own record of a session: ~/.claude/projects/<realpath of the workdir with / and .
    turned into -> /<session id>.jsonl (found in M11aa Task 0)."""
    mangled = re.sub(r"[/.]", "-", os.path.realpath(workdir))
    return os.path.join(os.path.expanduser("~"), ".claude", "projects", mangled, session_id + ".jsonl")


def copy_transcript(r, dest):
    """The `text` scenario's transcript, through the same always-on scrub, to `dest` (M11aa Task 7:
    `history()` reads this file for a resumed conversation)."""
    sid = next((x.get("session_id") for x in r.lines if isinstance(x, dict)
                and x.get("type") == "system" and x.get("subtype") == "init"), None)
    if not sid:
        print("no session id in the recording: no transcript copied", file=sys.stderr)
        return
    src = transcript_of(r.raw_workdir, sid)
    if not os.path.exists(src):
        print(f"no transcript at {src}", file=sys.stderr)
        return
    with open(src, encoding="utf-8") as f, open(dest, "w", encoding="utf-8") as out:
        for line in f:
            line = line.rstrip("\n")
            if not line:
                continue
            try:
                e = json.loads(line)
            except ValueError:
                continue
            # Only the turns: the file also holds the request blobs (system prompt, the user's
            # CLAUDE.md and hook context), attachments, account ids and costs, none of which
            # `history()` reads and none of which belong in the repo.
            # Claude's own lines (isMeta: hook output, CLAUDE.md context) and a sub-agent's thread
            # (isSidechain) are skipped too: `history()` reads neither.
            if (isinstance(e, dict) and e.get("type") in TRANSCRIPT_KEEP
                    and not e.get("isMeta") and not e.get("isSidechain")):
                for k in TRANSCRIPT_DROP:
                    e.pop(k, None)
                out.write(json.dumps(scrub_event(e, r.scrub), ensure_ascii=False) + "\n")
    print(f"transcript: {dest}")


def trim_plain_output(doc):
    """`--scrub-file` on a JSON result: Claude's `/usage` text keeps its limit lines and drops
    "What's contributing…" (the user's own skills, plugins and session counts); the login's
    expiry time goes."""
    if isinstance(doc, dict):
        if doc.get("local_command") == "usage" and isinstance(doc.get("result"), str):
            doc["result"] = doc["result"].split("What's contributing")[0].rstrip("\n") + "\n"
        if "credential_expires_at" in doc:
            doc["credential_expires_at"] = ""
    return doc


def scrub_jsonl(text, workdirs):
    """`--scrub-file` on a recorded .jsonl (e.g. a fixture recorded before a scrub existed): each
    JSON line through the string scrub, scrub_event and the trim (with its neutral lists); a line
    the scrub leaves alone is written back unchanged; non-JSON lines are only string-scrubbed."""
    out = []
    for line in text.split("\n"):
        try:
            e = json.loads(line)
        except ValueError:
            out.append(scrub_text(line, workdirs))
            continue
        if not isinstance(e, dict):
            out.append(line)
            continue
        e = neutral_init(scrub_event(e, lambda x: scrub_text(x, workdirs)))
        if TRIM:
            e = trim_lists(e)
        new = json.dumps(e, ensure_ascii=False)
        out.append(line if json.loads(line) == e else new)
    return "\n".join(out)


def main():
    global TRIM, IS_AGY
    if sys.argv[1:2] == ["--scrub-file"]:
        if len(sys.argv) != 5:
            sys.exit("usage: --scrub-file <in> <out> <workdir>")
        src, dest, wd = sys.argv[2:5]
        raw = open(src, encoding="utf-8").read()
        if src.endswith(".jsonl"):
            open(dest, "w", encoding="utf-8").write(scrub_jsonl(raw, (os.path.realpath(wd), wd)))
            print(f"scrubbed: {dest}")
            return
        text = scrub_text(raw, (os.path.realpath(wd), wd))
        try:
            doc = json.loads(text)
        except ValueError:
            doc = None
        if isinstance(doc, dict):
            doc = trim_plain_output(scrub_event(doc, lambda x: x))
            text = json.dumps(doc, ensure_ascii=False, indent=2) + "\n"
        open(dest, "w", encoding="utf-8").write(text)
        print(f"scrubbed: {dest}")
        return
    if {"-h", "--help"} & set(sys.argv[1:]) or len(sys.argv) < 5:
        print(__doc__)
        sys.exit(0 if len(sys.argv) >= 2 and {"-h", "--help"} & set(sys.argv[1:]) else 2)
    args = [a for a in sys.argv[1:] if a not in ("--trim", "--no-trim")]
    TRIM = "--no-trim" not in sys.argv
    agent, scenario, workdir, out = args[:4]
    sid = args[4] if len(args) > 4 else None
    effort = args[5] if len(args) > 5 else None  # restart-flags only
    # Validate everything BEFORE spawning the agent, so a bad invocation leaks no child.
    if agent not in SCENARIOS:
        sys.exit(f"unknown agent {agent!r} (claude|agy)")
    if scenario not in SCENARIOS[agent]:
        sys.exit(f"unknown scenario {scenario!r} for {agent}: {sorted(SCENARIOS[agent])}")
    if not os.path.isdir(workdir):
        sys.exit(f"workdir {workdir!r} is not a directory")
    claude = agent == "claude"
    IS_AGY = not claude
    argv = list(CLAUDE if claude else AGY)
    if scenario == "bad-resume":
        sid = ZERO
    if scenario in ("resume", "bad-resume"):
        if not sid:
            sys.exit("resume needs a session id as the 5th argument")
        argv += ["--resume", sid] if claude else ["--conversation", sid]
    if scenario == "restart-flags" and not sid:
        sys.exit("restart-flags needs a model id (from `agy models`) as the 5th argument")
    if scenario == "tools":
        open(os.path.join(workdir, "notes.txt"), "w").write("alpha\n")
    if scenario == "image" and not claude:
        open(os.path.join(workdir, "red.png"), "wb").write(base64.b64decode(RED_PNG))
    # `set-mode` denies any tool prompt: an ExitPlanMode allowed would leave plan mode and write.
    policy = {"permission-deny": "deny", "permission-always": "always",
              "set-mode": "deny"}.get(scenario, "allow")
    two_part = scenario in ("move", "restart-flags")
    failure, r = None, None
    try:
        if two_part:  # first half: a one-word conversation in <workdir>
            first = Run(argv, workdir, out, policy)
            try:
                if claude:
                    first.send(INIT)
                first.send(user(HI))
                first.expect_result()
            finally:
                first.finish()
            conv = session_of(first)
            if not conv:
                raise RuntimeError("the first half reported no conversation id")
            print(f"first half: conversation {conv}")
            again = list(argv) + (["--resume", conv] if claude else ["--conversation", conv])
            where = workdir
            if scenario == "move":
                where = os.path.join(workdir, "moved")
                os.makedirs(where, exist_ok=True)
            else:  # agy restart-flags: the same conversation with a new mode, model and effort
                i = again.index("--mode")
                again[i + 1] = "plan"
                again += ["--model", sid] + (["--effort", effort] if effort else [])
            print("second half:", " ".join(scrub_text(a, (workdir,)) for a in again), "in",
                  scrub_text(where, (workdir,)))
            r = Run(again, where, out, policy, scrub_root=workdir, append=True)
        else:
            r = Run(argv, workdir, out, policy)
        if claude:
            r.send(INIT)
        play(r, scenario)
    except (RuntimeError, KeyboardInterrupt, Exception) as ex:  # keep what was captured, then re-raise
        failure = ex
    finally:
        code = None
        if r is not None:
            code = r.finish()  # always writes <out>, and always kills the process group
            r.kill()
    if r is None:
        raise failure
    if claude and scenario == "text" and failure is None:
        copy_transcript(r, os.path.join(os.path.dirname(out) or ".", "claude-transcript.jsonl"))
    print(f"{agent} {scenario}: exit={code} prompts={r.prompts} out={out}")
    if failure is not None:
        raise failure


if __name__ == "__main__":
    main()
