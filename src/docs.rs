//! Help > Documentation: the HTML pages made from the repository's Markdown
//! (`xms_docs`), opened in the system's browser. Beside the program's own
//! `docs` folder (a release, or the source tree when run through cargo) the
//! pages are made again first, so they are the Markdown as it is now; where
//! that cannot be written, the pages already there are opened.

use std::path::{Path, PathBuf};

/// The documentation's first page, made from the Markdown where it can be.
fn first_page() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let beside = exe.parent().unwrap_or(Path::new("."));
    let root = xms_docs::find_root(beside);
    let made = root.as_ref().and_then(|root| {
        let out = root.join("docs/html");
        xms_docs::build(root, &out).ok().map(|_| out.join("index.html"))
    });
    made.into_iter()
        .chain(root.map(|r| r.join("docs/html/index.html")))
        .chain([beside.join("docs/html/index.html")])
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "No documentation beside the program: it is in docs/ of the repository, \
             https://github.com/MartinoMadeddu/xms-imago/tree/main/docs"
                .into()
        })
}

/// `page` in the system's browser.
fn show(page: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut c = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut c = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut c = std::process::Command::new("xdg-open");
    c.arg(page)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open {}: {e}", page.display()))
}

/// Open the documentation; what went wrong, if anything.
pub fn open() -> Result<(), String> {
    show(&first_page()?)
}
