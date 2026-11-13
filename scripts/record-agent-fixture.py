#!/usr/bin/env python3
"""Record a real headless agent session as a JSONL fixture (stdlib only).

Usage: python3 -I scripts/record-agent-fixture.py <claude|agy> <scenario> <workdir> <out> [session-id] [--no-trim]

Scenarios (claude): text, tools, permission-allow, permission-deny, permission-always,
  interrupt, interrupt-text, queue, resume (needs session-id), slash, bad-resume
Scenarios (agy): text, tools, resume (needs session-id = conversation id)
agy: `-p=` and {"event":"user","message":{"content":...}} input (see comments).

Run each recording in a fresh temp dir, never in the repo. The user's own hooks run (nothing
is switched off). stdout lines are scrubbed and written to <out>; stderr, if any, goes to
<out>.stderr. The scrub is always on (workdir -> /tmp/fx, $HOME -> ~, username -> user, thinking
and signature_delta signatures blanked, `account` replaced, hook output removed). `--no-trim`
only keeps the long lists in the init lines; the default cuts them to 3 entries.
"""
import copy, json, os, queue, re, signal, subprocess, sys, threading, time

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


def trim_lists(e):
    """Optional (--no-trim skips it): cut every list longer than KEEP to its first KEEP entries
    in the bulky init events: `system/init`, the initialize `control_response`, agy's `init`."""
    box = None
    if e.get("type") == "system" and e.get("subtype") == "init":
        box = e
    elif e.get("type") == "control_response" and isinstance(e.get("response", {}).get("response"), dict):
        box = e["response"]["response"]
    elif e.get("event") == "init" and isinstance(e.get("init"), dict):  # agy
        box = e["init"]
    if box is not None:
        for k, v in box.items():
            if isinstance(v, list) and len(v) > KEEP:
                box[k] = v[:KEEP]
    return e


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


class Run:
    def __init__(self, argv, workdir, out, policy="allow"):
        self.workdir, self.policy = os.path.realpath(workdir), policy
        self.raw_workdir = workdir
        self.out_path = out
        self.lines = []  # raw output: parsed dicts (or str for non-JSON lines); scrubbed in _write
        self.err_path = out + ".stderr"
        self.deadline = time.time() + TIMEOUT
        self.prompts = 0
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
        for w in sorted({self.workdir, self.raw_workdir}, key=len, reverse=True):
            s = s.replace(w, "/tmp/fx")
        s = s.replace(os.path.expanduser("~"), "~")
        s = re.sub(r"-(private-)?(var-folders|tmp)-[A-Za-z0-9-]+", "-tmp-fx", s)  # mangled cwd in ~/.claude/projects names
        return s.replace(os.path.basename(os.path.expanduser("~")), "user")  # e.g. `ls -l` owners

    def _write(self):
        """Scrub every string. Streamed tool input arrives as `partial_json` fragments that can
        split a path, so per content block: join the fragments, scrub, and if that changed
        anything put the whole scrubbed text in the first fragment and "" in the rest."""
        evs = [scrub_event(copy.deepcopy(x), self.scrub) if isinstance(x, dict) else self.scrub(x)
               for x in self.lines]
        if TRIM:
            evs = [trim_lists(x) if isinstance(x, dict) else x for x in evs]
        raws, deltas = [], []  # raw fragment text, and the matching scrubbed delta dicts
        for orig, ev in zip(self.lines, evs):
            inner = orig.get("event") if isinstance(orig, dict) else None
            if not isinstance(inner, dict):  # agy's "event" is a string
                continue
            kind = inner.get("type")
            if kind == "content_block_start":
                raws, deltas = [], []
            elif inner.get("delta", {}).get("type") == "input_json_delta":
                raws.append(inner["delta"]["partial_json"])
                deltas.append(ev["event"]["delta"])
            elif kind == "content_block_stop":
                joined = "".join(raws)
                if deltas and self.scrub(joined) != joined:
                    for i, d in enumerate(deltas):
                        d["partial_json"] = self.scrub(joined) if i == 0 else ""
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
        return self.pump(lambda e: e.get("type") == "result", secs)

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
        with open(self.out_path, "w") as f:
            for x in self._write():
                f.write((json.dumps(x, ensure_ascii=False) if isinstance(x, dict) else x) + "\n")
        err = self.scrub("".join(self.errbuf))
        if os.path.exists(self.err_path):
            os.remove(self.err_path)
        if err.strip():
            open(self.err_path, "w").write(err)
        return self.p.returncode


SCENARIOS = {
    "claude": {"text", "tools", "permission-allow", "permission-deny", "permission-always", "interrupt",
               "interrupt-text", "queue", "resume", "slash", "bad-resume"},
    "agy": {"text", "tools", "resume"},
}
TOOL_PROMPT = ("Do these four things in order, using the matching tools: 1) Read the file notes.txt "
               "2) Edit notes.txt, changing 'alpha' to 'beta' 3) run `ls` with Bash "
               "4) write a todo list with two items, using the TodoWrite tool (or the task-list tool, if "
               "TodoWrite does not exist). Then say done.")
WRITE_PROMPT = "Create a file named hello.txt containing the word hi, using the Write tool. Do nothing else."
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


def main():
    global TRIM, IS_AGY
    if {"-h", "--help"} & set(sys.argv[1:]) or len(sys.argv) < 5:
        print(__doc__)
        sys.exit(0 if len(sys.argv) >= 2 and {"-h", "--help"} & set(sys.argv[1:]) else 2)
    args = [a for a in sys.argv[1:] if a not in ("--trim", "--no-trim")]
    TRIM = "--no-trim" not in sys.argv
    agent, scenario, workdir, out = args[:4]
    sid = args[4] if len(args) > 4 else None
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
    if scenario == "tools":
        open(os.path.join(workdir, "notes.txt"), "w").write("alpha\n")
    policy = {"permission-deny": "deny", "permission-always": "always"}.get(scenario, "allow")
    r = Run(argv, workdir, out, policy)
    failure = None
    try:
        if claude:
            r.send(INIT)
        play(r, scenario)
    except (RuntimeError, KeyboardInterrupt, Exception) as ex:  # keep what was captured, then re-raise
        failure = ex
    finally:
        code = r.finish()  # always writes <out>, and always kills the process group
        r.kill()
    print(f"{agent} {scenario}: exit={code} prompts={r.prompts} out={out}")
    if failure is not None:
        raise failure


if __name__ == "__main__":
    main()
