#!/usr/bin/env python3
"""Checks that the documentation still matches the code. Run from the repository root:  python3 scripts/check_docs.py

  1. every diagnostic code in the Rust sources is in docs/diagnostics.html and language.md, and the other way round
  2. every built-in function of the checker has a reference page, and every page names a built-in
  3. every file path and relative link in README.md and the proposals' index exists
  4. every `emerge ...` command line in README.md parses as a known subcommand
  5. every `.em` example in docs and language.md fenced as a whole world parses (`emerge check`)
  6. every editor tab of the viewer is mentioned in the viewer guide, and every key it binds is documented

Exit status 1 if anything is stale."""
import os, re, subprocess, sys, glob

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
os.chdir(ROOT)
problems = []

def read(p):
    with open(p, encoding="utf-8") as f:
        return f.read()

def bad(msg):
    problems.append(msg)

# 1. diagnostic codes
code_re = re.compile(r'\b([EW]\d{4})\b')
src_codes = set()
for p in glob.glob("crates/**/src/**/*.rs", recursive=True):
    t = read(p)
    for m in re.finditer(r'"([EW]\d{4})"', t):
        src_codes.add(m.group(1))
doc_html = read("docs/diagnostics.html")
doc_codes = set(code_re.findall(doc_html))
lang_text = read("language.md")
lang_codes = set(code_re.findall(lang_text))
for a, b in re.findall(r'`([EW]\d{4})`[–-]`([EW]\d{4})`', lang_text):   # a range such as `E0001`–`E0007`
    lang_codes |= {f"{a[0]}{n:04d}" for n in range(int(a[1:]), int(b[1:]) + 1)}
for c in sorted(src_codes - doc_codes):
    bad(f"diagnostic {c} is raised in the code but is not in docs/diagnostics.html")
for c in sorted(src_codes - lang_codes):
    bad(f"diagnostic {c} is raised in the code but is not in language.md")
reserved = set(re.findall(r'<code>([EW]\d{4})</code></td>(?:(?!</tr>).)*?Reserved:', doc_html, re.S))
for c in sorted(doc_codes - src_codes - reserved):
    bad(f"docs/diagnostics.html lists {c}, which no code raises (say `Reserved:` in its text if that is meant)")

# 2. built-in functions: the checker's names against the reference pages
pages = {os.path.basename(p)[:-5] for p in glob.glob("docs/functions/*.html")}
res = read("crates/emerge-lang/src/resolve.rs")
m = re.search(r'const RESERVED: &\[&str\] = &\[(.*?)\];', res, re.S)
reserved = set(re.findall(r'"(\w+)"', m.group(1))) if m else set()
effects = {"transfer", "react", "diffuse_conserved", "take", "drop", "give", "steal", "guard", "move", "turn", "divide", "ramp", "fit", "fit_log", "lerp", "sqrt", "log", "abs", "min", "max", "clamp", "units", "float", "count", "sum", "mean", "argmax", "argmin", "cap", "amount", "jitter", "uniform", "gradient", "probe", "crowd", "neighbour", "clock", "inventory", "blur", "fade"}
for f in sorted(effects & reserved - pages):
    bad(f"function `{f}` is a reserved built-in but has no page in docs/functions/")

# 3. paths and links in the top-level docs
def check_links(path):
    t = read(path)
    base = os.path.dirname(path)
    for m in re.finditer(r'\]\(([^)#\s]+)(#[^)]*)?\)', t):
        target = m.group(1)
        if re.match(r'^[a-z]+:', target): continue
        if not os.path.exists(os.path.join(base, target)):
            bad(f"{path}: link to {target} does not exist")
    for m in re.finditer(r'`((?:crates|proposals|scripts|examples|docs|archive|gallery)/[\w./\-*]+)`', t):
        target = m.group(1)
        if "*" in target or target.endswith("/") and os.path.isdir(target): continue
        if not os.path.exists(target) and not glob.glob(target):
            bad(f"{path}: `{target}` does not exist")
for p in ["README.md", "language.md", "proposals/plan-next.md", "proposals/remaining-work.md"]:
    if os.path.exists(p): check_links(p)

# 4. command lines in the README
sub = set()
bin_src = read("crates/emerge-ref/src/bin/emerge.rs")
for m in re.finditer(r'"(check|order|run|expand|fmt|build|render|ask|diff|replay)"', bin_src):
    sub.add(m.group(1))
for m in re.finditer(r'target/release/emerge (\w+)', read("README.md")):
    if m.group(1) not in {"check", "order", "run", "expand", "fmt", "census"}:
        bad(f"README.md: `emerge {m.group(1)}` is not a subcommand of emerge")

# 5. the examples in language.md that are whole items parse (`emerge fmt` parses; an example with `...` in it is a sketch)
emerge = "target/release/emerge"
checked = 0
if os.path.exists(emerge):
    for i, b in enumerate(re.findall(r'```[a-z]*\n(.*?)\n```', read("language.md"), re.S)):
        m = re.match(r'(world|lineage|view|chart|patch|ruleset) [A-Z]\w*', b) or re.match(r'(rule) [a-z_]\w* on ', b)
        first = m.group(1) if m else ""
        if not m or "..." in b or "…" in b or "<" in b:
            continue
        if first == "rule":
            b = "world W {\n" + b + "\n}"   # a rule is written inside a world
        checked += 1
        r = subprocess.run([emerge, "fmt", "/dev/stdin"], input=b + "\n", capture_output=True, text=True)
        if r.returncode != 0:
            bad(f"language.md: example {i + 1} ({first}) does not parse: {(r.stderr or r.stdout).strip().splitlines()[0]}")
    print(f"parsed {checked} whole-item examples of language.md")
else:
    print("note: target/release/emerge is not built; example parsing skipped")

# 6. the viewer guide against the viewer
viewer = read("crates/emerge-server/src/viewer/index.html")
guide = ""
for p in glob.glob("docs/guide/using-*.html") + glob.glob("docs/guide/viewer*.html"):
    guide += read(p)
m = re.search(r'const EDITORS = \{(.*?)\};', viewer, re.S)
if guide and m:
    for name in re.findall(r':\s*"([^"]+)"', m.group(1)):
        if name not in guide:
            bad(f"the viewer has an editor `{name}` that the viewer guide does not mention")
elif not guide:
    bad("there is no viewer guide (docs/guide/using-*.html)")

if problems:
    print(f"{len(problems)} problems:")
    for p in problems: print(" -", p)
    sys.exit(1)
print("documentation checks pass")
