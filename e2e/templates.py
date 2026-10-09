"""templates.py: a built-in template as the app instantiates it, for scripts
that build template projects outside the app.

The app embeds src-tauri/templates/<id>/ plus the shared files that
src-tauri/templates/shared/overlay.txt names for it (core/build.rs reads the
same file), minus template.json, the template's own metadata. Copying the
folder alone misses the shared styles, and the project does not compile.

    python3 e2e/templates.py <id> <dest>    # dest must not exist
"""
import os
import shutil
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TEMPLATES = os.path.join(ROOT, "src-tauri", "templates")
SHARED = os.path.join(TEMPLATES, "shared")


def overlay():
    """[(path relative to shared/, set of template ids or None for all)]."""
    out = []
    with open(os.path.join(SHARED, "overlay.txt")) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            path, ids = (p.strip() for p in line.split(":", 1))
            out.append((path, None if ids == "*" else set(ids.split())))
    return out


def materialize(template_id, dest):
    """Write template `template_id` to `dest` (which must not exist) as a
    new project from it holds it."""
    src = os.path.join(TEMPLATES, template_id)
    if template_id == "shared" or not os.path.isfile(os.path.join(src, "template.json")):
        raise ValueError("not a template: %s" % template_id)
    shutil.copytree(src, dest)
    os.remove(os.path.join(dest, "template.json"))
    for path, ids in overlay():
        if ids is None or template_id in ids:
            shutil.copy(os.path.join(SHARED, path), os.path.join(dest, os.path.basename(path)))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    materialize(sys.argv[1], sys.argv[2])
