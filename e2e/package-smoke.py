"""package-smoke.py — install this host's release packages and prove they work.

The one smoke path for every OS. Only installing is per OS:
  Linux:   the .deb into a clean Ubuntu container (so its Depends are what
           make it run, not a build host's -dev packages); payload checks
           (binaries, desktop entry, dumps resource); the AppImage carries
           no libwayland and ships the dumps resource
  macOS:   mount the .dmg, copy the .app out, verify its signature, check
           every binary is built for this machine, and that every Mach-O in
           the bundle links only system libraries or ones inside the bundle;
           the probes below then run with Homebrew and the developer tools
           moved out of sight, as on a Mac that never had them
  Windows: silent NSIS install; check the installed binaries, that every exe
           and dll imports only system DLLs or ones in the install (no Visual
           C++ runtime), and run the probes with only System32 on PATH
Then the same checks everywhere:
  1. the installed MCP server, as maleficium-mcp and as the app's --mcp,
     names itself, compiles a page with its bundled engine, and answers a
     SyncTeX lookup (sidecar lookup, the engine, app dirs, and native paths
     all work)
  2. the app's first launch opens the welcome project, read from its own
     event log: the webview ran the UI, IPC reached Rust, project files
     resolved through the fs scope; any error event fails
  3. the app is still running afterwards (plus the AppImage on Linux)
What it does NOT cover: how anything looks or behaves under input.

Usage: python3 e2e/package-smoke.py <artifact-dir> [--image IMAGE | --in-container]
Needs: python3 3.9+; Linux: docker. A first launch needs no recent
projects, so macOS and Windows expect a fresh user (CI runners are).
"""
import argparse, contextlib, glob, json, os, platform, plistlib, shutil, signal, subprocess, sys, tempfile, time

from mcp_client import McpClient

IDENT = "io.github.wahahayes.maleficium"
REQUIRED = ["log.open", "template.welcome", "project.open", "index.open", "file.open"]
EVENTS_WAIT = 60  # seconds for the first launch to log REQUIRED
ALIVE = 20  # seconds a launch must survive
HERE = os.path.dirname(os.path.abspath(__file__))


def say(msg):
    print("smoke: " + msg, flush=True)


def fail(msg):
    sys.exit("smoke: FAIL " + msg)


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def one(dir, pattern):
    found = glob.glob(os.path.join(dir, pattern))
    if len(found) != 1:
        fail("need exactly one %s in %s, found %s" % (pattern, dir, found))
    return found[0]


# ---- shared checks ---------------------------------------------------------


def compile_probe(cmd, scratch, env=None):
    """initialize -> grant -> compile_run -> compile_poll -> synctex_forward over
    the stdio of an installed MCP server, started as `cmd` (a list)."""
    os.makedirs(scratch, exist_ok=True)
    with open(os.path.join(scratch, "main.tex"), "w") as f:
        f.write("\\documentclass{article}\n\\begin{document}\nSmoke.\n\\end{document}\n")
    mcp = McpClient(cmd, "smoke", env=env)

    def call(name, args):
        ok, r = mcp.tool(name, args)
        if not ok:
            fail("%s: %s" % (name, r))
        return r

    who = mcp.server_info
    if who["name"] != "maleficium":
        fail("%s: serverInfo is %s" % (" ".join(cmd), who))
    call("grant", {"root_id": "smoke", "root": scratch})
    job = call("compile_run", {"root_id": "smoke", "rel": "main.tex"})["job_id"]
    deadline = time.time() + 300
    rec = {"status": "running"}
    while rec["status"] == "running" and time.time() < deadline:
        rec = call("compile_poll", {"job_id": job, "tail_lines": 5, "wait_ms": 30000})
    if rec["status"] != "success":
        fail("compile %s: %s" % (rec["status"], str(rec)[:300]))
    hit = call("synctex_forward", {"root_id": "smoke", "main_rel": "main.tex", "tex_rel": "main.tex", "line": 3})
    mcp.close()
    if hit.get("page") != 1:
        fail("synctex_forward: %s" % hit)
    say("%s: %s %s, compile + synctex ok" % (" ".join([os.path.basename(cmd[0])] + cmd[1:]), who["name"], who["version"]))


def read_events(path):
    try:
        with open(path, encoding="utf-8") as f:
            lines = [l for l in f.read().splitlines() if l.strip()]
    except FileNotFoundError:
        return []
    out = []
    for l in lines:
        try:
            out.append(json.loads(l))
        except ValueError:
            pass  # a line mid-write; the next poll sees it whole
    return out


def wait_for_welcome(path):
    """None when the first launch logged REQUIRED and no error, else why not."""
    deadline = time.time() + EVENTS_WAIT
    while True:
        events = read_events(path)
        actions = [e["event"].get("action") for e in events if isinstance(e.get("event"), dict)]
        errors = [e.get("message", "")[:200] for e in events if e.get("kind") == "error"]
        missing = [a for a in REQUIRED if a not in actions]
        if errors or not missing or time.time() > deadline:
            break
        time.sleep(1)
    for e in events:
        if e.get("kind") == "warn":
            say("app warn: %s" % e.get("message", "")[:200])
    if not events:
        return "no event log at %s (the UI never started recording)" % path
    if errors:
        return "error events: %s" % "; ".join(errors)
    if missing:
        return "event log lacks %s (has %s)" % (missing, sorted(set(actions)))
    say("launch events ok: %d events, %s" % (len(events), ",".join(REQUIRED)))
    return None


def stop(p):
    if p.poll() is not None:
        return
    if os.name == "posix":
        os.killpg(p.pid, signal.SIGKILL)
    else:
        p.kill()
    p.wait()


def launch(name, cmd, events=None, env=None):
    """Start the app; check its first-launch events (if given), still alive."""
    log = tempfile.TemporaryFile()
    p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT, env=env, start_new_session=os.name == "posix")
    try:
        why = wait_for_welcome(events) if events else None
        time.sleep(5 if events else ALIVE)
        alive = p.poll() is None
    finally:
        stop(p)
    if why or not alive:
        log.seek(0)
        print(log.read().decode(errors="replace")[-3000:])
        fail("%s: %s" % (name, why or "exited early (code %s)" % p.returncode))
    say("%s launch ok" % name)


# ---- per OS ----------------------------------------------------------------


def linux_in_container(dir, image):
    """Re-run this script inside a clean `image` against the same artifacts."""
    run(["docker", "run", "--rm", "-v", "%s:/pkg:ro" % os.path.abspath(dir), "-v", "%s:/e2e:ro" % HERE, image,
         "bash", "-c", "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq python3 >/dev/null"
         " && python3 /e2e/package-smoke.py /pkg --in-container"])


LINUX_SCREEN = "1280x800x24"  # xvfb-run's default is 640x480x8


def linux(dir):
    deb = one(dir, "Maleficium_*_amd64.deb")
    img = deb[: -len(".deb")] + ".AppImage"
    if not os.path.isfile(img):
        fail("missing %s" % img)
    fields = run(["dpkg-deb", "-f", deb, "Package", "Version", "Depends"], capture_output=True, text=True).stdout
    say("control: " + " ".join(fields.split()))
    run(["apt-get", "install", "-y", "-qq", os.path.abspath(deb), "xvfb", "matchbox-window-manager"], stdout=subprocess.DEVNULL,
        env=dict(os.environ, DEBIAN_FRONTEND="noninteractive"))
    files = run(["dpkg", "-L", "maleficium"], capture_output=True, text=True).stdout.splitlines()
    for want in ["/usr/bin/maleficium", "/usr/bin/maleficium-mcp", "/usr/bin/maleficium-engine"]:
        if want not in files:
            fail("package lacks %s" % want)
    if not any(f.endswith("Maleficium.desktop") for f in files):
        fail("package lacks its desktop entry")
    dumps = [f for f in files if f.startswith("/usr/lib/Maleficium/resources/dumps/")]
    if not any(os.path.basename(f).startswith("latex.") and f.endswith(".dump.txt") for f in dumps):
        fail("package lacks the latexml format dumps under /usr/lib/Maleficium/resources/dumps")
    say("install ok (%d dumps resource files)" % len(dumps))
    with tempfile.TemporaryDirectory() as t:
        run([os.path.abspath(img), "--appimage-extract"], cwd=t, stdout=subprocess.DEVNULL)
        bundled = glob.glob(os.path.join(t, "squashfs-root/usr/lib/libwayland-*"))
        if bundled:
            fail("AppImage bundles %s (clashes with newer host Mesa)" % os.path.basename(bundled[0]))
        dumps = glob.glob(os.path.join(t, "squashfs-root/usr/lib/Maleficium/resources/dumps/latex.*.dump.txt"))
        if not dumps:
            fail("AppImage lacks the latexml format dumps (usr/lib/Maleficium/resources/dumps)")
    say("AppImage bundles no libwayland, dumps resource ok")
    home = tempfile.mkdtemp(prefix="smoke-home-")
    env = {k: v for k, v in os.environ.items() if not k.startswith("XDG_")}
    env["HOME"] = home
    compile_probe(["/usr/bin/maleficium-mcp"], os.path.join(home, "doc"), env)
    compile_probe(["/usr/bin/maleficium", "--mcp"], os.path.join(home, "doc-app"), env)
    compile_probe([os.path.abspath(img), "--mcp"], os.path.join(home, "doc-appimage"),
                  dict(env, APPIMAGE_EXTRACT_AND_RUN="1"))
    events = os.path.join(home, ".local/share", IDENT, "maleficium-log/events.jsonl")
    # matchbox maximizes the window to the whole screen, as a desktop session would
    session = "matchbox-window-manager -use_titlebar no & exec maleficium"
    launch("deb", ["xvfb-run", "-a", "-s", "-screen 0 " + LINUX_SCREEN, "sh", "-c", session],
           events=events, env=env)
    launch("AppImage", ["xvfb-run", "-a", os.path.abspath(img)], env=dict(env, APPIMAGE_EXTRACT_AND_RUN="1"))


MACHO_MAGIC = {b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe", b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"}


def macho_files(app):
    """Every Mach-O file (executable, dylib, bundle) under the .app, by magic number."""
    for root, _, files in os.walk(app):
        for f in files:
            path = os.path.join(root, f)
            if os.path.islink(path) or not os.path.isfile(path):
                continue
            with open(path, "rb") as fh:
                if fh.read(4) in MACHO_MAGIC:
                    yield path


def macho_offenders(app, path):
    """The dependencies of one Mach-O that a clean Mac would not have: anything but
    /usr/lib, /System/Library, or an @executable_path/@loader_path/@rpath path that
    lands inside the bundle (Homebrew's /opt/homebrew, /usr/local/... are absent)."""
    out = lambda *a: run(["otool", *a, path], capture_output=True, text=True).stdout
    own = set(out("-D").splitlines()[1:])  # a dylib's own install name is not a dependency
    rpaths = [l.split()[1] for l in out("-l").splitlines() if l.strip().startswith("path ") and "(offset" in l]
    exe_dir = os.path.join(app, "Contents/MacOS")
    here = os.path.dirname(path)
    inside = lambda p: os.path.exists(p) and os.path.realpath(p).startswith(os.path.realpath(app) + os.sep)
    bad = []
    for line in out("-L").splitlines()[1:]:
        dep = line.strip().split(" (compatibility")[0]
        if not dep or dep in own:
            continue
        if dep.startswith(("/usr/lib/", "/System/Library/")):
            continue
        base = {"@executable_path": exe_dir, "@loader_path": here}
        head, _, rest = dep.partition("/")
        if head in base:
            ok = inside(os.path.join(base[head], rest))
        elif head == "@rpath":
            ok = any(
                inside(os.path.join(base.get(r.partition("/")[0], ""), r.partition("/")[2], rest))
                for r in rpaths
                if r.partition("/")[0] in base
            )
        else:
            ok = False
        if not ok:
            bad.append(dep)
    return bad


def macos_dependencies(app):
    files = list(macho_files(app))
    offenders = {os.path.relpath(f, app): macho_offenders(app, f) for f in files}
    offenders = {f: d for f, d in offenders.items() if d}
    if offenders:
        for f, deps in sorted(offenders.items()):
            for d in deps:
                print("smoke:   %s links %s" % (f, d), flush=True)
        return "%d binaries link libraries a clean Mac lacks (listed above)" % len(offenders)
    say("dependencies ok (%d Mach-O files link only system or bundled libraries)" % len(files))
    return None


@contextlib.contextmanager
def macos_hidden_tooling():
    """Yield an env for the probes, with what a clean Mac lacks moved aside: Homebrew
    (/opt/homebrew on Apple silicon; its libraries under /usr/local on Intel) and the
    Xcode/Command Line Tools (DEVELOPER_DIR points at an empty directory). CI runners
    have all of it, so a binary that leans on any of it passes there and fails for
    users. The paths come back in the finally."""
    if platform.machine() == "arm64":
        paths = ["/opt/homebrew"]
    else:
        paths = ["/usr/local/" + d for d in ("lib", "opt", "Cellar")]
    moved = []
    try:
        for p in paths:
            if os.path.exists(p):
                run(["sudo", "mv", p, p + ".smoke-hidden"])
                moved.append(p)
        still = [p for p in paths if os.path.exists(p)]
        if still:
            fail("could not hide %s" % still)
        say("hid %s and the developer tools" % (", ".join(moved) or "nothing (none present)"))
        env = dict(os.environ, DEVELOPER_DIR=tempfile.mkdtemp(prefix="no-devtools-"))
        env["PATH"] = ":".join(d for d in env["PATH"].split(":") if not d.startswith(("/opt/homebrew", "/usr/local")))
        yield env
    finally:
        for p in moved:
            run(["sudo", "mv", p + ".smoke-hidden", p])


def macos(dir):
    dmg = one(dir, "Maleficium_*.dmg")
    t = tempfile.mkdtemp(prefix="smoke-")
    mnt = os.path.join(t, "dmg")
    run(["hdiutil", "attach", "-nobrowse", "-readonly", "-mountpoint", mnt, dmg], stdout=subprocess.DEVNULL)
    try:
        run(["cp", "-R", os.path.join(mnt, "Maleficium.app"), t])
    finally:
        run(["hdiutil", "detach", mnt], stdout=subprocess.DEVNULL)
    app = os.path.join(t, "Maleficium.app")
    run(["codesign", "--verify", "--deep", "--strict", app])
    say("signature ok")
    bin = os.path.join(app, "Contents/MacOS")
    arch = platform.machine()  # arm64 or x86_64, as lipo names them
    for b in ["maleficium-mcp", "maleficium-engine"]:
        path = os.path.join(bin, b)
        if not os.access(path, os.X_OK):
            fail("bundle lacks %s" % b)
        archs = run(["lipo", "-archs", path], capture_output=True, text=True).stdout.split()
        if arch not in archs:
            fail("%s is %s, not %s" % (b, archs, arch))
    say("bundle ok (%s)" % arch)
    deps = macos_dependencies(app)  # needs otool, so before the tools are hidden; fails below
    with open(os.path.join(app, "Contents/Info.plist"), "rb") as f:
        main = plistlib.load(f)["CFBundleExecutable"]
    events = os.path.expanduser("~/Library/Application Support/%s/maleficium-log/events.jsonl" % IDENT)
    with macos_hidden_tooling() as env:
        try:
            compile_probe([os.path.join(bin, "maleficium-mcp")], os.path.join(t, "doc"), env=env)
            compile_probe([os.path.join(bin, main), "--mcp"], os.path.join(t, "doc-app"), env=env)
            launch("app", [os.path.join(bin, main)], events=events, env=env)
        except SystemExit as e:
            if deps:
                say("dependencies: " + deps)  # the likely cause of the probe failure above
            raise e
    if deps:
        fail(deps)


# DLLs a fresh Windows 10+ has in System32, plus the API sets (api-ms-win-*, which
# includes the UCRT). This is exactly what the three exes import today (the smoke
# logs it); a new import fails here until someone checks it ships with Windows and
# adds it. Anything else must be a file in the install itself. The Visual C++
# runtime (msvcp140.dll, vcruntime140.dll, ...) is on CI runners but not on a fresh
# Windows, so it fails here (it would start with 0xC0000135).
WINDOWS_SYSTEM = {
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "combase.dll", "comctl32.dll", "crypt32.dll",
    "dwmapi.dll", "gdi32.dll", "kernel32.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll", "shell32.dll",
    "shlwapi.dll", "user32.dll", "userenv.dll", "ws2_32.dll",
}


def pe_imports(path):
    """The DLL names a PE file imports, lowercased, from its import table."""
    import struct

    with open(path, "rb") as f:
        data = f.read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe : pe + 4] != b"PE\0\0":
        fail("%s is not a PE file" % path)
    _, nsections, _, _, _, optsize = struct.unpack_from("<HHIIIH", data, pe + 4)
    opt = pe + 24
    magic = struct.unpack_from("<H", data, opt)[0]
    if magic not in (0x10B, 0x20B):  # PE32 (the NSIS uninstaller) or PE32+
        fail("%s is not a PE file" % path)
    dirs = 96 if magic == 0x10B else 112  # offset of the data directories
    import_rva = struct.unpack_from("<I", data, opt + dirs + 8)[0]  # data directory 1: imports
    sections = [struct.unpack_from("<IIII", data, opt + optsize + i * 40 + 8) for i in range(nsections)]

    def offset(rva):
        for vsize, vaddr, rawsize, rawptr in sections:
            if vaddr <= rva < vaddr + max(vsize, rawsize):
                return rawptr + rva - vaddr
        fail("%s: rva %#x is in no section" % (path, rva))

    names, at = set(), offset(import_rva)
    while True:
        entry = struct.unpack_from("<IIIII", data, at)
        if not any(entry):
            return names
        name = offset(entry[3])
        names.add(data[name : data.index(b"\0", name)].decode("ascii").lower())
        at += 20


def windows(dir):
    setup = one(dir, "Maleficium_*-setup.exe")
    dest = tempfile.mkdtemp(prefix="smoke-")
    # NSIS takes /D= last and unquoted, so pass a raw command line.
    run('"%s" /S /D=%s' % (setup, dest))
    for b in ["Maleficium.exe", "maleficium-mcp.exe", "maleficium-engine.exe"]:
        if not os.path.isfile(os.path.join(dest, b)):
            fail("install lacks %s" % b)
    shipped = {}
    for root, _, files in os.walk(dest):
        for f in files:
            if f.lower().endswith((".exe", ".dll")):
                shipped[os.path.join(root, f)] = None
    local = {os.path.basename(f).lower() for f in shipped}
    offenders, seen = {}, set()
    for f in shipped:
        imports = pe_imports(f)
        seen |= imports - local
        bad = sorted(
            d for d in imports
            if d not in WINDOWS_SYSTEM and d not in local and not d.startswith(("api-ms-win-", "ext-ms-win-"))
        )
        if bad:
            offenders[os.path.relpath(f, dest)] = bad
    for f, deps in sorted(offenders.items()):
        for d in deps:
            print("smoke:   %s links %s" % (f, d), flush=True)
    if offenders:
        fail("%d binaries import DLLs a fresh Windows lacks (listed above)" % len(offenders))
    say("install ok (%s), %d exe/dll files import only system or bundled DLLs" % (dest, len(shipped)))
    say("system DLLs imported: " + " ".join(sorted(d for d in seen if not d.startswith("api-ms-win-"))))
    # A fresh Windows has none of the runner's toolchains on PATH (vcpkg, MSYS, cargo, ...).
    root = os.environ.get("SystemRoot", r"C:\Windows")
    env = dict(os.environ, PATH=";".join([root + r"\System32", root, root + r"\System32\Wbem"]))
    compile_probe([os.path.join(dest, "maleficium-mcp.exe")], os.path.join(dest, "..", "smoke-doc"), env=env)
    compile_probe([os.path.join(dest, "Maleficium.exe"), "--mcp"], os.path.join(dest, "..", "smoke-doc-app"), env=env)
    events = os.path.join(os.environ["APPDATA"], IDENT, "maleficium-log", "events.jsonl")
    launch("app", [os.path.join(dest, "Maleficium.exe")], events=events, env=env)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("dir", help="directory holding this host's packages (scripts/package.sh --out)")
    ap.add_argument("--image", default="ubuntu:24.04", help="Linux: clean image to install into")
    ap.add_argument("--in-container", action="store_true", help=argparse.SUPPRESS)
    a = ap.parse_args()
    system = platform.system()
    if system == "Linux":
        linux(a.dir) if a.in_container else linux_in_container(a.dir, a.image)
    elif system == "Darwin":
        macos(a.dir)
    elif system == "Windows":
        windows(a.dir)
    else:
        fail("unsupported host: %s" % system)
    if not a.in_container:
        say("ALL GREEN")


if __name__ == "__main__":
    main()
