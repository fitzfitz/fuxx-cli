#!/usr/bin/env python3
"""fuxx benchmark harness (2026-11-09, M11n Task 0). Runs isolated copies of fuxx and Ghostty in a scratch
HOME and only ever signals PIDs it started (and their descendants, found by walking ppid links).

  run.py --bench DIR bundle NAME BINARY    make DIR/apps/NAME.app (single-arch, ad-hoc signed,
                                           bundle id io.github.fitzfitz.fuxx.bench-NAME)
  run.py --bench DIR APP PHASE [--tag N]   APP: fuxx-base | fuxx-m11n | ghostty
                                           PHASE: launch | mem | tput | cpu_base | cpu_noglow | cpu_rest | grid
                                           -> DIR/results/APP-PHASE[N].json
  run.py --bench DIR ghostty-bundle        ditto copy of ~/Applications/Ghostty.app -> DIR/apps/Ghostty-bench.app
                                           (not re-signed; the source is only read)
  run.py --bench DIR summary               the medians (cpu runs counted only when frontmost 15/15)

Ghostty runs from DIR/apps/Ghostty-bench.app (see ghostty-bundle). Its prefs domain is the real Ghostty's, so every
Ghostty phase backs up the user's prefs first and restores + verifies them after (see ghostty_restore).
Every process the harness starts is killed on any exit path (error, SIGTERM, Ctrl-C).
Make the data first: make_data.py DIR.  Self-test of the cleanup paths:
  run.py --bench DIR selftest raise|sigterm|wait|badcomm|badrestore
  (badcomm: a kill is refused; badrestore: that, and the mocked prefs restore exits BLOCKED too)"""
import glob, json, os, re, shutil, signal, statistics, subprocess, sys, time

args = sys.argv[1:]
if len(args) < 2 or args[0] != "--bench":
    sys.exit(__doc__)
B = os.path.abspath(args[1])
args = args[2:]
if not args:
    sys.exit(__doc__)
HERE = os.path.dirname(os.path.abspath(__file__))
for d in ("apps", "bin", "data", "homes", "logs", "results"):
    os.makedirs(f"{B}/{d}", exist_ok=True)
PIDLOG = f"{B}/logs/pids.log"
WININFO = f"{B}/bin/wininfo"
TMP_ROOT = f"/private/tmp/claude-{os.getuid()}"
os.makedirs(TMP_ROOT, exist_ok=True)
CPU_WINDOW, CPU_SKIP = 30, 10          # ps CPU time over 30 s, from 10 s after ready
REST_SKIP = 305                        # cpu_rest: past the breath's 300.8 s rest
FRONT_EVERY = 2                        # s between frontmost checks


def log(*a):
    print(*a, flush=True)


def bin_of(app):
    if app == "ghostty":
        return f"{B}/apps/Ghostty-bench.app/Contents/MacOS/ghostty"
    return f"{B}/apps/{app}.app/Contents/MacOS/fuxx"


def ensure_wininfo():
    if not os.path.exists(WININFO) or os.path.getmtime(WININFO) < os.path.getmtime(f"{HERE}/wininfo.swift"):
        subprocess.run(["swiftc", "-O", f"{HERE}/wininfo.swift", "-o", WININFO], check=True)


# ---------------------------------------------------------------- bundle
def do_bundle(name, binary):
    app = f"{B}/apps/{name}.app"
    shutil.rmtree(app, ignore_errors=True)
    os.makedirs(f"{app}/Contents/MacOS")
    shutil.copy(binary, f"{app}/Contents/MacOS/fuxx")
    repo = os.path.dirname(os.path.dirname(HERE))
    plist = open(f"{repo}/packaging/macos/Info.plist").read()
    plist = plist.replace("@VERSION@", "0.0.0-bench").replace(
        "<string>io.github.fitzfitz.fuxx</string>", f"<string>io.github.fitzfitz.fuxx.bench-{name}</string>")
    if (f"<string>io.github.fitzfitz.fuxx.bench-{name}</string>" not in plist
            or "<string>io.github.fitzfitz.fuxx</string>" in plist):
        shutil.rmtree(app, ignore_errors=True)
        sys.exit("ABORT: the bundle id was not replaced in Info.plist (the bench copy would share the real fuxx's prefs)")
    open(f"{app}/Contents/Info.plist", "w").write(plist)
    subprocess.run(["codesign", "--force", "--sign", "-", app], check=True)
    log("bundle", app, subprocess.run(["lipo", "-archs", binary], capture_output=True, text=True).stdout.strip())


def do_ghostty_bundle():
    src = os.path.expanduser("~/Applications/Ghostty.app")
    dst = f"{B}/apps/Ghostty-bench.app"
    if not os.path.isdir(src):
        sys.exit(f"no {src}")
    shutil.rmtree(dst, ignore_errors=True)
    subprocess.run(["ditto", src, dst], check=True)  # not re-signed: a re-signed copy can't load Sparkle
    log("ghostty-bundle", dst)


# ---------------------------------------------------------------- summary
def med(xs):
    return statistics.median(xs)


def do_summary():
    for app in ("fuxx-base", "fuxx-m11n", "ghostty"):
        R = lambda p: json.load(open(f"{B}/results/{app}-{p}.json")) if os.path.exists(f"{B}/results/{app}-{p}.json") else None
        if not glob.glob(f"{B}/results/{app}-*.json"):
            continue
        l = R("launch")
        if l:
            xs = [x * 1000 for x in l["launch_s"] if x is not None]
            print(f"{app} launch ms {med(xs):.0f} ({min(xs):.0f}-{max(xs):.0f}) {[round(x, 1) for x in xs]}")
        m = R("mem")
        if m:
            for k in ("idle10", "idle60", "tabs10"):
                if k in m:
                    s = m[k]
                    print(f"   {k} rss {s['app_rss_mb']:.1f} fp {s['app_footprint_mb']:.1f} children {len(s['children'])} {s['children_rss_mb']:.1f}")
            print(f"   cpu mem-phase {m['idle_cpu_pct']:.2f} {m['spawned_helpers_30s']}")
        t = R("tput")
        if t:
            for k, label in (("cat100mb", "cat100mb"), ("colors200k", "colors200k")):
                xs = t[k]["s"]
                print(f"   {label} {med(xs):.3f} ({min(xs):.3f}-{max(xs):.3f}) {[round(x, 3) for x in xs]}")
        for phase in ("cpu_base", "cpu_noglow", "cpu_rest"):
            ok, dropped = [], []
            for f in sorted(glob.glob(f"{B}/results/{app}-{phase}*.json")):
                r = json.load(open(f))
                (ok if r.get("frontmost_ok") else dropped).append((os.path.basename(f), round(r["idle_cpu_pct"], 2), r.get("frontmost")))
            if ok or dropped:
                v = [x[1] for x in ok]
                head = f"median {med(v):.2f}" if v else "no run counted"
                print(f"   {phase}: {head}  counted {len(ok)} (15/15 frontmost), dropped {len(dropped)}")
                print(f"      runs {[(n, c) for n, c, _ in ok]}")
                if dropped:
                    print(f"      dropped {[(n, c, fr) for n, c, fr in dropped]}")


# ---------------------------------------------------------------- runtime
if args[0] == "bundle":
    if len(args) != 3:
        sys.exit(__doc__)
    do_bundle(args[1], os.path.abspath(args[2]))
    sys.exit(0)
if args[0] == "ghostty-bundle":
    if len(args) != 1:
        sys.exit(__doc__)
    do_ghostty_bundle()
    sys.exit(0)
if args[0] == "summary":
    do_summary()
    sys.exit(0)

SELFTEST = args[0] == "selftest"
if SELFTEST:
    app, phase = "selftest", "selftest"
    selftest_mode = args[1] if len(args) > 1 else "raise"
elif len(args) < 2:
    sys.exit(__doc__)
else:
    app, phase = args[0], args[1]
if "--tag" in args and args.index("--tag") + 1 >= len(args):
    sys.exit(__doc__)
tag = args[args.index("--tag") + 1] if "--tag" in args else ""
if app not in ("fuxx-base", "fuxx-m11n", "ghostty", "selftest"):
    sys.exit("APP must be fuxx-base | fuxx-m11n | ghostty")
if app == "ghostty" and phase in ("cpu_noglow", "cpu_rest"):
    sys.exit(f"{phase} is a fuxx phase (the glow); not for ghostty")
BIN = bin_of(app) if not SELFTEST else "/bin/sleep"
REFUSED_KILL = SELFTEST and selftest_mode in ("badcomm", "badrestore")
ALLOWED = (B,) + (("/bin/sleep",) if SELFTEST and not REFUSED_KILL else ())
OUT = f"{B}/results/{app}-{phase}{tag}.json"
HOME_REAL = f"{B}/homes/{app}"
HOME = f"{TMP_ROOT}/fxb-{app}"  # short symlink -> DIR/homes/APP (fuxx's socket path limit)
ensure_wininfo()


def setup_home():
    shutil.rmtree(HOME_REAL, ignore_errors=True)
    for d in ("tmp", "zdot", ".config/fuxx", ".config/ghostty", "Library/Application Support/fuxx", "Library/Preferences"):
        os.makedirs(f"{HOME_REAL}/{d}")
    if os.path.islink(HOME) or os.path.exists(HOME):
        os.remove(HOME)
    os.symlink(HOME_REAL, HOME)
    # zsh rc: the ready marker (needs zsh/datetime for $EPOCHREALTIME) and the phase's command.
    open(f"{HOME_REAL}/zdot/.zshrc", "w").write(
        "zmodload zsh/datetime\nPS1='%# '\n"
        '[[ -n $BENCH_MARKER ]] && print $EPOCHREALTIME > $BENCH_MARKER\n'
        '[[ -n $BENCH_CMD ]] && eval $BENCH_CMD\n')
    if app == "ghostty":
        open(f"{HOME_REAL}/.config/ghostty/config", "w").write(
            "window-save-state = never\nconfirm-close-surface = false\nauto-update = off\nshell-integration = none\n")
    else:
        glow = "off" if phase == "cpu_noglow" else "on"
        open(f"{HOME_REAL}/.config/fuxx/config", "w").write(f"cursor-glow = {glow}\n")
    seed_state()


def seed_state():
    # A missing sessions.json shows the Welcome card (M11k), which takes the keyboard from the terminal
    # (the glow never runs); {"version":2} is "welcomed" and has no sessions.
    if app != "ghostty":
        open(f"{HOME_REAL}/Library/Application Support/fuxx/sessions.json", "w").write('{"version":2}')


def env(extra=None):
    e = {
        "HOME": HOME, "USER": os.environ.get("USER", ""), "LOGNAME": os.environ.get("USER", ""),
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "SHELL": "/bin/zsh", "LANG": "en_US.UTF-8",
        "TMPDIR": f"{HOME}/tmp/", "XDG_CONFIG_HOME": f"{HOME}/.config",
        "XDG_DATA_HOME": f"{HOME}/.local/share", "XDG_CACHE_HOME": f"{HOME}/.cache",
        "XDG_STATE_HOME": f"{HOME}/.local/state", "CFFIXED_USER_HOME": HOME,
        "ZDOTDIR": f"{HOME}/zdot",
    }
    e.update(extra or {})
    return e


def ps_table():
    out = subprocess.run(["/bin/ps", "-axo", "pid=,ppid=,rss=,time=,comm="], capture_output=True, text=True).stdout
    rows = {}
    for line in out.splitlines():
        p = line.split(None, 4)
        if len(p) == 5:
            rows[int(p[0])] = (int(p[1]), int(p[2]), p[3], p[4])
    return rows


def descendants(pid, rows=None):
    rows = rows or ps_table()
    kids, frontier = [], [pid]
    while frontier:
        cur = frontier.pop()
        for p, (pp, *_r) in rows.items():
            if pp == cur and p not in kids:
                kids.append(p)
                frontier.append(p)
    return kids


def cpu_secs(t):
    s = 0.0
    for x in t.replace("-", ":").split(":"):
        s = s * 60 + float(x)
    return s


STARTED = set()
PROCS = []  # every Popen this run starts


def on_sigterm(signum, frame):
    sys.exit(143)  # runs the top-level finally (kills our children, restores prefs)


signal.signal(signal.SIGTERM, on_sigterm)
signal.signal(signal.SIGINT, signal.default_int_handler)  # also when started in the background (SIGINT ignored)


def launch(extra=None):
    seed_state()
    if extra and "BENCH_MARKER" in extra and os.path.exists(extra["BENCH_MARKER"]):
        os.remove(extra["BENCH_MARKER"])
    t0 = time.time()
    p = subprocess.Popen([BIN] + (["60"] if SELFTEST else []), env=env(extra), cwd=HOME, stdin=subprocess.DEVNULL,
                         stdout=open(f"{B}/logs/{app}-stdout.log", "ab"), stderr=subprocess.STDOUT,
                         start_new_session=True)
    STARTED.add(p.pid)
    PROCS.append(p)
    with open(PIDLOG, "a") as f:
        f.write(f"{time.strftime('%H:%M:%S')} {app} {phase} started pid {p.pid}\n")
    return p, t0


def kill(p):
    rows = ps_table()
    kids = descendants(p.pid, rows)
    comm = rows.get(p.pid, (0, 0, "", ""))[3]
    if p.pid not in STARTED or (comm and not comm.startswith(ALLOWED)):
        msg = f"REFUSING to signal pid {p.pid} ({comm!r}): not a process this harness started; it is LEFT RUNNING"
        print("!!!", msg, file=sys.stderr, flush=True)
        with open(PIDLOG, "a") as f:
            f.write(f"{time.strftime('%H:%M:%S')} {msg}\n")
        raise RuntimeError(msg)
    try:
        os.kill(p.pid, signal.SIGTERM)
        p.wait(5)
    except subprocess.TimeoutExpired:
        os.kill(p.pid, signal.SIGKILL)
        p.wait(5)
    except ProcessLookupError:
        pass
    time.sleep(0.5)
    rows2 = ps_table()
    left = [k for k in kids if k in rows2 and rows2[k][3] == rows[k][3]]
    for k in left:
        try:
            os.kill(k, signal.SIGKILL)
        except ProcessLookupError:
            pass
    with open(PIDLOG, "a") as f:
        f.write(f"{time.strftime('%H:%M:%S')} {app} {phase} killed pid {p.pid} (descendants {kids}, leftover killed {left})\n")


def cleanup_all():
    """Kill every process this run started that is still alive (the apps run in their own sessions, so a
    signal to the harness does not reach them). Never raises; returns the pids it could not kill."""
    bad = []
    for p in PROCS:
        if p.poll() is None:
            try:
                kill(p)
            except BaseException as e:
                print(f"!!! cleanup could not kill pid {p.pid}: {e}", file=sys.stderr, flush=True)
                bad.append(p.pid)
    return bad


def wait_ready(mk):
    v = wait_file(mk, 30)
    if v is None:
        raise RuntimeError(f"the shell never wrote the ready marker {mk} (app did not start a shell in 30 s)")
    return float(v)


def wait_file(path, timeout):
    end = time.time() + timeout
    while time.time() < end:
        if os.path.exists(path) and os.path.getsize(path) > 0:
            time.sleep(0.05)
            return open(path).read().strip()
        time.sleep(0.01)
    return None


def wininfo(pid):
    try:
        return json.loads(subprocess.run([WININFO, str(pid)], capture_output=True, text=True, timeout=10).stdout)
    except Exception as e:
        return {"error": str(e)}


def is_front(pid):
    w = wininfo(pid)
    return w.get("frontmost") == pid and bool(w.get("windows"))


def footprint(pid):
    out = subprocess.run(["/usr/bin/footprint", str(pid)], capture_output=True, text=True).stdout
    m = re.search(r"Footprint: ([\d.]+) (KB|MB|GB)", out)
    if not m:
        return None
    v = float(m.group(1))
    return v / 1024 if m.group(2) == "KB" else v * 1024 if m.group(2) == "GB" else v


def mem_snapshot(pid):
    rows = ps_table()
    kids = descendants(pid, rows)
    return {
        "app_rss_mb": rows[pid][1] / 1024 if pid in rows else None,
        "app_footprint_mb": footprint(pid),
        "children": [{"pid": k, "comm": rows[k][3], "rss_mb": rows[k][1] / 1024} for k in kids],
        "children_rss_mb": sum(rows[k][1] for k in kids) / 1024,
    }


def marker_path(tag):
    return f"{HOME}/tmp/marker-{tag}"


def sleep_until(t):
    while time.time() < t:
        time.sleep(0.05)


def run_launch(n, warm):
    res, res_win = [], None
    for i in range(n + warm):
        mk = marker_path(f"l{i}")
        p, t0 = launch({"BENCH_MARKER": mk})
        v = wait_file(mk, 30)
        dt = float(v) - t0 if v else None  # a missing marker is recorded as None, loudly
        log(f"launch run {i} ({'warm-up' if i < warm else 'measured'}): {dt}")
        if i >= warm:
            res.append(dt)
        time.sleep(1.0)
        if i == warm:
            res_win = wininfo(p.pid)
        kill(p)
        if os.path.exists(mk):
            os.remove(mk)
        time.sleep(2)
    return {"launch_s": res, "wininfo": res_win}


def run_mem():
    mk = marker_path("mem")
    p, t0 = launch({"BENCH_MARKER": mk})
    t_ready = wait_ready(mk)
    r = {"launch_s": t_ready - t0}
    sleep_until(t_ready + 10)
    r["idle10"] = mem_snapshot(p.pid)
    r["wininfo"] = wininfo(p.pid)
    log("idle10", r["idle10"])
    rows = ps_table()
    c0 = cpu_secs(rows[p.pid][2]); w0 = time.time()
    seen = {}
    while time.time() < w0 + 30:
        rows = ps_table()
        for k in descendants(p.pid, rows):
            comm = rows[k][3]
            if not comm.endswith("zsh"):
                seen.setdefault(k, comm)
        time.sleep(0.05)
    rows = ps_table()
    c1 = cpu_secs(rows[p.pid][2]); w1 = time.time()
    r["idle_cpu_pct"] = 100 * (c1 - c0) / (w1 - w0)
    spawned = {}
    for comm in seen.values():
        spawned[comm] = spawned.get(comm, 0) + 1
    r["spawned_helpers_30s"] = spawned
    log("cpu", r["idle_cpu_pct"], spawned)
    sleep_until(t_ready + 60)
    r["idle60"] = mem_snapshot(p.pid)
    log("idle60", r["idle60"])
    if app.startswith("fuxx"):
        ids = []
        for i in range(9):
            out = subprocess.run([BIN, "new", "--tab"], env=env(), capture_output=True, text=True, timeout=20)
            ids.append((out.returncode, out.stdout.strip(), out.stderr.strip()))
            time.sleep(0.3)
        r["tab_cmds"] = ids
        ls = subprocess.run([BIN, "ls"], env=env(), capture_output=True, text=True, timeout=20)
        r["ls"] = ls.stdout
        time.sleep(10)
        r["tabs10"] = mem_snapshot(p.pid)
        log("tabs10", r["tabs10"], ids, ls.stdout)
    kill(p)
    return r


def run_tput(n):
    r = {}
    for name, f in [("cat100mb", f"{B}/data/big.txt"), ("colors200k", f"{B}/data/colors.txt")]:
        times, wins = [], []
        for i in range(n):
            mk, rf = marker_path(f"t{name}{i}"), f"{HOME}/tmp/result-{name}{i}"
            cmd = f"sleep 2; print $COLUMNS $LINES > {rf}.grid; s=$EPOCHREALTIME; cat {f}; e=$EPOCHREALTIME; print $((e-s)) > {rf}"
            p, t0 = launch({"BENCH_MARKER": mk, "BENCH_CMD": cmd})
            wait_ready(mk)
            time.sleep(2.8)
            wins.append(wininfo(p.pid))
            v = wait_file(rf, 300)
            times.append(float(v) if v else None)
            grids = r.setdefault("grid", [])
            grids.append(open(rf + ".grid").read().strip() if os.path.exists(rf + ".grid") else None)
            log(name, i, v, wins[-1])
            time.sleep(0.5)
            kill(p)
            time.sleep(2)
        r[name] = {"s": times, "wininfo": wins}
    return r


def run_cpu_once():
    """One cpu run: ps CPU time over a 30 s window; frontmost checked every 2 s over the window
    (15 checks), or over the whole 335 s for cpu_rest. frontmost_ok only when every check passed."""
    rest = phase == "cpu_rest"
    mk = marker_path("cpu")
    p, t0 = launch({"BENCH_MARKER": mk})
    t_ready = wait_ready(mk)
    start = t_ready + (REST_SKIP if rest else CPU_SKIP)
    check_from = t_ready if rest else start
    end = start + CPU_WINDOW
    checks = []
    c0 = w0 = c1 = w1 = None
    nxt = check_from
    while time.time() < end:
        now = time.time()
        if c0 is None and now >= start:
            c0 = cpu_secs(ps_table()[p.pid][2]); w0 = now
        if now >= nxt:
            checks.append(is_front(p.pid))
            nxt += FRONT_EVERY
        time.sleep(0.05)
    rows = ps_table(); c1 = cpu_secs(rows[p.pid][2]); w1 = time.time()
    res = {"idle_cpu_pct": 100 * (c1 - c0) / (w1 - w0), "wininfo": wininfo(p.pid),
           "frontmost": f"{sum(checks)}/{len(checks)}", "frontmost_ok": all(checks) and len(checks) >= 15}
    log(res)
    kill(p)
    return res


GHOSTTY_DOMAIN = "com.mitchellh.ghostty"


def plain(src, dst):
    shutil.copy(src, dst)
    subprocess.run(["plutil", "-convert", "xml1", dst], check=True)


def ghostty_backup():
    """The bench Ghostty copy keeps Ghostty's bundle id and cfprefsd ignores CFFIXED_USER_HOME, so it writes
    the user's real prefs. Back them up before the phase; ghostty_restore puts them back and verifies."""
    os.makedirs(f"{B}/prefs", exist_ok=True)
    n = len(glob.glob(f"{B}/prefs/ghostty-before-{phase}{tag}-*.plist")) + 1
    path = f"{B}/prefs/ghostty-before-{phase}{tag}-{n}.plist"
    subprocess.run(["defaults", "export", GHOSTTY_DOMAIN, path], check=True)
    return path


def ghostty_restore(path):
    if SELFTEST:
        alive = [p.pid for p in PROCS if p.poll() is None]
        log(f"SELFTEST ghostty_restore called; started children still alive at that moment: {alive}")
        with open(f"{B}/logs/prefs-restore.log", "a") as f:
            f.write(f"selftest restore called, children alive: {alive}\n")
        if selftest_mode == "badrestore":
            sys.exit("BLOCKED: selftest: the prefs restore failed")
        return
    end = time.time() + 15
    while time.time() < end:  # every bench Ghostty process must have exited first
        if not any(r[3].startswith(f"{B}/apps/Ghostty-bench.app") for r in ps_table().values()):
            break
        time.sleep(0.3)
    else:
        sys.exit("BLOCKED: a bench Ghostty process is still running")
    subprocess.run(["defaults", "import", GHOSTTY_DOMAIN, path], check=True)
    now = f"{B}/prefs/now.plist"
    subprocess.run(["defaults", "export", GHOSTTY_DOMAIN, now], check=True)
    a, b = f"{B}/prefs/a.xml", f"{B}/prefs/b.xml"
    plain(path, a)
    plain(now, b)
    same = subprocess.run(["cmp", a, b]).returncode == 0
    with open(f"{B}/logs/prefs-restore.log", "a") as f:
        f.write(f"{time.strftime('%H:%M:%S')} ghostty {phase}{tag} restore {'EQUAL' if same else 'DIFFERENT'} ({path})\n")
    log("ghostty prefs restore:", "equal to backup" if same else "DIFFERENT")
    if not same:
        sys.exit("BLOCKED: Ghostty prefs differ from the backup after restore")


backup = ghostty_backup() if app == "ghostty" else ("selftest-mock" if SELFTEST else None)
failed = []
try:
    setup_home()
    if SELFTEST:
        p, _ = launch()
        log("selftest child", p.pid, "mode", selftest_mode)
        if selftest_mode == "raise":
            raise RuntimeError("selftest: error mid-phase")
        if selftest_mode == "sigterm":
            os.kill(os.getpid(), signal.SIGTERM)
            time.sleep(5)
        if REFUSED_KILL:
            kill(p)  # refused: raises
        time.sleep(30)  # wait: for an external SIGTERM / SIGINT
        res = {}
    elif phase.startswith("cpu"):
        tries = []
        for t in range(5):
            r = run_cpu_once()
            tries.append(r)
            if r["frontmost_ok"]:
                break
            log(f"try {t + 1}: frontmost {r['frontmost']}, repeating")
            time.sleep(2)
        res = dict(tries[-1])
        res["tries"] = [{"idle_cpu_pct": x["idle_cpu_pct"], "frontmost": x["frontmost"]} for x in tries]
    elif phase == "grid":
        mk = marker_path("grid"); gf = f"{HOME}/tmp/gridfile"
        p, t0 = launch({"BENCH_MARKER": mk, "BENCH_CMD": f"sleep 1; print $COLUMNS $LINES > {gf}"})
        v = wait_file(gf, 30)
        if v is None:
            raise RuntimeError("grid: the shell never wrote the grid file")
        log("grid", v, wininfo(p.pid)); os.remove(gf); kill(p); res = {"grid": v}
    elif phase == "launch":
        res = run_launch(5, 1)
    elif phase == "mem":
        res = run_mem()
    elif phase == "tput":
        res = run_tput(3)
    else:
        sys.exit(f"unknown phase {phase}")
finally:
    failed = cleanup_all()  # first the apps, then the prefs restore (a live Ghostty would rewrite them)
    if backup:
        try:
            ghostty_restore(backup)
        except BaseException as e:
            if not failed:
                raise
            # The restore's own exit must not hide the processes left running.
            why = e.code if isinstance(e, SystemExit) else f"{type(e).__name__}: {e}"
            sys.exit(f"{why}\nBLOCKED: could not kill {failed}")
    if failed:
        sys.exit(f"BLOCKED: could not kill {failed}")
json.dump(res, open(OUT, "w"), indent=1)
log("wrote", OUT)
