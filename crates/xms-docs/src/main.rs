//! `cargo run -p xms-docs [-- <folder>]`: the documentation as HTML pages in
//! `<folder>` (`docs/html` by default), made from the repository's Markdown.
//! Says what did not lead anywhere (the test
//! `the_repositorys_documentation_builds_with_every_link_found` fails on it).

fn main() {
    let here = std::env::current_dir().expect("the working folder");
    let Some(root) = xms_docs::find_root(&here) else {
        eprintln!(
            "Run from inside the XMS | Imago repository: no docs/README.md above {}",
            here.display()
        );
        std::process::exit(2);
    };
    let out = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("docs/html"));
    match xms_docs::build(&root, &out) {
        Ok(built) => {
            for p in &built.problems {
                eprintln!("{p}");
            }
            println!("{} pages in {}", built.pages, out.join("index.html").display());
        }
        Err(e) => {
            eprintln!("{}: {e}", out.display());
            std::process::exit(1);
        }
    }
}
