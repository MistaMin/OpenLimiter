#!/usr/bin/env python3
"""Host parameter sweep for OpenLimiter, one module at a time, in several hidden REAPERs at once.

The generic suite (plugin-daw-test, reaper_qa.py) tests every parameter with the plugin at its
defaults. In OpenLimiter only the first limiter is on by default, so every other module's
parameters render nothing and fall through to its random-state retry, which sets all ~2000
parameters per state: days of host time. This driver tests what the user does instead:

  1. One module switched on (the first limiter switched off unless it is the one under test).
  2. Each parameter at its minimum and maximum, compared with that module's default render.
     Different = ok. Identical = suspect.
  3. Suspects are retried in up to 5 random states of their own neighbourhood (the chunk of the
     module being tested and 40 parameters either side, inside that module): acting in one makes
     the parameter CONDITIONAL (it needs another setting, like a band switched on). Still
     identical = DEAD.

The bridge, launch, quit, test signal and comparisons are reaper_qa.py's own, unchanged.
Parameter positions come from `build/layout.json` (`cargo test --release --lib -- --ignored dump_layout`).

  module_sweep.py --out DIR [--workers 4] [--seconds 2] [--only key,key] [--chunk 40]
"""

import argparse
import json
import os
import queue
import random
import subprocess
import sys
import threading
import time

SKILL = os.environ.get("DAW_TEST_SCRIPTS") or sys.exit("set DAW_TEST_SCRIPTS to the folder holding reaper_qa.py")
sys.path.insert(0, SKILL)
import reaper_qa as rq  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.dirname(os.path.dirname(HERE))
FX = "VST3: OpenLimiter (OpenGrid)"

# Parameters that do not touch the audio, each with its reason.
NON_AUDIO = {
    "rackShown": "which modules the rack shows; editor only, hidden from automation",
    "inputLink": "gangs the two input-gain controls in the editor; the gains themselves are tested",
    "headroom": "kept so old sessions load; the setup sheet says it has no effect",
    "delayUnit": "how the channel delay is shown (ms or frames); the delay is stored in ms and settings_from never reads it",
}
# Chain order: needs two modules on; covered by the chain's unit tests (position_decides_the_order).
ORDER_REASON = "chain order needs two modules on; covered by unit test position_decides_the_order_and_any_values_are_valid"


def groups(layout):
    """(name, params to test, context {index: normalized}) per module and per section of the shell."""
    ids = layout["ids"]
    slots = layout["slots"]
    classic_on = slots[0]["on"]
    out = []
    for s in slots:
        ctx = {s["on"]: 1.0}
        if s["key"]:
            ctx[classic_on] = 0.0
        out.append((s["key"] or "classic", list(range(s["base"], s["base"] + s["count"])) + [s["on"]], ctx, s))
    d0, dn = layout["dither_base"], layout["dither_count"]
    # The word length starts at Off; the other dither settings act only with a word length chosen.
    out.append(("requantise", list(range(d0, d0 + dn)), {d0: 0.3}, None))
    out.append(("input", list(layout["input_gain"]) + [layout["input_link"], layout["rack_shown"]], {}, None))
    out.append(("monitor", [layout["autogain"], layout["delta"]], {}, None))
    return out, ids


class Worker(threading.Thread):
    def __init__(self, n, args, jobs, results, defaults_holder):
        super().__init__(daemon=True)
        self.n, self.args, self.jobs, self.results = n, args, jobs, results
        self.dir = os.path.join(args.out, "w%d" % n)
        self.defaults_holder = defaults_holder
        self.count = 0

    def log(self, text):
        line = "[w%d %s] %s" % (self.n, time.strftime("%H:%M:%S"), text)
        print(line, flush=True)
        with open(os.path.join(self.args.out, "sweep.log"), "a") as f:
            f.write(line + "\n")

    def render(self):
        self.count += 1
        name = "r%d" % self.count
        path = os.path.join(self.dir, name + ".wav")
        if os.path.exists(path):
            os.remove(path)
        path = self.b.call("render", seconds=self.args.seconds, name=name, timeout=300)["file"]
        for _ in range(200):
            if os.path.exists(path) and os.path.getsize(path) > 44:
                break
            time.sleep(0.05)
        return path

    def set(self, index, value):
        if abs(self.cur.get(index, -1.0) - value) > 1e-9:
            self.b.call("set_param", index=index, value=value)
            self.cur[index] = value

    def restore(self, keep=()):
        for i, v in list(self.cur.items()):
            if i not in keep and abs(v - self.defaults[i]) > 1e-9:
                self.set(i, self.defaults[i])

    def run(self):
        os.makedirs(self.dir, exist_ok=True)
        try:
            subprocess.run([sys.executable, os.path.join(SKILL, "reaper_qa.py"), "launch", "--qa-dir", self.dir],
                           check=True, capture_output=True, text=True)
            self.b = rq.Bridge(self.dir)
            self.b.call("add_fx", name=FX)
            self.b.call("add_audio", path=self.args.signal)
            params = self.b.call("params")["params"]
            self.defaults = {p["index"]: p["value"] for p in params}
            self.defaults_holder.setdefault("n", len(params))
            self.cur = dict(self.defaults)
            self.log("ready, %d parameters" % len(params))
            while True:
                try:
                    job = self.jobs.get_nowait()
                except queue.Empty:
                    break
                try:
                    self.results.extend(self.do(*job))
                except Exception as e:  # noqa: BLE001 - report and go on with the next job
                    self.log("job %s failed: %s" % (job[0], e))
                    self.results.extend({"group": job[0], "index": i, "audio": "SKIPPED (worker error: %s)" % e}
                                        for i in job[1])
        except Exception as e:  # noqa: BLE001 - its jobs stay in the queue for the others
            self.log("worker stopped: %s" % e)
        finally:
            subprocess.run([sys.executable, os.path.join(SKILL, "reaper_qa.py"), "quit", "--qa-dir", self.dir],
                           capture_output=True)

    def do(self, group, todo, ctx, span, ids):
        t0 = time.time()
        self.restore()
        for i, v in ctx.items():
            self.set(i, v)
        ref_path = self.render()
        ref = rq.analyse(ref_path)
        rows, suspects = [], []
        for i in todo:
            pid = ids[i] if i < len(ids) else str(i)
            short = pid.split(".")[-1]
            row = {"group": group, "index": i, "id": pid}
            if short in NON_AUDIO:
                row["audio"], row["reason"] = "non-audio", NON_AUDIO[short]
                rows.append(row)
                continue
            if short == "position":
                row["audio"], row["reason"] = "unit test", ORDER_REASON
                rows.append(row)
                continue
            if ref["peakDb"] <= rq.SILENCE_DB:
                row["audio"] = "SILENT (reference render silent)"
                rows.append(row)
                continue
            diffs = []
            for v in (0.0, 1.0):
                self.set(i, v)
                p = self.render()
                diffs.append(rq.difference_db(ref_path, p))
                if not rq.analyse(p)["finite"]:
                    row["nonfinite"] = v
                os.remove(p)
            self.set(i, ctx.get(i, self.defaults[i]))
            row["diffDb"] = round(max(diffs), 1)
            row["audio"] = "ok" if max(diffs) > rq.SAME_DB else "suspect"
            if row["audio"] == "suspect":
                suspects.append(row)
            rows.append(row)
        os.remove(ref_path)
        # Retry the suspects in random states of their neighbourhood.
        lo, hi = span
        hood = [i for i in range(max(lo, min(todo) - 40), min(hi, max(todo) + 41)) if i not in ctx]
        for seed in range(1, 6):
            left = [r for r in suspects if r["audio"] == "suspect"]
            if not left:
                break
            rnd = random.Random(seed * 7919 + lo)
            state = {i: rnd.random() for i in hood}
            for i, v in state.items():
                self.set(i, v)
            rp = self.render()
            if rq.analyse(rp)["peakDb"] <= rq.SILENCE_DB:
                os.remove(rp)
                continue
            for r in left:
                d = []
                for v in (0.0, 1.0):
                    self.set(r["index"], v)
                    p = self.render()
                    d.append(rq.difference_db(rp, p))
                    os.remove(p)
                self.set(r["index"], state.get(r["index"], self.defaults[r["index"]]))
                if max(d) > rq.SAME_DB:
                    r["audio"], r["diffDb"], r["state"] = "CONDITIONAL", round(max(d), 1), seed
            os.remove(rp)
        for r in suspects:
            if r["audio"] == "suspect":
                r["audio"] = "DEAD"
        self.log("%s %d-%d: %d params, %d suspect, %d dead, %.0f s" % (
            group, min(todo), max(todo), len(rows), len(suspects),
            sum(r["audio"] == "DEAD" for r in rows), time.time() - t0))
        return rows


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True)
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--seconds", type=float, default=2.0)
    ap.add_argument("--chunk", type=int, default=40)
    ap.add_argument("--only")
    args = ap.parse_args()
    args.out = os.path.abspath(args.out)
    os.makedirs(args.out, exist_ok=True)
    layout = json.load(open(os.path.join(PROJECT, "build", "layout.json")))
    args.signal = rq.make_test_signal(args.out, args.seconds)
    gs, ids = groups(layout)
    only = set(args.only.split(",")) if args.only else None
    jobs = queue.Queue()
    planned = []
    for name, todo, ctx, slot in gs:
        if only and name not in only:
            continue
        span = (slot["base"], slot["base"] + slot["count"]) if slot else (min(todo), max(todo) + 1)
        main_params = [i for i in todo if not slot or i != slot["on"]]
        for k in range(0, len(main_params), args.chunk):
            planned.append((name, main_params[k:k + args.chunk], ctx, span, ids))
        if slot:
            # The on switch itself: off against on, with the module's defaults.
            planned.append((name, [slot["on"]], {k: v for k, v in ctx.items() if k != slot["on"]}, span, ids))
    # Biggest modules first, so the workers finish together.
    planned.sort(key=lambda j: -(j[3][1] - j[3][0]))
    for job in planned:
        jobs.put(job)
    results, holder = [], {}
    t0 = time.time()
    workers = [Worker(n, args, jobs, results, holder) for n in range(args.workers)]
    for w in workers:
        w.start()
        time.sleep(3)  # stagger the launches: they share REAPER's startup loader
    for w in workers:
        w.join()
    results.sort(key=lambda r: r["index"])
    report = {"fx": FX, "seconds": args.seconds, "workers": args.workers, "minutes": round((time.time() - t0) / 60, 1),
              "hostParams": holder.get("n"), "params": results}
    json.dump(report, open(os.path.join(args.out, "module-sweep.json"), "w"), indent=1)
    counts = {}
    for r in results:
        counts.setdefault(r["group"], {}).setdefault(r["audio"].split(" (")[0], 0)
        counts[r["group"]][r["audio"].split(" (")[0]] += 1
    lines = ["# OpenLimiter host sweep, one module at a time", "",
             "%s, %d workers, %.0f s renders, %.1f min, %s host parameters" % (
                 FX, args.workers, args.seconds, report["minutes"], holder.get("n")), "",
             "| Module | Result counts |", "|---|---|"]
    lines += ["| %s | %s |" % (g, ", ".join("%s %d" % kv for kv in sorted(c.items()))) for g, c in counts.items()]
    lines += ["", "## Not ok", "", "| # | Parameter | Module | Result | Detail |", "|---|---|---|---|---|"]
    lines += ["| %d | %s | %s | %s | %s |" % (r["index"], r.get("id", ""), r["group"], r["audio"],
                                             r.get("reason", "") or ("state %s, %s dB" % (r["state"], r["diffDb"]) if "state" in r else ""))
              for r in results if r["audio"] not in ("ok",)]
    open(os.path.join(args.out, "module-sweep.md"), "w").write("\n".join(lines) + "\n")
    print("\n".join(lines[:8 + len(counts)]))
    tested = {r["index"] for r in results}
    missing = [i for job in planned for i in job[1] if i not in tested]
    if missing:
        print("not tested (worker failure): %d parameters" % len(missing))
    return 1 if any(r["audio"].startswith(("DEAD", "SKIPPED", "SILENT")) or "nonfinite" in r for r in results) else 0


if __name__ == "__main__":
    sys.exit(main())
