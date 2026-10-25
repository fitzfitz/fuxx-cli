// fuxx mockups — shared behavior. Plain JS, no dependencies.
// Mirrors the planned rules where a mockup can: session switching (M3), "typing into a settled
// session acknowledges it" (M2), and focus-aware notifications (M4: notifier::should_notify).

(function () {
  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => Array.from(root.querySelectorAll(sel));

  // Same text as notifier::message() in the M4 plan.
  const MESSAGE = { done: "Finished", waiting: "Needs your input" };

  // ---------- feature markers ----------
  const toggle = $("#toggle-marks");
  if (toggle) {
    toggle.addEventListener("click", () => {
      const hidden = document.body.classList.toggle("no-marks");
      toggle.textContent = hidden ? "Show feature markers" : "Hide feature markers";
    });
  }
  function hot(n, on) {
    $$(`.mk[data-n="${n}"]`).forEach((m) => m.classList.toggle("hot", on));
    $$(`.legend li[data-n="${n}"]`).forEach((li) => li.classList.toggle("hot", on));
  }
  $$(".legend li[data-n], .mk[data-n]").forEach((el) => {
    el.addEventListener("mouseenter", () => hot(el.dataset.n, true));
    el.addEventListener("mouseleave", () => hot(el.dataset.n, false));
  });

  // ---------- toast ----------
  let toastTimer;
  function toast(text) {
    const t = $("#toast");
    if (!t) return;
    t.textContent = text;
    t.classList.remove("gone");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => t.classList.add("gone"), 2600);
  }

  // ---------- sessions ----------
  const rows = () => $$(".row[data-s]");
  const rowFor = (s) => $(`.row[data-s="${s}"]`);
  const labelOf = (row) => ($(".label", row) || row).textContent.trim();
  const ringOf = (row) => $(".ring", row);
  const statusOf = (row) => ["idle", "working", "waiting", "done"].find((c) => ringOf(row).classList.contains(c));

  function setStatus(row, status) {
    const ring = ringOf(row);
    ring.classList.remove("idle", "working", "waiting", "done");
    ring.classList.add(status);
    ring.title = status[0].toUpperCase() + status.slice(1);
  }

  function select(s) {
    const row = rowFor(s);
    if (!row) return;
    rows().forEach((r) => r.classList.toggle("active", r === row));
    $$(".term[data-s]").forEach((t) => (t.hidden = t.dataset.s !== String(s)));
    const title = $(".titlebar .title");
    if (title) title.textContent = `fuxx — ${labelOf(row)}`;
  }
  rows().forEach((row) => row.addEventListener("click", () => select(row.dataset.s)));

  // M2: typing while Waiting/Done resets the active session to Idle.
  document.addEventListener("keydown", (e) => {
    if (e.metaKey || e.ctrlKey || e.altKey || e.key.length !== 1) return;
    const active = $(".row.active[data-s]");
    if (!active) return;
    const st = statusOf(active);
    if (st === "waiting" || st === "done") {
      setStatus(active, "idle");
      toast(`You typed into "${labelOf(active)}", so it's acknowledged and back to Idle`);
    }
  });

  // ---------- notifications ----------
  const banner = $("#banner");
  let bannerTimer;
  function showBanner(row, status) {
    if (!banner) return;
    $(".ttl", banner).textContent = `fuxx — ${labelOf(row)}`;
    $(".msg", banner).textContent = MESSAGE[status];
    banner.dataset.jump = row.dataset.s;
    banner.classList.remove("gone");
    clearTimeout(bannerTimer);
    bannerTimer = setTimeout(() => banner.classList.add("gone"), 6000);
  }

  // M4: notify only on a change into Done/Waiting, and stay silent for the session you're looking at.
  function simulate(s, status) {
    const row = rowFor(s);
    if (!row) return;
    const old = statusOf(row);
    setStatus(row, status);
    if (old === status || !MESSAGE[status]) return;
    if (row.classList.contains("active") && document.hasFocus()) {
      toast(`No banner: you're already looking at "${labelOf(row)}" (focus-aware)`);
      return;
    }
    showBanner(row, status);
  }
  $$("[data-sim]").forEach((btn) =>
    btn.addEventListener("click", () => {
      const [s, status] = btn.dataset.sim.split(":");
      simulate(s, status);
    })
  );

  if (banner) {
    banner.addEventListener("click", () => {
      banner.classList.add("gone");
      if (banner.classList.contains("clickable")) {
        select(banner.dataset.jump);
        toast("Clicked the notification and jumped to that session (M11)");
      } else {
        toast("Until M11 a click opens Script Editor, not fuxx (osascript limit, decision D5)");
      }
    });
  }
})();
