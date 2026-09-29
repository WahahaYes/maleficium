#!/usr/bin/env python3
"""papers-run.py — compile every vendored real paper and score the result.

Each folder in e2e/fixtures/vendored/ with a fixture.json is one paper:

    {"main": "paper.tex", "expect": "fail", "why": "minted needs shell escape"}

A paper passes when the compile succeeds and diagnostics report no errors
(warnings are counted, not judged). `expect` records what happens today, so
the run is a ratchet:
  - an expected pass that fails is a regression: the run fails;
  - an expected fail that passes is progress: the run also fails, so the
    expectation gets flipped to "pass" and the gain is locked in.

Each paper is copied to a scratch dir and compiled over the MCP sidecar, the
same path an agent takes. The engine cache persists across runs (PAPERS_CACHE,
default /var/tmp/maleficium-papers-cache-<uid>), so only the first run
downloads TeX support files. Needs network on a cold cache.

Usage:
  python3 e2e/papers-run.py [--bin PATH] [--paper NAME ...] [--json OUT]
Without --bin it builds and uses the maleficium-mcp sidecar.
"""
import argparse, json, os, shutil, subprocess, sys, tempfile, time

from mcp_client import McpClient

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
VENDORED = os.path.join(HERE, "fixtures", "vendored")
COMPILE_TIMEOUT = 900


def say(msg):
    print("papers: " + msg, flush=True)


def build_sidecar():
    target = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "src-tauri", "target"))
    env = dict(os.environ, PATH=os.path.expanduser("~/.cargo/bin") + os.pathsep + os.environ.get("PATH", ""))
    subprocess.run(["cargo", "build", "-q", "--manifest-path", os.path.join(ROOT, "src-tauri", "Cargo.toml"),
                    "--bin", "maleficium-mcp"], check=True, env=env)
    return os.path.join(target, "debug", "maleficium-mcp" + (".exe" if os.name == "nt" else ""))


def load_papers(names):
    papers = []
    for name in sorted(os.listdir(VENDORED)):
        spec = os.path.join(VENDORED, name, "fixture.json")
        if not os.path.isfile(spec):
            continue
        with open(spec) as f:
            p = json.load(f)
        if p.get("expect") not in ("pass", "fail"):
            sys.exit("papers: %s: expect must be \"pass\" or \"fail\"" % spec)
        papers.append(dict(p, name=name))
    if names:
        missing = set(names) - {p["name"] for p in papers}
        if missing:
            sys.exit("papers: no such paper: %s" % ", ".join(sorted(missing)))
        papers = [p for p in papers if p["name"] in names]
    return papers


def blocker(rec, diags):
    """The first thing standing between this paper and a clean build."""
    missing = rec.get("missing") or diags.get("missing")
    if missing:
        return "missing %s (%s)" % (missing.get("file"), missing.get("reason"))
    for d in diags.get("diagnostics", []):
        if d.get("severity") == "error":
            where = "%s:%s: " % (d.get("file"), d.get("line")) if d.get("file") else ""
            return (where + (d.get("message") or "")).strip()[:160]
    if rec.get("status") != "success":
        for line in rec.get("lines") or []:
            if line.startswith("error:"):
                return line[:160]
        return "compile " + str(rec.get("status"))
    return ""


def run_paper(mcp, paper, scratch):
    proj = os.path.join(scratch, paper["name"])
    shutil.copytree(os.path.join(VENDORED, paper["name"]), proj)
    ok, g = mcp.tool("grant", {"root_id": paper["name"], "root": proj})
    if not ok:
        return {"status": "grant-failed", "blocker": g}
    ok, pre = mcp.tool("precompile_checks", {"root_id": paper["name"], "main_rel": paper["main"]})
    findings = pre.get("findings", []) if ok else []
    t0 = time.time()
    ok, job = mcp.tool("compile_run", {"root_id": paper["name"], "rel": paper["main"]})
    if not ok:
        return {"status": "refused", "blocker": job}
    rec, deadline = {"status": "running"}, t0 + COMPILE_TIMEOUT
    while rec.get("status") == "running" and time.time() < deadline:
        ok, rec = mcp.tool("compile_poll", {"job_id": job["job_id"], "tail_lines": 40, "wait_ms": 30000})
        if not ok:
            return {"status": "poll-failed", "blocker": rec}
    if rec.get("status") == "running":
        mcp.tool("compile_cancel", {"job_id": job["job_id"]})
        rec["status"] = "timeout"
    ok, diags = mcp.tool("diagnostics", {"root_id": paper["name"], "main_rel": paper["main"]})
    diags = diags if ok else {}
    found = diags.get("diagnostics", [])
    sev = [d.get("severity") for d in found]
    return {
        "status": rec.get("status"),
        "seconds": round(time.time() - t0, 1),
        "errors": sev.count("error"),
        "warnings": sev.count("warning"),
        "precheck": ["%s %s" % (f.get("kind"), f.get("name")) for f in findings],
        "diagnostics": [{k: d.get(k) for k in ("severity", "path", "line", "message")} for d in found],
        "truncated": diags.get("truncated", 0),
        "blocker": blocker(rec, diags),
    }


def print_diagnostics(r):
    """Each distinct diagnostic once, in the order the app reports them."""
    counts = {}
    for d in r.get("diagnostics", []):
        where = "%s:%s" % (d["path"], d["line"]) if d.get("path") else "-"
        key = (d["severity"], where, d["message"] or "(empty)")
        counts[key] = counts.get(key, 0) + 1
    for (sev, where, msg), n in counts.items():
        print("%-28s %-7s %-14s %s%s" % ("", sev, where, msg[:100], " (%dx)" % n if n > 1 else ""))
    if r.get("truncated"):
        print("%-28s the app truncated %s more" % ("", r["truncated"]))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--bin", help="MCP server to drive (default: build maleficium-mcp)")
    ap.add_argument("--paper", action="append", default=[], help="only this paper (repeatable)")
    ap.add_argument("--json", help="also write the results, every diagnostic included, here")
    ap.add_argument("--brief", action="store_true", help="scoreboard only, without each paper's diagnostics")
    a = ap.parse_args()

    papers = load_papers(a.paper)
    if not papers:
        sys.exit("papers: no fixture.json under %s" % VENDORED)
    argv = [os.path.abspath(a.bin)] if a.bin else [build_sidecar()]
    if a.bin and os.path.basename(a.bin).startswith("maleficium") and not a.bin.endswith("-mcp"):
        argv.append("--mcp")
    uid = os.getuid() if hasattr(os, "getuid") else 0
    cache = os.environ.get("PAPERS_CACHE", "/var/tmp/maleficium-papers-cache-%d" % uid)
    os.makedirs(cache, exist_ok=True)
    scratch = tempfile.mkdtemp(prefix="maleficium-papers-")
    env = dict(os.environ, XDG_CACHE_HOME=cache, XDG_DATA_HOME=os.path.join(scratch, ".data"))
    say("%d papers, engine cache %s" % (len(papers), cache))

    mcp = McpClient(argv, "papers-run", env=env)
    results = []
    try:
        for p in papers:
            say("compiling %s (%s)" % (p["name"], p["main"]))
            r = run_paper(mcp, p, scratch)
            r.update(name=p["name"], main=p["main"], expect=p["expect"])
            r["passed"] = r["status"] == "success" and r.get("errors", 0) == 0
            results.append(r)
    finally:
        mcp.close(timeout=30)

    regressions = [r for r in results if r["expect"] == "pass" and not r["passed"]]
    gains = [r for r in results if r["expect"] == "fail" and r["passed"]]
    print("")
    print("%-28s %-6s %-9s %6s %8s  %s" % ("paper", "expect", "result", "errors", "warnings", "first blocker"))
    for r in results:
        print("%-28s %-6s %-9s %6s %8s  %s" % (r["name"], r["expect"], "pass" if r["passed"] else r["status"],
                                               r.get("errors", "-"), r.get("warnings", "-"), r.get("blocker") or ""))
        for f in r.get("precheck", []):
            print("%-28s precheck: %s" % ("", f))
        if not a.brief:
            print_diagnostics(r)
    print("")
    say("%d of %d papers compile error-free" % (sum(r["passed"] for r in results), len(results)))
    if a.json:
        with open(a.json, "w") as f:
            json.dump(results, f, indent=2)
    for r in regressions:
        say("REGRESSION: %s was expected to pass" % r["name"])
    for r in gains:
        say("NOW PASSES: %s; set \"expect\": \"pass\" in its fixture.json" % r["name"])
    shutil.rmtree(scratch, ignore_errors=True)
    sys.exit(1 if regressions or gains else 0)


if __name__ == "__main__":
    main()
