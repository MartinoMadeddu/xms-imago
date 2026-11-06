//! XMS | Imago's documentation as HTML pages, made from the repository's
//! Markdown, to open in a browser from the disk: no server, nothing fetched.
//! The Markdown stays the source; the pages are made again from it.
//!
//! The pages: the README as the Overview, the User Guide in the order
//! `docs/README.md` lists its pages, and a Reference of the operators,
//! vocabulary, architecture and design philosophy. Each page has the guide's
//! contents on its left, its own headings on its right and a search over every
//! page at the top. Links between the Markdown files become links between the
//! pages, images are copied beside them, and links to anything else in the
//! repository go to it on GitHub.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use pulldown_cmark::{CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// Where a link to a repository file that is not a page goes.
const REPOSITORY: &str = "https://github.com/MartinoMadeddu/xms-imago/blob/main/";

/// The Reference pages: the Markdown file and its page.
const REFERENCE: [(&str, &str); 4] = [
    ("XMS_CORE_OPS_SPEC.md", "operators.html"),
    ("docs/vocabulary.md", "vocabulary.html"),
    ("ARCHITECTURE.md", "architecture.html"),
    ("DESIGN_PHILOSOPHY.md", "design-philosophy.html"),
];

const STYLE: &str = include_str!("../assets/style.css");
const SEARCH: &str = include_str!("../assets/search.js");

/// One page: its Markdown (a path in the repository), its file, its name in
/// the contents and the part of the contents it is in.
#[derive(Clone, Debug)]
struct Page {
    source: String,
    file: String,
    title: String,
    part: &'static str,
}

/// What a build made, and what in the Markdown did not lead anywhere (a
/// missing file or image, a heading no page has).
#[derive(Debug, Default)]
pub struct Built {
    pub pages: usize,
    pub problems: Vec<String>,
}

/// The pages, in the order of the contents: the Overview, the User Guide as
/// `docs/README.md` lists it, then the Reference.
fn pages(root: &Path) -> Vec<Page> {
    let mut out = vec![Page {
        source: "README.md".into(),
        file: "index.html".into(),
        title: "Overview".into(),
        part: "",
    }];
    let list = std::fs::read_to_string(root.join("docs/README.md")).unwrap_or_default();
    for line in list.lines() {
        // "- [Title](page.md): what it holds"
        let Some(rest) = line.strip_prefix("- [") else { continue };
        let Some((title, rest)) = rest.split_once("](") else {
            continue;
        };
        let Some((md, _)) = rest.split_once(')') else { continue };
        if let Some(stem) = md.strip_suffix(".md").filter(|s| !s.contains('/')) {
            out.push(Page {
                source: format!("docs/{md}"),
                file: format!("{stem}.html"),
                title: title.into(),
                part: "User Guide",
            });
        }
    }
    for (md, file) in REFERENCE {
        out.push(Page {
            source: md.into(),
            file: file.into(),
            title: String::new(),
            part: "Reference",
        });
    }
    out.retain(|p| root.join(&p.source).is_file());
    out
}

/// A heading's anchor as GitHub makes it: lower case, spaces as hyphens,
/// punctuation dropped; a second heading of the same words numbered.
fn slug(text: &str, used: &mut HashMap<String, usize>) -> String {
    let base: String = text
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect();
    let n = used.entry(base.clone()).or_insert(0);
    let id = if *n == 0 { base.clone() } else { format!("{base}-{n}") };
    *n += 1;
    id
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `dest`, relative to the folder `from` (a path in the repository), as a
/// path from the repository's root; `None` when it leaves it.
fn in_repository(from: &str, dest: &str) -> Option<String> {
    let mut parts: Vec<&str> = from.split('/').filter(|p| !p.is_empty()).collect();
    for p in dest.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    Some(parts.join("/"))
}

/// A page made: its HTML, its headings for the right-hand contents (level,
/// anchor, text) and its sections for the search (anchor, heading, text).
struct Made {
    title: String,
    html: String,
    headings: Vec<(u8, String, String)>,
    sections: Vec<(String, String, String)>,
}

/// What the pages link to, for checking once every page is made.
struct Links<'a> {
    by_source: &'a HashMap<String, String>,
    images: HashSet<String>,
    anchors: Vec<(String, String, String)>,
    /// From the pages up to the repository's root, when they are in it
    /// (`docs/html`, also in a release): images in `docs/` (which a release
    /// has) are linked where they are; others are copied beside the pages.
    up: Option<String>,
}

fn make(root: &Path, page: &Page, md: &str, links: &mut Links, problems: &mut Vec<String>) -> Made {
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut events: Vec<Event> = Parser::new_ext(md, options).collect();
    let folder = page.source.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let mut used = HashMap::new();
    let mut headings = vec![];
    let mut sections: Vec<(String, String, String)> = vec![(String::new(), String::new(), String::new())];
    let mut title = String::new();

    // Each heading's text, for its anchor.
    let mut i = 0;
    while i < events.len() {
        if let Event::Start(Tag::Heading { level, .. }) = &events[i] {
            let level = *level;
            let mut text = String::new();
            let mut j = i + 1;
            while !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                if let Event::Text(t) | Event::Code(t) = &events[j] {
                    text.push_str(t);
                }
                j += 1;
            }
            let id = slug(&text, &mut used);
            if level == HeadingLevel::H1 && title.is_empty() {
                title = text.clone();
            }
            let n = level as u8;
            if n == 2 || n == 3 {
                headings.push((n, id.clone(), text.clone()));
            }
            if n <= 3 {
                sections.push((id.clone(), text, String::new()));
            }
            if let Event::Start(Tag::Heading { id: slot, .. }) = &mut events[i] {
                *slot = Some(CowStr::from(id));
            }
            i = j;
        }
        i += 1;
    }

    // Links and images to where they are in the pages; text for the search;
    // a table in a box of its own, to scroll when it is wide.
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut section = 0;
    let mut in_heading = false;
    let mut links_open = 0;
    for event in events {
        match event {
            Event::Start(Tag::Heading {
                level,
                id,
                classes,
                attrs,
            }) => {
                if (level as u8) <= 3 {
                    section += 1;
                }
                in_heading = true;
                out.push(Event::Start(Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                }));
            }
            Event::End(TagEnd::Heading(l)) => {
                in_heading = false;
                out.push(Event::End(TagEnd::Heading(l)));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest = link(root, folder, &page.file, &dest_url, links, problems, false);
                links_open += 1;
                out.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: dest.into(),
                    title,
                    id,
                }));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest = link(root, folder, &page.file, &dest_url, links, problems, true);
                // A picture opens at full size when clicked, unless it is a
                // link already.
                if links_open == 0 {
                    out.push(Event::Html(
                        format!("<a class=\"full\" href=\"{}\">", escape(&dest)).into(),
                    ));
                }
                out.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: dest.into(),
                    title,
                    id,
                }));
            }
            Event::End(TagEnd::Link) => {
                links_open -= 1;
                out.push(Event::End(TagEnd::Link));
            }
            Event::End(TagEnd::Image) => {
                out.push(Event::End(TagEnd::Image));
                if links_open == 0 {
                    out.push(Event::Html("</a>".into()));
                }
            }
            Event::Start(Tag::Table(a)) => {
                out.push(Event::Html("<div class=\"table\">".into()));
                out.push(Event::Start(Tag::Table(a)));
            }
            Event::End(TagEnd::Table) => {
                out.push(Event::End(TagEnd::Table));
                out.push(Event::Html("</div>".into()));
            }
            Event::Text(t) if !in_heading => {
                sections[section].2.push_str(&t);
                out.push(Event::Text(t));
            }
            Event::Code(t) if !in_heading => {
                sections[section].2.push_str(&t);
                out.push(Event::Code(t));
            }
            // Words of one cell, item or paragraph kept apart from the next's.
            e @ Event::End(TagEnd::Paragraph | TagEnd::TableCell | TagEnd::Item | TagEnd::CodeBlock) => {
                sections[section].2.push(' ');
                out.push(e);
            }
            e @ (Event::SoftBreak | Event::HardBreak) => {
                sections[section].2.push(' ');
                out.push(e);
            }
            e => out.push(e),
        }
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, out.into_iter());
    if title.is_empty() {
        title = page.title.clone();
    }
    Made {
        title,
        html,
        headings,
        sections: sections
            .into_iter()
            .filter(|s| !s.2.trim().is_empty() || !s.1.is_empty())
            .collect(),
    }
}

/// Where a link or an image in `page` (in `folder`) goes among the pages.
fn link(
    root: &Path,
    folder: &str,
    page: &str,
    dest: &str,
    links: &mut Links,
    problems: &mut Vec<String>,
    image: bool,
) -> String {
    if dest.contains("://") || dest.starts_with("mailto:") {
        return dest.into();
    }
    if let Some(anchor) = dest.strip_prefix('#') {
        links.anchors.push((page.into(), page.into(), anchor.into()));
        return dest.into();
    }
    let (path, anchor) = match dest.split_once('#') {
        Some((p, a)) => (p, Some(a)),
        None => (dest, None),
    };
    let Some(path) = in_repository(folder, path) else {
        problems.push(format!("{page}: {dest} is outside the repository"));
        return dest.into();
    };
    if let Some(file) = links.by_source.get(&path) {
        if let Some(a) = anchor {
            links.anchors.push((page.into(), file.clone(), a.into()));
            return format!("{file}#{a}");
        }
        return file.clone();
    }
    if !root.join(&path).exists() {
        problems.push(format!("{page}: {dest} is not in the repository"));
    }
    if image {
        if let Some(up) = links.up.as_ref().filter(|_| path.starts_with("docs/")) {
            return format!("{up}{path}");
        }
        links.images.insert(path.clone());
        let name = path.rsplit('/').next().unwrap_or(&path);
        return format!("img/{name}");
    }
    let mut url = format!("{REPOSITORY}{path}");
    if let Some(a) = anchor {
        url.push('#');
        url.push_str(a);
    }
    url
}

/// The page around a page's content.
fn frame(pages: &[Page], at: usize, made: &Made, titles: &[String]) -> String {
    let mut nav = String::new();
    let mut part = "";
    for (k, p) in pages.iter().enumerate() {
        if p.part != part {
            if !part.is_empty() || k > 0 {
                nav.push_str("</ul>");
            }
            part = p.part;
            let _ = write!(nav, "<p class=\"part\">{}</p>", escape(part));
            nav.push_str("<ul>");
        } else if k == 0 {
            nav.push_str("<ul>");
        }
        let here = if k == at {
            " class=\"here\" aria-current=\"page\""
        } else {
            ""
        };
        let _ = write!(nav, "<li><a href=\"{}\"{here}>{}</a></li>", p.file, escape(&titles[k]));
    }
    nav.push_str("</ul>");

    let mut toc = String::new();
    if made.headings.len() > 1 {
        toc.push_str("<p class=\"part\">On this page</p><ul>");
        for (level, id, text) in &made.headings {
            let _ = write!(
                toc,
                "<li class=\"h{level}\"><a href=\"#{id}\">{}</a></li>",
                escape(text)
            );
        }
        toc.push_str("</ul>");
    }

    let mut pager = String::new();
    if at > 0 {
        let p = &pages[at - 1];
        let _ = write!(
            pager,
            "<a class=\"prev\" href=\"{}\"><span>Previous</span>{}</a>",
            p.file,
            escape(&titles[at - 1])
        );
    }
    if let Some(p) = pages.get(at + 1) {
        let _ = write!(
            pager,
            "<a class=\"next\" href=\"{}\"><span>Next</span>{}</a>",
            p.file,
            escape(&titles[at + 1])
        );
    }

    let title = if at == 0 {
        "XMS | Imago documentation".to_string()
    } else {
        format!("{} · XMS | Imago documentation", made.title)
    };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="style.css">
<script>try {{ var t = localStorage.getItem("imago-docs-theme"); if (t) document.documentElement.dataset.theme = t; }} catch (e) {{}}</script>
</head>
<body>
<header class="top">
  <button class="menu" type="button" aria-label="Contents">&#9776;</button>
  <a class="brand" href="index.html"><span class="mark">XMS | Imago</span><span class="sub">Documentation</span></a>
  <div class="find">
    <input id="search" type="search" placeholder="Search (press /)" autocomplete="off" aria-label="Search the documentation">
    <div id="results" hidden></div>
  </div>
  <button class="theme" type="button" aria-label="Night mode" title="Night mode">&#9790;</button>
</header>
<div class="layout">
  <nav class="side" aria-label="Contents">{nav}</nav>
  <main>
    <article>
{content}
    </article>
    <footer class="pager">{pager}</footer>
  </main>
  <aside class="toc" aria-label="On this page">{toc}</aside>
</div>
<script src="search-index.js"></script>
<script src="search.js"></script>
</body>
</html>
"#,
        title = escape(&title),
        content = made.html,
    )
}

/// The search's index, as a script setting `IMAGO_INDEX` (a page opened from
/// the disk cannot fetch a file): per section its page, the page's title, the
/// heading's anchor and words, and its text.
fn index(pages: &[Page], made: &[Made], titles: &[String]) -> String {
    let quote = |s: &str| {
        let mut q = String::with_capacity(s.len() + 2);
        q.push('"');
        for c in s.chars() {
            match c {
                '"' => q.push_str("\\\""),
                '\\' => q.push_str("\\\\"),
                '\n' | '\r' | '\t' => q.push(' '),
                '<' => q.push_str("\\u003c"),
                c if (c as u32) < 0x20 => {}
                c => q.push(c),
            }
        }
        q.push('"');
        q
    };
    let mut out = String::from("var IMAGO_INDEX = [\n");
    for ((p, m), title) in pages.iter().zip(made).zip(titles) {
        for (id, heading, text) in &m.sections {
            let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
            let _ = writeln!(
                out,
                "[{},{},{},{},{}],",
                quote(&p.file),
                quote(title),
                quote(id),
                quote(heading),
                quote(&text)
            );
        }
    }
    out.push_str("];\n");
    out
}

/// Make the pages from the repository at `root` into `out`, over what is
/// there (nothing else in it is removed).
pub fn build(root: &Path, out: &Path) -> std::io::Result<Built> {
    let pages = pages(root);
    let by_source: HashMap<String, String> = pages.iter().map(|p| (p.source.clone(), p.file.clone())).collect();
    std::fs::create_dir_all(out)?;
    let up = out
        .canonicalize()?
        .strip_prefix(root.canonicalize()?)
        .ok()
        .map(|inside| "../".repeat(inside.components().count()));
    let mut links = Links {
        by_source: &by_source,
        images: HashSet::new(),
        anchors: vec![],
        up,
    };
    let mut problems = vec![];
    let mut made = vec![];
    for p in &pages {
        let md = std::fs::read_to_string(root.join(&p.source))?;
        made.push(make(root, p, &md, &mut links, &mut problems));
    }
    // The contents name a guide page as docs/README.md does, a reference page
    // by its first heading.
    let titles: Vec<String> = pages
        .iter()
        .zip(&made)
        .map(|(p, m)| {
            if p.title.is_empty() {
                m.title.clone()
            } else {
                p.title.clone()
            }
        })
        .collect();

    for (from, to, anchor) in &links.anchors {
        let Some(k) = pages.iter().position(|p| &p.file == to) else {
            continue;
        };
        let found =
            made[k].sections.iter().any(|s| &s.0 == anchor) || made[k].html.contains(&format!("id=\"{anchor}\""));
        if !found {
            problems.push(format!("{from}: no heading #{anchor} in {to}"));
        }
    }

    if !links.images.is_empty() {
        std::fs::create_dir_all(out.join("img"))?;
    }
    for (k, m) in made.iter().enumerate() {
        std::fs::write(out.join(&pages[k].file), frame(&pages, k, m, &titles))?;
    }
    std::fs::write(out.join("style.css"), STYLE)?;
    std::fs::write(out.join("search.js"), SEARCH)?;
    std::fs::write(out.join("search-index.js"), index(&pages, &made, &titles))?;
    for path in &links.images {
        let name = path.rsplit('/').next().unwrap_or(path);
        if root.join(path).is_file() {
            std::fs::copy(root.join(path), out.join("img").join(name))?;
        }
    }
    let fonts = root.join("site/fonts");
    for font in [
        "michroma-latin-400-normal.woff2",
        "archivo-narrow-latin-700-normal.woff2",
        // Anisette, for the name at the top, when a licensed copy is there:
        // it cannot be shipped, so the pages fall back to an installed one,
        // then to Michroma.
        "anisette.woff2",
        "anisette.otf",
    ] {
        if fonts.join(font).is_file() {
            std::fs::create_dir_all(out.join("fonts"))?;
            std::fs::copy(fonts.join(font), out.join("fonts").join(font))?;
        }
    }
    problems.sort();
    problems.dedup();
    Ok(Built {
        pages: pages.len(),
        problems,
    })
}

/// The repository's root: the nearest folder above `from` (or `from`) with
/// `docs/README.md` in it.
pub fn find_root(from: &Path) -> Option<std::path::PathBuf> {
    from.ancestors()
        .find(|d| d.join("docs/README.md").is_file() && d.join("README.md").is_file())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_are_githubs() {
        let mut used = HashMap::new();
        assert_eq!(slug("Big stages: boxes first", &mut used), "big-stages-boxes-first");
        assert_eq!(
            slug("Set Variable: values for everything below", &mut used),
            "set-variable-values-for-everything-below"
        );
        assert_eq!(slug("`Geo` and mesh", &mut used), "geo-and-mesh");
        assert_eq!(slug("Lights", &mut used), "lights");
        assert_eq!(slug("Lights", &mut used), "lights-1");
    }

    #[test]
    fn links_are_found_from_their_folder() {
        assert_eq!(in_repository("docs", "../README.md").as_deref(), Some("README.md"));
        assert_eq!(in_repository("docs", "usd.md").as_deref(), Some("docs/usd.md"));
        assert_eq!(in_repository("", "docs/x.png").as_deref(), Some("docs/x.png"));
        assert_eq!(in_repository("", "../out.md"), None);
    }

    /// The repository's own documentation makes every page, each link and
    /// image leading somewhere.
    #[test]
    fn the_repositorys_documentation_builds_with_every_link_found() {
        let root = find_root(Path::new(env!("CARGO_MANIFEST_DIR"))).expect("the repository");
        let out = std::env::temp_dir().join("xms_docs_test");
        let built = build(&root, &out).unwrap();
        assert!(built.pages > 10, "{} pages", built.pages);
        assert!(built.problems.is_empty(), "{}", built.problems.join("\n"));
        let index = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(index.contains("usd.html") && index.contains("operators.html"));
        let usd = std::fs::read_to_string(out.join("usd.html")).unwrap();
        assert!(usd.contains("id=\"big-stages-boxes-first\""));
        assert!(usd.contains("<code>"), "code spans stay code");
        assert!(!usd.contains(".md\""), "links between the pages are to the pages");
        let interface = std::fs::read_to_string(out.join("interface.html")).unwrap();
        assert!(
            interface.contains("<a class=\"full\" href="),
            "a picture opens at full size"
        );
    }
}
