/// Where a README lives, for resolving its relative links.
pub struct Source<'a> {
    pub owner: &'a str,
    pub repo: &'a str,
    pub branch: &'a str,
    /// Folder of the README in the repository, "" for the root.
    pub dir: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub url: String,
    pub alt: String,
}

/// The README's folder: GitHub serves the root README first, then `.github/` and `docs/`.
pub fn readme_dir(paths: &[String]) -> &'static str {
    let in_dir = |dir: &str| {
        paths.iter().any(|p| {
            p.strip_prefix(dir).is_some_and(|rest| {
                !rest.contains('/') && rest.to_ascii_lowercase().starts_with("readme")
            })
        })
    };
    if in_dir("") {
        ""
    } else if in_dir(".github/") {
        ".github"
    } else if in_dir("docs/") {
        "docs"
    } else {
        ""
    }
}

/// The text with fenced code blocks blanked, so examples inside them are not read as content.
fn without_code(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    let mut fence: Option<&str> = None;
    for line in md.lines() {
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m));
        match (fence, marker) {
            (None, Some(m)) => fence = Some(m),
            (Some(f), Some(m)) if f == m => fence = None,
            (None, None) => out.push_str(line),
            _ => {}
        }
        out.push('\n');
    }
    out
}

/// Every Markdown and HTML image in document order, unresolved.
pub fn images(md: &str) -> Vec<Image> {
    let text = without_code(md);
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let md_pos = text[at..].find("![").map(|p| p + at);
        let html_pos = lower[at..].find("<img").map(|p| p + at);
        let next = match (md_pos, html_pos) {
            (Some(m), Some(h)) => m.min(h),
            (Some(m), None) => m,
            (None, Some(h)) => h,
            (None, None) => break,
        };
        let parsed = if Some(next) == md_pos {
            markdown_image(&text[next..])
        } else {
            html_image(&text[next..], &lower[next..])
        };
        match parsed {
            Some((image, used)) => {
                found.push(image);
                at = next + used;
            }
            None => at = next + 2,
        }
    }
    found
}

/// `![alt](url "title")` at the start of `s`; returns the image and the bytes consumed.
fn markdown_image(s: &str) -> Option<(Image, usize)> {
    let close = s.find("](")?;
    let alt = &s[2..close];
    if alt.contains("\n\n") {
        return None;
    }
    let rest = &s[close + 2..];
    let end = rest.find(')')?;
    let inner = rest[..end].trim();
    let url = if let Some(stripped) = inner.strip_prefix('<') {
        stripped.split('>').next().unwrap_or("")
    } else {
        inner.split_whitespace().next().unwrap_or("")
    };
    if url.is_empty() {
        return None;
    }
    let image = Image {
        url: url.to_string(),
        alt: alt.trim().to_string(),
    };
    Some((image, close + 2 + end + 1))
}

/// `<img ... src="..." alt="...">` at the start of `s` (with `lower` its lowercase copy).
fn html_image(s: &str, lower: &str) -> Option<(Image, usize)> {
    let end = lower.find('>')?;
    let tag = &s[..end];
    let tag_lower = &lower[..end];
    let url = attribute(tag, tag_lower, "src")?;
    let alt = attribute(tag, tag_lower, "alt").unwrap_or_default();
    Some((Image { url, alt }, end + 1))
}

fn attribute(tag: &str, tag_lower: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(pos) = tag_lower[from..].find(name).map(|p| p + from) {
        from = pos + name.len();
        let before_ok = pos > 0 && tag_lower.as_bytes()[pos - 1].is_ascii_whitespace();
        let after = tag[from..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let value = after[1..].trim_start();
        let parsed = match value.chars().next() {
            Some(q @ ('"' | '\'')) => value[1..].split(q).next().unwrap_or(""),
            _ => value
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or(""),
        };
        return Some(parsed.trim().to_string());
    }
    None
}

/// Status badges and vector logos are not screenshots.
pub fn is_badge(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let path = lower.split(['?', '#']).next().unwrap_or("");
    lower.contains("shields.io") || lower.contains("badge") || path.ends_with(".svg")
}

/// An absolute https URL for an image source, or None for data URIs and other schemes.
pub fn resolve(src: &str, source: &Source) -> Option<String> {
    let src = src.trim();
    if src.is_empty() || src.starts_with('#') {
        return None;
    }
    if let Some(rest) = src.strip_prefix("//") {
        return Some(format!("https://{rest}"));
    }
    if src.starts_with("https://") || src.starts_with("http://") {
        return Some(github_file_to_raw(src));
    }
    let scheme_end = src.find(':');
    let slash = src.find('/');
    if scheme_end.is_some_and(|c| slash.is_none_or(|s| c < s)) {
        return None;
    }
    let path = src.split(['?', '#']).next().unwrap_or("");
    let joined = match path.strip_prefix('/') {
        Some(from_root) => from_root.to_string(),
        None if source.dir.is_empty() => path.to_string(),
        None => format!("{}/{path}", source.dir),
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(crate::github::raw_url(
        source.owner,
        source.repo,
        source.branch,
        &parts.join("/"),
    ))
}

/// github.com file pages (`/blob/` or `/raw/`) become their raw.githubusercontent.com address.
fn github_file_to_raw(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return url.to_string();
    };
    let parts: Vec<&str> = rest.splitn(4, '/').collect();
    match parts.as_slice() {
        [owner, repo, "blob" | "raw", tail] => {
            let tail = tail.split(['?', '#']).next().unwrap_or("");
            format!("https://raw.githubusercontent.com/{owner}/{repo}/{tail}")
        }
        _ => url.to_string(),
    }
}

/// The first image that is not a badge, resolved against the repository.
pub fn first_image(md: &str, source: &Source) -> Option<Image> {
    images(md).into_iter().find_map(|image| {
        let url = resolve(&image.url, source)?;
        (!is_badge(&url)).then_some(Image {
            url,
            alt: image.alt,
        })
    })
}

/// Markdown links, emphasis and code marks reduced to their text.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find("](") {
            Some(close) => {
                let target = &after[close + 2..];
                let end = target.find(')').unwrap_or(target.len());
                out.push_str(&after[..close]);
                rest = target.get(end + 1..).unwrap_or("");
            }
            None => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace(['*', '`'], "")
        .replace("__", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text up to and including the first sentence end.
pub fn first_sentence(text: &str) -> String {
    let text = text.trim();
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if !matches!(b, b'.' | b'!' | b'?') {
            continue;
        }
        let at_end = i + 1 == bytes.len();
        let before_space = bytes.get(i + 1).is_some_and(|c| c.is_ascii_whitespace());
        let word = text[..i].rsplit(' ').next().unwrap_or("");
        let abbreviation = matches!(word, "e.g" | "i.e" | "etc" | "vs" | "Dr" | "Mr" | "Ms");
        if (at_end || before_space) && !abbreviation {
            return text[..=i].to_string();
        }
    }
    text.to_string()
}

fn is_ordered_item(paragraph: &str) -> bool {
    let digits = paragraph.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && paragraph[digits..].starts_with(". ")
}

/// The README's first prose paragraph cut to one sentence, for repositories without a description.
pub fn first_paragraph_sentence(md: &str) -> Option<String> {
    let text = without_code(md);
    let paragraphs = text.split("\n\n").map(str::trim).filter(|p| !p.is_empty());
    for paragraph in paragraphs {
        let first = paragraph.chars().next().unwrap_or(' ');
        let structural = matches!(first, '#' | '<' | '!' | '|' | '>' | '-' | '*' | '=' | '+')
            || paragraph.starts_with("[!")
            || is_ordered_item(paragraph);
        if structural {
            continue;
        }
        let prose = plain(paragraph);
        if prose.split_whitespace().count() >= 3 {
            return Some(first_sentence(&prose));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: Source = Source {
        owner: "o",
        repo: "r",
        branch: "main",
        dir: "",
    };

    #[test]
    fn badges_are_skipped_and_the_first_screenshot_is_resolved_against_the_branch() {
        let md = include_str!("../tests/fixtures/readme-badges.md");
        let image = first_image(md, &SOURCE).unwrap();
        assert_eq!(
            image.url,
            "https://raw.githubusercontent.com/o/r/main/docs/images/dashboard.png"
        );
        assert_eq!(image.alt, "The dashboard");
    }

    #[test]
    fn an_html_image_is_found_before_a_later_markdown_one() {
        let md = include_str!("../tests/fixtures/readme-html.md");
        let image = first_image(md, &SOURCE).unwrap();
        assert_eq!(
            image.url,
            "https://raw.githubusercontent.com/o/r/main/assets/shot%20one.jpg"
        );
        assert_eq!(image.alt, "Console session");
    }

    #[test]
    fn absolute_and_github_page_urls() {
        let md = include_str!("../tests/fixtures/readme-absolute.md");
        let all: Vec<String> = images(md)
            .iter()
            .filter_map(|i| resolve(&i.url, &SOURCE))
            .filter(|u| !is_badge(u))
            .collect();
        assert_eq!(
            all,
            [
                "https://raw.githubusercontent.com/o/r/main/docs/a.png",
                "https://user-images.example.com/1/screen.png",
                "https://raw.githubusercontent.com/other/x/dev/img/b.gif",
            ]
        );
        assert_eq!(
            first_image(md, &SOURCE).unwrap().url,
            "https://raw.githubusercontent.com/o/r/main/docs/a.png"
        );
    }

    #[test]
    fn images_inside_code_fences_are_ignored() {
        let md = "```md\n![x](in-code.png)\n```\n\n![real](real.png)\n";
        assert_eq!(first_image(md, &SOURCE).unwrap().alt, "real");
    }

    #[test]
    fn relative_paths_follow_the_readme_folder() {
        let docs = Source {
            dir: "docs",
            ..SOURCE
        };
        let base = "https://raw.githubusercontent.com/o/r/main/";
        assert_eq!(
            resolve("img/a.png", &docs).unwrap(),
            format!("{base}docs/img/a.png")
        );
        assert_eq!(resolve("../a.png", &docs).unwrap(), format!("{base}a.png"));
        assert_eq!(
            resolve("/b.png?raw=true", &docs).unwrap(),
            format!("{base}b.png")
        );
        assert_eq!(resolve("./c.png", &SOURCE).unwrap(), format!("{base}c.png"));
        assert_eq!(resolve("../../escape.png", &SOURCE), None);
        assert_eq!(resolve("data:image/png;base64,AAAA", &SOURCE), None);
        assert_eq!(resolve("mailto:x@y", &SOURCE), None);
    }

    #[test]
    fn badge_detection() {
        assert!(is_badge("https://img.shields.io/badge/x-y-green"));
        assert!(is_badge(
            "https://github.com/o/r/actions/workflows/ci.yml/badge.svg"
        ));
        assert!(is_badge(
            "https://raw.githubusercontent.com/o/r/main/logo.SVG?x=1"
        ));
        assert!(!is_badge(
            "https://raw.githubusercontent.com/o/r/main/shot.png"
        ));
    }

    #[test]
    fn readme_folder_prefers_root_then_github_then_docs() {
        let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(readme_dir(&p(&["README.md", "docs/README.md"])), "");
        assert_eq!(
            readme_dir(&p(&[".github/README.md", "docs/readme.md"])),
            ".github"
        );
        assert_eq!(
            readme_dir(&p(&["docs/Readme.rst", "docs/x/README.md"])),
            "docs"
        );
    }

    #[test]
    fn first_paragraph_skips_headings_badges_and_html() {
        let md = include_str!("../tests/fixtures/readme-badges.md");
        assert_eq!(
            first_paragraph_sentence(md).unwrap(),
            "A control plane for reverse proxies, e.g. Caddy."
        );
        assert_eq!(first_sentence("Ends here. Then more."), "Ends here.");
        assert_eq!(first_sentence("Version 1.2 is out"), "Version 1.2 is out");
        assert_eq!(first_paragraph_sentence("# Only a title\n"), None);
    }
}
