#!/usr/bin/env python3
"""Record a real headless agent session as a JSONL fixture (stdlib only).

Usage: python3 -I scripts/record-agent-fixture.py <claude|agy> <scenario> <workdir> <out> [session-id]

Scenarios (claude): text, tools, permission-allow, permission-deny, permission-always,
  interrupt, interrupt-text, queue, resume (needs session-id), slash, bad-resume
Scenarios (agy): text, tools, resume (needs session-id = conversation id)
agy: `-p=` and {"event":"user","message":{"content":...}} input (see comments).

Run each recording in a fresh temp dir, never in the repo. The user's own hooks run (nothing
is switched off). stdout lines are scrubbed (workdir -> /tmp/fx, $HOME -> ~, thinking
`signature` -> "") and written to <out>; stderr, if any, goes to <out>.stderr.
"""
import json, os, queue, signal, subprocess, sys, threading, time

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
    return False


def user(text):
    if IS_AGY:  # agy input events are {"event": ...}; "role" in the message is rejected
        return {"event": "user", "message": {"content": text}}
    return {"type": "user", "message": {"role": "user", "content": text}}


class Run:
    def __init__(self, argv, workdir, out, policy="allow"):
        self.workdir, self.policy = os.path.realpath(workdir), policy
        self.raw_workdir = workdir
        self.out_path = out
        self.lines = []  # scrubbed output lines (str, or parsed dict while buffered)
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
        return s.replace(os.path.basename(os.path.expanduser("~")), "user")  # e.g. `ls -l` owners

    def _clean(self, o):
        """In place: blank thinking signatures, hide the account identity."""
        if isinstance(o, dict):
            if o.get("type") == "thinking" and "signature" in o:
                o["signature"] = ""
            if isinstance(o.get("account"), dict):
                o["account"].update({k: v for k, v in (("email", "user@example.com"),
                                                       ("organization", "Example Org"))
                                     if k in o["account"]})
            for v in o.values():
                self._clean(v)
        elif isinstance(o, list):
            for v in o:
                self._clean(v)

    def _scrub_obj(self, o):
        if isinstance(o, dict):
            return {k: self._scrub_obj(v) for k, v in o.items()}
        if isinstance(o, list):
            return [self._scrub_obj(v) for v in o]
        return self.scrub(o) if isinstance(o, str) else o

    def _write(self):
        """Scrub every string. Streamed tool input arrives as `partial_json` fragments that can
        split a path, so per content block: join the fragments, scrub, and if that changed
        anything put the whole scrubbed text in the first fragment and "" in the rest."""
        evs = [self._scrub_obj(x) if isinstance(x, dict) else x for x in self.lines]
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
                self._clean(e)
                text = json.dumps(e, ensure_ascii=False)
            except ValueError:
                e, text = {}, line.rstrip("\n")
            self.lines.append(e if e else self.scrub(text))
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

    def finish(self):
        try:
            self.p.stdin.close()
        except Exception:
            pass
        try:
            self.p.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass
        try:
            os.killpg(self.p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
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


def main():
    agent, scenario, workdir, out = sys.argv[1:5]
    sid = sys.argv[5] if len(sys.argv) > 5 else None
    claude = agent == "claude"
    global IS_AGY
    IS_AGY = not claude
    argv = list(CLAUDE if claude else AGY)
    if scenario == "bad-resume":
        sid = ZERO
    if scenario in ("resume", "bad-resume"):
        if not sid:
            sys.exit("resume needs a session id as the 5th argument")
        argv += ["--resume", sid] if claude else ["--conversation", sid]
    policy = {"permission-deny": "deny", "permission-always": "always"}.get(scenario, "allow")
    r = Run(argv, workdir, out, policy)
    if claude:
        r.send(INIT)
    tool_prompt = ("Do these four things in order, using the matching tools: 1) Read the file notes.txt "
                   "2) Edit notes.txt, changing 'alpha' to 'beta' 3) run `ls` with Bash "
                   "4) write a todo list with two items, using the TodoWrite tool (or the task-list tool, if "
                   "TodoWrite does not exist). Then say done.")
    write_prompt = "Create a file named hello.txt containing the word hi, using the Write tool. Do nothing else."
    if scenario in ("tools",):
        open(os.path.join(workdir, "notes.txt"), "w").write("alpha\n")
    if scenario == "text" or scenario == "bad-resume":
        r.send(user("Say hello in one word"))
        r.result()
    elif scenario == "tools":
        r.send(user(tool_prompt))
        r.result()
    elif scenario in ("permission-allow", "permission-deny"):
        r.send(user(write_prompt))
        r.result()
    elif scenario == "permission-always":
        r.send(user(write_prompt))
        r.result()
        r.send(user("Now create hello2.txt containing the word hi, using the Write tool. Do nothing else."))
        r.result()
    elif scenario == "interrupt":
        r.send(user("Run `python3 -c 'import time; time.sleep(60)'` with the Bash tool in the foreground (do not background it), then say done."))
        tool_starts(r)
        r.send(STOP)
        r.result(40)
        r.pump(None, 6)  # whatever arrives after the interrupt
    elif scenario == "interrupt-text":  # interrupt while the model is streaming text
        r.send(user("Write a 500-word essay about trains. No tools."))
        r.pump(lambda e: e.get("type") == "stream_event"
               and e["event"].get("delta", {}).get("type") == "text_delta")
        r.send(STOP)
        r.result(40)
        r.pump(None, 6)
    elif scenario == "queue":
        r.send(user("Run `python3 -c 'import time; time.sleep(20)'` with the Bash tool in the foreground (do not background it), then say done."))
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
    else:
        sys.exit("unknown scenario " + scenario)
    if scenario == "bad-resume":
        pass
    code = r.finish()
    print(f"{agent} {scenario}: exit={code} prompts={r.prompts} out={out}")


main()
