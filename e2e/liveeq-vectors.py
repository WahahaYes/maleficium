#!/usr/bin/env python3
"""liveeq-vectors.py — run the formula language's shared cases through l3fp.

embed-runtime/src/expr.vectors.json holds the cases the reader's interpreter
(expr.ts, under vitest) must get right. This runs the same cases through the
package's TeX path (\\mfw_leq_check:nn and \\mfw_leq_eval:nnn in
maleficium-interactive.sty) in one compile, so both evaluators are held to one
answer: every value must show the expected text (or be undefined), and every
reject marked tex=true must be refused.

The cases go to TeX as a file read line by line as strings, so % and \\ reach
the check as written. The engine runs directly (not over the MCP sidecar)
because the results are read from the TeX log. Its cache persists across runs
(LIVEEQ_CACHE, default /var/tmp/maleficium-liveeq-cache-<uid>), so only the
first run downloads TeX support files. Needs network on a cold cache.

Usage:
  python3 e2e/liveeq-vectors.py [--engine PATH]
Without --engine it uses src-tauri/binaries/maleficium-engine-<host triple>.
"""
import argparse, glob, json, os, re, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
VECTORS = os.path.join(ROOT, "embed-runtime", "src", "expr.vectors.json")
STY = os.path.join(ROOT, "embed-runtime", "tex", "maleficium-interactive.sty")
ENGINE_RS = os.path.join(ROOT, "src-tauri", "core", "src", "engine.rs")

DRIVER = r"""\documentclass{article}
\usepackage{maleficium-interactive}
\ExplSyntaxOn
\ior_new:N \l_cases_ior
\ior_open:Nn \l_cases_ior { cases.txt }
\int_zero:N \l_tmpa_int
\ior_str_map_inline:Nn \l_cases_ior {
  \int_incr:N \l_tmpa_int
  \seq_set_split:Nnn \l_tmpb_seq { ; } {#1}
  \exp_args:Nx \str_if_eq:nnTF { \seq_item:Nn \l_tmpb_seq { 1 } } { V }
    {
      \exp_args:Nxx \mfw_leq_check:nn { \seq_item:Nn \l_tmpb_seq { 2 } } { \seq_item:Nn \l_tmpb_seq { 3 } }
      \str_if_empty:NTF \l_mfw_leq_err_str
        {
          \use:x { \exp_not:N \mfw_leq_eval:nnn
            { \seq_item:Nn \l_tmpb_seq { 2 } } { \seq_item:Nn \l_tmpb_seq { 4 } } { \seq_item:Nn \l_tmpb_seq { 5 } } }
          \iow_term:x { LEQ | \int_use:N \l_tmpa_int | OUT | \l_mfw_leq_out_tl | }
        }
        { \iow_term:x { LEQ | \int_use:N \l_tmpa_int | ERR | \l_mfw_leq_err_str | } }
    }
    {
      \exp_args:Nxx \mfw_leq_check:nn { \seq_item:Nn \l_tmpb_seq { 2 } } { \seq_item:Nn \l_tmpb_seq { 3 } }
      \iow_term:x { LEQ | \int_use:N \l_tmpa_int | ERR | \l_mfw_leq_err_str | }
    }
}
\ExplSyntaxOff
\begin{document}
cases done
\end{document}
"""


def say(msg):
    print("liveeq: " + msg, flush=True)


def host_engine():
    found = sorted(glob.glob(os.path.join(ROOT, "src-tauri", "binaries", "maleficium-engine-*")))
    if not found:
        sys.exit("liveeq: no engine in src-tauri/binaries (sh scripts/build-engine.sh), or pass --engine")
    return found[0]


def bundle_url():
    m = re.search(r'pub const BUNDLE_URL: &str = "([^"]+)"', open(ENGINE_RS).read())
    return m.group(1)


def case_lines(v):
    """One line per case: V;expr;names;k=v,...;places or R;expr;names."""
    lines, cases = [], []
    for c in v["values"]:
        names = ",".join(c["vars"])
        assigns = ",".join("%s=%r" % (k, x) for k, x in c["vars"].items())
        lines.append("V;%s;%s;%s;%d" % (c["expr"], names, assigns, c["places"]))
        cases.append(("value", c))
    for c in v["rejects"]:
        if c["tex"]:
            lines.append("R;%s;%s" % (c["expr"], ",".join(c["names"])))
            cases.append(("reject", c))
    for line in lines:
        assert "\n" not in line and line.count(";") <= 4, line
    return lines, cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine")
    ap.add_argument("--keep", action="store_true", help="keep the scratch dir")
    a = ap.parse_args()
    engine = a.engine or host_engine()
    lines, cases = case_lines(json.load(open(VECTORS)))
    work = tempfile.mkdtemp(prefix="maleficium-liveeq-")
    cache = os.environ.get("LIVEEQ_CACHE", "/var/tmp/maleficium-liveeq-cache-%d" % os.getuid())
    os.makedirs(cache, exist_ok=True)
    try:
        shutil.copy(STY, work)
        open(os.path.join(work, "cases.txt"), "w").write("\n".join(lines) + "\n")
        open(os.path.join(work, "main.tex"), "w").write(DRIVER)
        r = subprocess.run([engine, "compile", "main.tex", "--outdir", ".", "--keep-logs", "-b", bundle_url()],
                           cwd=work, env=dict(os.environ, TECTONIC_CACHE_DIR=cache),
                           capture_output=True, text=True, timeout=900)
        log = open(os.path.join(work, "main.log"), errors="replace").read() if os.path.exists(
            os.path.join(work, "main.log")) else ""
        if r.returncode != 0:
            say("compile failed (exit %d):\n%s\n%s" % (r.returncode, r.stderr[-2000:], "\n".join(
                l for l in log.splitlines() if l.startswith("!"))[:2000]))
            return 1
        got = {int(m.group(1)): (m.group(2), m.group(3))
               for m in re.finditer(r"^LEQ\|(\d+)\|(OUT|ERR)\|(.*)\|$", log, re.M)}
        bad = 0
        for i, (kind, c) in enumerate(cases, 1):
            g = got.get(i)
            if kind == "value":
                want = ("OUT", c["out"] or "")
                ok = g == want
            else:
                ok = g is not None and g[0] == "ERR" and g[1] != ""
            if not ok:
                bad += 1
                say("FAIL %s %r: want %s, got %s" % (kind, c["expr"],
                                                     want if kind == "value" else "a refusal", g))
        say("%d of %d cases agree with l3fp" % (len(cases) - bad, len(cases)))
        return 1 if bad else 0
    finally:
        if a.keep:
            say("kept " + work)
        else:
            shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
