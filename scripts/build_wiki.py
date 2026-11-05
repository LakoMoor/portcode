#!/usr/bin/env python3
"""Builds the wiki in docs/wiki/ from wiki/*.md and a few docs/*.md files.

Usage: build_wiki.py [path-to-portcode-binary]
Generated blocks in the sources: {{slash}} (slash commands), {{tools}} (tools), {{cli}} (portcode --help).
Old docs/guide/*.html pages become redirects so existing links keep working.
"""
import html, json, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target/release/portcode")
OUT = os.path.join(ROOT, "docs/wiki")

# (group, slug, title, source)
NAV = [
    ("Start", "index", "Overview", "wiki/index.md"),
    ("Start", "install", "Install", "wiki/install.md"),
    ("Start", "first-run", "First run", "wiki/first-run.md"),
    ("Start", "tour", "A tour of the app", "wiki/tour.md"),
    ("Use", "agents", "Agents", "wiki/agents.md"),
    ("Use", "commands", "Slash commands", "wiki/commands.md"),
    ("Use", "keys", "Keyboard shortcuts", "wiki/keys.md"),
    ("Use", "sessions", "Sessions and history", "wiki/sessions.md"),
    ("Use", "permissions", "Permissions and allow-all", "wiki/permissions.md"),
    ("Use", "team", "Team mode", "wiki/team.md"),
    ("Use", "python", "Python environments", "wiki/python.md"),
    ("Use", "tools", "Tools", "wiki/tools.md"),
    ("Port and reverse", "port-workflow", "The port workflow", "wiki/port-workflow.md"),
    ("Port and reverse", "modes", "Clean-room and deep mode", "wiki/modes.md"),
    ("Port and reverse", "engines", "Engines", "docs/ENGINES.md"),
    ("Models", "providers", "Providers and models", "docs/PROVIDERS.md"),
    ("Models", "subscriptions", "Claude Code and Codex", "docs/SUBSCRIPTIONS.md"),
    ("Reference", "config", "Configuration", "wiki/config.md"),
    ("Reference", "cli", "Command line", "wiki/cli.md"),
    ("Reference", "remote", "Remote control", "docs/REMOTE.md"),
    ("Reference", "troubleshooting", "Troubleshooting", "wiki/troubleshooting.md"),
    ("Project", "architecture", "Architecture", "docs/ARCHITECTURE.md"),
    ("Project", "opencode", "OpenCode mapping", "docs/OPENCODE_MAPPING.md"),
    ("Project", "contributing", "Contributing", "wiki/contributing.md"),
    ("Project", "roadmap", "Roadmap", "docs/ROADMAP.md"),
]
GUIDE_REDIRECTS = {
    "GETTING_STARTED": "first-run", "SUBSCRIPTIONS": "subscriptions", "ARCHITECTURE": "architecture",
    "PROVIDERS": "providers", "OPENCODE_MAPPING": "opencode", "ENGINES": "engines", "REMOTE": "remote", "ROADMAP": "roadmap",
}
# Links written in the old docs/*.md files, mapped to wiki pages.
LINKMAP = {
    "SUBSCRIPTIONS.html": "subscriptions.html", "PROVIDERS.html": "providers.html", "ENGINES.html": "engines.html",
    "REMOTE.html": "remote.html", "GETTING_STARTED.html": "first-run.html", "ARCHITECTURE.html": "architecture.html",
    "OPENCODE_MAPPING.html": "opencode.html", "ROADMAP.html": "roadmap.html",
    "SUBSCRIPTIONS.md": "subscriptions.html", "PROVIDERS.md": "providers.html", "ENGINES.md": "engines.html",
    "REMOTE.md": "remote.html", "GETTING_STARTED.md": "first-run.html", "ARCHITECTURE.md": "architecture.html",
    "OPENCODE_MAPPING.md": "opencode.html", "ROADMAP.md": "roadmap.html",
}

e = html.escape


def slugify(t):
    s = re.sub(r"<[^>]+>", "", t).lower()
    s = re.sub(r"[^a-z0-9]+", "-", s).strip("-")
    return s or "section"


def inline(t):
    codes = []

    def keep(m):
        codes.append("<code>%s</code>" % e(m.group(1)))
        return "\x00%d\x00" % (len(codes) - 1)

    t = re.sub(r"`([^`]+)`", keep, t)
    t = e(t, quote=False)
    t = re.sub(r"!\[([^\]]*)\]\(([^)\s]+)\)", lambda m: '<img src="%s" alt="%s" loading="lazy">' % (fix_src(m.group(2)), m.group(1)), t)
    t = re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", lambda m: '<a href="%s">%s</a>' % (fix_link(m.group(2)), m.group(1)), t)
    t = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", t)
    t = re.sub(r"(?<![\w*])\*([^*\s][^*]*)\*(?![\w*])", r"<em>\1</em>", t)
    return re.sub(r"\x00(\d+)\x00", lambda m: codes[int(m.group(1))], t)


def fix_link(u):
    base = u.split("#")[0]
    frag = u[len(base):]
    name = os.path.basename(base)
    if name in LINKMAP and not base.startswith("http"):
        return LINKMAP[name] + frag
    return u


def fix_src(u):
    if u.startswith(("http", "/")):
        return u
    # docs/*.md sources use paths relative to docs/; wiki pages live in docs/wiki/
    return "../" + u.lstrip("./") if not u.startswith("../") else u.replace("../", "../../", 1)


def table(lines):
    def cells(l):
        l = l.strip().strip("|")
        return [c.strip() for c in re.split(r"(?<!\\)\|", l)]

    head = cells(lines[0])
    body = [cells(l) for l in lines[2:]]
    out = ["<div class=\"tw\"><table><thead><tr>" + "".join("<th>%s</th>" % inline(c) for c in head) + "</tr></thead><tbody>"]
    for r in body:
        out.append("<tr>" + "".join("<td>%s</td>" % inline(c.replace("\\|", "|")) for c in r) + "</tr>")
    out.append("</tbody></table></div>")
    return "\n".join(out)


def render(md):
    """Small Markdown subset: headings, fences, lists (nested), tables, quotes, callouts, rules, paragraphs."""
    lines = md.split("\n")
    i, out, heads = 0, [], []
    while i < len(lines):
        l = lines[i]
        if l.startswith("```"):
            lang = l[3:].strip()
            i += 1
            buf = []
            while i < len(lines) and not lines[i].startswith("```"):
                buf.append(lines[i])
                i += 1
            i += 1
            out.append('<pre><code%s>%s</code></pre>' % (' class="lang-%s"' % e(lang) if lang else "", e("\n".join(buf))))
            continue
        m = re.match(r"^(#{1,4})\s+(.*)$", l)
        if m:
            n, t = len(m.group(1)), m.group(2).strip()
            sid = slugify(t)
            if n in (2, 3):
                heads.append((n, sid, re.sub(r"[`*]", "", t)))
            out.append('<h%d id="%s">%s<a class="anchor" href="#%s" aria-label="Link to this section">#</a></h%d>' % (n, sid, inline(t), sid, n))
            i += 1
            continue
        if re.match(r"^(-{3,}|\*{3,})\s*$", l):
            out.append("<hr>")
            i += 1
            continue
        if l.startswith("|") and i + 1 < len(lines) and re.match(r"^\|?\s*:?-{2,}", lines[i + 1]):
            j = i
            while j < len(lines) and lines[j].startswith("|"):
                j += 1
            out.append(table(lines[i:j]))
            i = j
            continue
        if l.startswith(">"):
            buf = []
            while i < len(lines) and lines[i].startswith(">"):
                buf.append(re.sub(r"^>\s?", "", lines[i]))
                i += 1
            kind = ""
            m2 = re.match(r"^\[!(NOTE|WARNING|TIP)\]\s*$", buf[0]) if buf else None
            if m2:
                kind, buf = m2.group(1).lower(), buf[1:]
            inner, _ = render("\n".join(buf))
            label = {"note": "Note", "warning": "Careful", "tip": "Tip"}.get(kind, "")
            out.append('<aside class="callout %s">%s%s</aside>' % (kind or "quote", ('<b>%s</b>' % label) if label else "", inner))
            continue
        if re.match(r"^(\s*)([-*]|\d+\.)\s+", l):
            html_list, i = lst(lines, i, 0)
            out.append(html_list)
            continue
        if l.strip() == "":
            i += 1
            continue
        if l.startswith("<"):
            out.append(l)
            i += 1
            continue
        buf = []
        while i < len(lines) and lines[i].strip() and not re.match(r"^(#{1,4}\s|```|>|\||(\s*)([-*]|\d+\.)\s+)", lines[i]):
            buf.append(lines[i].strip())
            i += 1
        out.append("<p>%s</p>" % inline(" ".join(buf)))
    return "\n".join(out), heads


def lst(lines, i, indent):
    first = re.match(r"^(\s*)([-*]|\d+\.)\s+", lines[i])
    ordered = first.group(2)[0].isdigit()
    tag = "ol" if ordered else "ul"
    items = []
    while i < len(lines):
        m = re.match(r"^(\s*)([-*]|\d+\.)\s+(.*)$", lines[i])
        if not m or len(m.group(1)) < indent:
            break
        if len(m.group(1)) > indent:
            sub, i = lst(lines, i, len(m.group(1)))
            if items:
                items[-1] += sub
            continue
        text = m.group(3)
        i += 1
        while i < len(lines) and lines[i].strip() and not re.match(r"^\s*([-*]|\d+\.)\s+", lines[i]) and lines[i].startswith(" "):
            text += " " + lines[i].strip()
            i += 1
        items.append(inline(text))
        if i < len(lines) and lines[i].strip() == "" and i + 1 < len(lines) and re.match(r"^(\s*)([-*]|\d+\.)\s+", lines[i + 1]):
            i += 1
    return "<%s>%s</%s>" % (tag, "".join("<li>%s</li>" % it for it in items), tag), i


# ---- generated blocks -------------------------------------------------------

def gen_slash():
    src = open(os.path.join(ROOT, "crates/portcode-cli/src/tui/app.rs")).read()
    block = src[src.index("pub const SLASH"):]
    block = block[:block.index("];")]
    pairs = re.findall(r'\(\s*"([a-z-]+)",\s*"([^"]+)",?\s*\)', block)
    rows = ["| Command | What it does |", "|---|---|"]
    for n, d in pairs:
        rows.append("| `/%s` | %s |" % (n, d.replace("|", "\\|")))
    return "\n".join(rows)


def gen_tools():
    d = os.path.join(ROOT, "crates/portcode-agent/src/tools")
    rows = ["| Tool | What it does |", "|---|---|"]
    seen = set()
    for f in sorted(os.listdir(d)):
        if not f.endswith(".rs"):
            continue
        s = open(os.path.join(d, f)).read()
        for m in re.finditer(r'fn name\(&self\) -> String \{\s*"([a-z_]+)"\.into\(\)\s*\}\s*fn description\(&self\) -> String \{\s*(?:format!\(\s*)?"((?:[^"\\]|\\.)*)"', s):
            name, desc = m.group(1), m.group(2).replace("\\n", " ").replace('\\"', '"')
            if name in seen:
                continue
            seen.add(name)
            first = re.split(r"(?<=[.!?])\s", desc.strip())[0]
            rows.append("| `%s` | %s |" % (name, first.replace("|", "\\|").replace("{}", "…")))
    return "\n".join(rows)


def gen_cli():
    try:
        h = subprocess.run([PC, "--help"], capture_output=True, text=True, timeout=20).stdout
    except Exception:
        h = "(run scripts/build_wiki.py with a built portcode binary to fill this in)"
    return "```text\n%s\n```" % h.strip()


def expand(md):
    return md.replace("{{slash}}", gen_slash()).replace("{{tools}}", gen_tools()).replace("{{cli}}", gen_cli())


# ---- page template ----------------------------------------------------------

def sidebar(cur):
    out, group = [], None
    for g, slug, title, _ in NAV:
        if g != group:
            if group:
                out.append("</ul></section>")
            out.append('<section><h4>%s</h4><ul>' % e(g))
            group = g
        out.append('<li><a href="%s.html"%s>%s</a></li>' % (slug, ' class="on" aria-current="page"' if slug == cur else "", e(title)))
    out.append("</ul></section>")
    return "\n".join(out)


def page(idx, body, heads):
    g, slug, title, _ = NAV[idx]
    prev = NAV[idx - 1] if idx > 0 else None
    nxt = NAV[idx + 1] if idx + 1 < len(NAV) else None
    toc = "".join('<a class="t%d" href="#%s">%s</a>' % (n, sid, e(t)) for n, sid, t in heads)
    pn = '<div class="pn">%s%s</div>' % (
        '<a class="prev" href="%s.html"><small>Previous</small>%s</a>' % (prev[1], e(prev[2])) if prev else "<span></span>",
        '<a class="next" href="%s.html"><small>Next</small>%s</a>' % (nxt[1], e(nxt[2])) if nxt else "<span></span>",
    )
    src = NAV[idx][3]
    edit = "https://github.com/LakoMoor/portcode/edit/main/" + src
    return f"""<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>{e(title)} — PortCode wiki</title><meta property="og:title" content="{e(title)} — PortCode wiki"><meta name="description" content="PortCode documentation: {e(title)}."><link rel="icon" type="image/svg+xml" href="../assets/logo.svg"><script src="../theme.js"></script><script src="../copy.js" defer></script><link rel="stylesheet" href="../style.css"><link rel="stylesheet" href="wiki.css"></head><body>
<nav><div class="wrap"><a class="brand" href="../"><img src="../assets/logo.svg" alt="">PortCode</a><a class="l" href="./">Wiki</a><a class="l keep" href="../remote/">Remote</a><a class="gh" href="https://github.com/LakoMoor/portcode">GitHub</a></div></nav>
<div class="wiki">
<button class="menu" type="button" aria-expanded="false" aria-controls="side">Contents</button>
<aside id="side" class="side"><div class="sbox"><input id="q" type="search" placeholder="Search the wiki" aria-label="Search the wiki" autocomplete="off"><ul id="hits" class="hits" hidden></ul></div>
<div id="navs">{sidebar(slug)}</div></aside>
<main class="doc wk"><p class="crumb"><a href="./">Wiki</a> / {e(g)}</p>
{body}
{pn}
<p class="edit"><a href="{edit}">Edit this page on GitHub</a></p></main>
<aside class="toc" aria-label="On this page"><b>On this page</b>{toc}</aside>
</div>
<footer><div class="wrap">MIT licensed · <a href="https://github.com/LakoMoor/portcode">github.com/LakoMoor/portcode</a></div></footer>
<script src="wiki.js"></script></body></html>
"""


CSS = """/* wiki layout: sidebar, content, on-this-page; reuses the site tokens */
.wiki{display:grid;grid-template-columns:260px minmax(0,1fr) 220px;gap:28px;max-width:1280px;margin:8px auto 0;padding:0 24px;align-items:start}
.wiki main.doc.wk{max-width:none;margin:0;padding:40px 48px;min-width:0}
.side{position:sticky;top:84px;max-height:calc(100vh - 100px);overflow:auto;padding:4px 2px 24px}
.side section{margin:0 0 14px;padding:0}.side h4{margin:0 0 6px;padding:0 12px;font-size:.78rem;letter-spacing:.06em;text-transform:uppercase;color:var(--mut);font-weight:650}
.side ul{list-style:none;margin:0;padding:0}.side li a{display:block;padding:6px 12px;border-radius:10px;color:var(--ink2);text-decoration:none;font-size:.95rem;line-height:1.35}
.side li a:hover{background:var(--white);color:var(--ink)}.side li a.on{background:var(--white);color:var(--ink);font-weight:650;box-shadow:var(--shadow)}
.sbox{position:relative;margin:0 0 16px}.sbox input{width:100%;font:inherit;font-size:.95rem;padding:9px 14px;border-radius:12px;border:1px solid var(--rule);background:var(--white);color:var(--ink)}
.hits{list-style:none;margin:6px 0 0;padding:6px;background:var(--white);border-radius:12px;box-shadow:var(--shadow)}.hits a{display:block;padding:7px 9px;border-radius:8px;text-decoration:none;color:var(--ink);font-size:.92rem}
.hits a:hover,.hits a.sel{background:var(--paper)}.hits small{display:block;color:var(--mut)}
.toc{position:sticky;top:84px;max-height:calc(100vh - 100px);overflow:auto;font-size:.86rem;padding:6px 0}.toc b{display:block;font-size:.78rem;letter-spacing:.06em;text-transform:uppercase;color:var(--mut);margin:0 0 8px}
.toc a{display:block;color:var(--mut);text-decoration:none;padding:3px 0 3px 12px;border-left:2px solid var(--rule);line-height:1.35}.toc a.t3{padding-left:24px}.toc a:hover,.toc a.on{color:var(--ink);border-left-color:var(--orange)}
.wk p,.wk li,.wk td,.wk h1,.wk h2,.wk h3{overflow-wrap:anywhere}.wk pre{max-width:100%}
.crumb{margin:0 0 6px;color:var(--mut);font-size:.9rem}.crumb a{color:var(--mut)}
.wk h1{margin-bottom:.5em}.wk h2{margin-top:1.9em;padding-top:.4em}.wk h2:first-of-type{margin-top:1.2em}
.wk .anchor{margin-left:.4em;color:var(--mut);text-decoration:none;opacity:0;font-weight:400;font-size:.8em}.wk h2:hover .anchor,.wk h3:hover .anchor,.wk .anchor:focus{opacity:.8}
.wk .tw{overflow-x:auto;margin:18px 0;border:1px solid var(--rule);border-radius:14px}.wk .tw table{margin:0;border:0;display:table;overflow:visible;box-shadow:none;border-radius:0;background:transparent;width:100%;min-width:480px}
.wk th,.wk td{text-align:left;vertical-align:top;padding:10px 14px;border-bottom:1px solid var(--rule)}.wk tr:last-child td{border-bottom:0}.wk th{font-size:.84rem;color:var(--mut);font-weight:650}
.callout{margin:20px 0;padding:14px 18px;border-radius:14px;background:var(--paper);border-left:4px solid var(--blue)}.callout p{margin:.4em 0}.callout>b{display:block;margin-bottom:2px}
.callout.warning{border-left-color:var(--red)}.callout.tip{border-left-color:var(--green)}.callout.quote{border-left-color:var(--rule);color:var(--ink2)}
.pn{display:grid;grid-template-columns:1fr 1fr;gap:14px;margin:44px 0 10px}.pn a{display:block;padding:14px 18px;border:1px solid var(--rule);border-radius:14px;text-decoration:none;color:var(--ink);font-weight:650}
.pn a:hover{border-color:var(--orange)}.pn small{display:block;color:var(--mut);font-weight:400;font-size:.8rem}.pn .next{text-align:right}
.edit{margin:6px 0 0;font-size:.88rem}.menu{display:none}
.wk kbd{font:.85em var(--mono);padding:.1em .45em;border:1px solid var(--rule);border-bottom-width:2px;border-radius:6px;background:var(--paper)}
@media(max-width:1180px){.wiki{grid-template-columns:240px minmax(0,1fr)}.toc{display:none}}
@media(max-width:860px){
 .wiki{grid-template-columns:minmax(0,1fr);gap:10px;padding:0 14px}
 .menu{display:block;width:100%;font:inherit;font-weight:650;text-align:left;padding:11px 16px;border-radius:14px;border:1px solid var(--rule);background:var(--white);color:var(--ink)}
 .side{display:none;position:static;max-height:none;background:var(--white);border-radius:16px;padding:14px;box-shadow:var(--shadow)}.side.open{display:block}
 .wiki main.doc.wk{padding:26px 20px;border-radius:22px}.pn{grid-template-columns:1fr}
}
[data-theme=dark] .callout{background:rgba(255,255,255,.05)}[data-theme=dark] .wk kbd{background:rgba(255,255,255,.06)}
[data-theme=dark] .side li a:hover,[data-theme=dark] .side li a.on{background:var(--dark2)}[data-theme=dark] .hits a:hover,[data-theme=dark] .hits a.sel{background:var(--dark)}
"""

JS = """(function(){
var q=document.getElementById('q'),hits=document.getElementById('hits'),navs=document.getElementById('navs'),idx=null,sel=-1;
var btn=document.querySelector('.menu'),side=document.getElementById('side');
if(btn)btn.onclick=function(){var o=side.classList.toggle('open');btn.setAttribute('aria-expanded',o)};
function load(cb){if(idx)return cb();fetch('search.json').then(function(r){return r.json()}).then(function(j){idx=j;cb()}).catch(function(){idx=[];cb()})}
function run(){var t=q.value.trim().toLowerCase();if(!t){hits.hidden=true;navs.hidden=false;return}
load(function(){var words=t.split(/\\s+/),r=[];idx.forEach(function(p){var s=0;words.forEach(function(w){if(p.t.toLowerCase().indexOf(w)>=0)s+=10;if(p.h.toLowerCase().indexOf(w)>=0)s+=4;var c=p.x.split(w).length-1;s+=Math.min(c,12)});
 var ok=words.every(function(w){return (p.t+' '+p.h+' '+p.x).toLowerCase().indexOf(w)>=0});if(ok)r.push([s,p])});
 r.sort(function(a,b){return b[0]-a[0]});hits.innerHTML='';sel=-1;
 r.slice(0,8).forEach(function(x){var p=x[1],li=document.createElement('li'),a=document.createElement('a');a.href=p.u;a.textContent=p.t;var s=document.createElement('small');s.textContent=p.g;a.appendChild(s);li.appendChild(a);hits.appendChild(li)});
 if(!r.length){var li=document.createElement('li');li.style.padding='8px 10px';li.textContent='Nothing found';hits.appendChild(li)}
 hits.hidden=false;navs.hidden=true})}
if(q){q.addEventListener('input',run);q.addEventListener('keydown',function(ev){var a=hits.querySelectorAll('a');if(ev.key==='ArrowDown'||ev.key==='ArrowUp'){ev.preventDefault();if(!a.length)return;sel=(sel+(ev.key==='ArrowDown'?1:-1)+a.length)%a.length;a.forEach(function(x,i){x.classList.toggle('sel',i===sel)})}
 else if(ev.key==='Enter'&&a.length){location.href=a[Math.max(sel,0)].href}else if(ev.key==='Escape'){q.value='';run()}});
 addEventListener('keydown',function(ev){if(ev.key==='/'&&document.activeElement!==q&&!/input|textarea/i.test(document.activeElement.tagName)){ev.preventDefault();q.focus()}})}
var links=[].slice.call(document.querySelectorAll('.toc a'));
if(links.length&&'IntersectionObserver' in window){var map={};links.forEach(function(a){map[a.getAttribute('href').slice(1)]=a});
 var io=new IntersectionObserver(function(es){es.forEach(function(e){if(e.isIntersecting){links.forEach(function(a){a.classList.remove('on')});var a=map[e.target.id];if(a)a.classList.add('on')}})},{rootMargin:'-80px 0px -70% 0px'});
 document.querySelectorAll('.wk h2[id],.wk h3[id]').forEach(function(h){io.observe(h)})}
})();
"""


def strip_md(t):
    t = re.sub(r"```.*?```", " ", t, flags=re.S)
    t = re.sub(r"[`*#|>\[\]()_-]+", " ", t)
    return re.sub(r"\s+", " ", t).lower()[:1800]


def main():
    os.makedirs(OUT, exist_ok=True)
    index = []
    for i, (g, slug, title, src) in enumerate(NAV):
        md = open(os.path.join(ROOT, src), encoding="utf-8").read()
        body, heads = render(expand(md))
        if "<h1" not in body:
            body = "<h1>%s</h1>\n" % e(title) + body
        open(os.path.join(OUT, slug + ".html"), "w", encoding="utf-8").write(page(i, body, heads))
        index.append({"u": slug + ".html", "t": title, "g": g, "h": " ".join(h[2] for h in heads), "x": strip_md(md)})
    open(os.path.join(OUT, "search.json"), "w", encoding="utf-8").write(json.dumps(index, ensure_ascii=False))
    open(os.path.join(OUT, "wiki.css"), "w").write(CSS)
    open(os.path.join(OUT, "wiki.js"), "w").write(JS)
    gd = os.path.join(ROOT, "docs/guide")
    os.makedirs(gd, exist_ok=True)
    for old, new in GUIDE_REDIRECTS.items():
        open(os.path.join(gd, old + ".html"), "w").write(
            '<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Moved</title>'
            '<meta http-equiv="refresh" content="0; url=../wiki/%s.html"><link rel="canonical" href="https://lakomoor.github.io/portcode/wiki/%s.html">'
            '</head><body><p>This page moved to the <a href="../wiki/%s.html">wiki</a>.</p></body></html>\n' % (new, new, new))
    print("wiki: %d pages in docs/wiki/" % len(NAV))


main()
