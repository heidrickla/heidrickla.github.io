use std::fmt::Write as _;

/// Escapes text for HTML element content and quoted attribute values.
pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// "one" to "twenty" in words, larger counts in digits.
pub fn number_word(n: usize) -> String {
    const WORDS: [&str; 21] = [
        "no",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    WORDS
        .get(n)
        .map_or_else(|| n.to_string(), |w| w.to_string())
}

pub fn counted(n: usize, singular: &str, plural: &str) -> String {
    format!(
        "{} {}",
        number_word(n),
        if n == 1 { singular } else { plural }
    )
}

/// Upper-cases the first letter, for a sentence that starts with a counted phrase.
pub fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[derive(Clone)]
pub struct Image {
    pub src: String,
    pub alt: String,
}

#[derive(Clone)]
pub struct Card {
    pub name: String,
    pub url: String,
    pub desc: String,
    /// Small label: language, group or Home Assistant kind.
    pub tag: Option<String>,
    /// Home Assistant cards: kind and HACS badges.
    pub badges: Vec<String>,
    pub image: Option<Image>,
}

pub struct Page {
    pub owner: String,
    pub name: String,
    pub eyebrow: String,
    pub headline: String,
    pub summary: String,
    pub featured: Vec<Card>,
    pub home_assistant: Vec<Card>,
    pub more: Vec<Card>,
    pub built: String,
}

fn initial(name: &str) -> String {
    let letter = name
        .trim_start_matches("ha-")
        .chars()
        .find(|c| c.is_alphanumeric())
        .unwrap_or('?');
    esc(&letter.to_uppercase().to_string())
}

fn featured_card(out: &mut String, card: &Card) {
    let _ = write!(
        out,
        "<a class=\"card feature\" href=\"{}\">",
        esc(&card.url)
    );
    match &card.image {
        Some(image) => {
            let _ = write!(
                out,
                "<span class=\"shot\"><img src=\"{}\" alt=\"{}\"></span>",
                esc(&image.src),
                esc(&image.alt)
            );
        }
        None => {
            let _ = write!(
                out,
                "<span class=\"shot shot-empty\" aria-hidden=\"true\">{}</span>",
                initial(&card.name)
            );
        }
    }
    let _ = write!(
        out,
        "<span class=\"feature-body\"><span class=\"feature-head\"><span class=\"name\">{}</span>",
        esc(&card.name)
    );
    if let Some(tag) = &card.tag {
        let _ = write!(out, "<span class=\"tag tag-accent\">{}</span>", esc(tag));
    }
    let _ = writeln!(
        out,
        "</span><span class=\"desc\">{}</span></span></a>",
        esc(&card.desc)
    );
}

fn home_assistant_card(out: &mut String, card: &Card) {
    let _ = write!(out, "<a class=\"card ha\" href=\"{}\">", esc(&card.url));
    match &card.image {
        Some(icon) => {
            let _ = write!(
                out,
                "<img class=\"icon\" src=\"{}\" alt=\"\" width=\"40\" height=\"40\">",
                esc(&icon.src)
            );
        }
        None => {
            let _ = write!(
                out,
                "<span class=\"icon icon-initial\" aria-hidden=\"true\">{}</span>",
                initial(&card.name)
            );
        }
    }
    let _ = write!(
        out,
        "<span class=\"ha-body\"><span class=\"ha-head\"><span class=\"name\">{}</span>",
        esc(&card.name)
    );
    for badge in &card.badges {
        let _ = write!(out, "<span class=\"badge\">{}</span>", esc(badge));
    }
    let _ = writeln!(
        out,
        "</span><span class=\"desc\">{}</span></span></a>",
        esc(&card.desc)
    );
}

fn more_card(out: &mut String, card: &Card) {
    let _ = write!(
        out,
        "<a class=\"card more\" href=\"{}\"><span class=\"more-head\"><span class=\"name\">{}</span>",
        esc(&card.url),
        esc(&card.name)
    );
    if let Some(tag) = &card.tag {
        let _ = write!(out, "<span class=\"tag\">{}</span>", esc(tag));
    }
    let _ = writeln!(
        out,
        "</span><span class=\"desc\">{}</span></a>",
        esc(&card.desc)
    );
}

const THEME_BOOT: &str = r#"<script>try{var t=localStorage.getItem("theme");if(t==="light"||t==="dark")document.documentElement.setAttribute("data-theme",t)}catch(e){}</script>"#;

const THEME_TOGGLE: &str = r#"<script>
(function(){
  var root=document.documentElement,button=document.getElementById("theme-toggle");
  function show(){var dark=root.getAttribute("data-theme")!=="light";button.textContent=dark?"Light theme":"Dark theme";button.setAttribute("aria-pressed",dark?"false":"true");}
  button.addEventListener("click",function(){
    var next=root.getAttribute("data-theme")==="light"?"dark":"light";
    root.setAttribute("data-theme",next);
    try{localStorage.setItem("theme",next)}catch(e){}
    show();
  });
  show();
})();
</script>"#;

pub fn page(page: &Page) -> String {
    let profile = format!("https://github.com/{}", page.owner);
    let mut out = String::with_capacity(64 * 1024);
    let _ = writeln!(
        out,
        "<!doctype html>\n<html lang=\"en\" data-theme=\"dark\">\n<head>\n<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<title>{name}, projects</title>\n<meta name=\"description\" content=\"{summary}\">\n\
<meta name=\"color-scheme\" content=\"dark light\">\n{THEME_BOOT}\n\
<link rel=\"icon\" href=\"favicon.svg\" type=\"image/svg+xml\">\n\
<link rel=\"preconnect\" href=\"https://fonts.googleapis.com\">\n\
<link rel=\"preconnect\" href=\"https://fonts.gstatic.com\" crossorigin>\n\
<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=Manrope:wght@400;500;600;700&amp;display=swap\">\n\
<link rel=\"stylesheet\" href=\"style.css\">\n</head>\n<body>\n<div class=\"page\">",
        name = esc(&page.name),
        summary = esc(&page.summary),
    );

    let _ = write!(
        out,
        "<nav class=\"nav\" aria-label=\"Site\"><span class=\"brand\">{}</span><span class=\"nav-links\">",
        esc(&page.name)
    );
    let sections = [
        ("featured", "Featured", !page.featured.is_empty()),
        (
            "integrations",
            "Home Assistant",
            !page.home_assistant.is_empty(),
        ),
        ("more", "More", !page.more.is_empty()),
    ];
    for (id, label, shown) in sections {
        if shown {
            let _ = write!(out, "<a class=\"nav-link\" href=\"#{id}\">{label}</a>");
        }
    }
    let _ = writeln!(
        out,
        "<a class=\"nav-link nav-outline\" href=\"{}\">GitHub</a>\
<button type=\"button\" class=\"nav-link nav-outline theme-toggle\" id=\"theme-toggle\" aria-pressed=\"false\">Light theme</button>\
</span></nav>",
        esc(&profile)
    );

    let _ = writeln!(
        out,
        "<header class=\"intro\"><p class=\"eyebrow\">{}</p><h1>{}</h1><p class=\"lead\">{}</p></header>\n<main>",
        esc(&page.eyebrow),
        esc(&page.headline),
        esc(&page.summary)
    );

    if !page.featured.is_empty() {
        out.push_str("<section id=\"featured\" class=\"section\" aria-labelledby=\"featured-title\">\n<h2 id=\"featured-title\" class=\"label\">Featured</h2>\n<div class=\"grid grid-featured\">\n");
        for card in &page.featured {
            featured_card(&mut out, card);
        }
        out.push_str("</div>\n</section>\n");
    }
    if !page.home_assistant.is_empty() {
        out.push_str("<section id=\"integrations\" class=\"section\" aria-labelledby=\"ha-title\">\n<div class=\"section-head\"><h2 id=\"ha-title\">Home Assistant</h2><span class=\"note\">Integrations install through HACS; add-ons through the add-on store</span></div>\n<div class=\"grid\">\n");
        for card in &page.home_assistant {
            home_assistant_card(&mut out, card);
        }
        out.push_str("</div>\n</section>\n");
    }
    if !page.more.is_empty() {
        out.push_str("<section id=\"more\" class=\"section\" aria-labelledby=\"more-title\">\n<h2 id=\"more-title\">More projects</h2>\n<div class=\"grid\">\n");
        for card in &page.more {
            more_card(&mut out, card);
        }
        out.push_str("</div>\n</section>\n");
    }

    let _ = writeln!(
        out,
        "</main>\n<footer class=\"footer\"><span>Rebuilt every day from public repositories. Last built {} UTC.</span>\
<a href=\"{}\">github.com/{}</a></footer>\n</div>\n{THEME_TOGGLE}\n</body>\n</html>",
        esc(&page.built),
        esc(&profile),
        esc(&page.owner)
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_covers_markup_and_both_quotes() {
        assert_eq!(
            esc(r#"<a href="x">Tom & 'Jerry'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;Tom &amp; &#39;Jerry&#39;&lt;/a&gt;"
        );
        assert_eq!(esc("plain"), "plain");
    }

    #[test]
    fn values_from_github_are_escaped_in_the_page() {
        let hostile = Card {
            name: "<script>alert(1)</script>".into(),
            url: "https://github.com/o/r\"onmouseover=\"x".into(),
            desc: "A & B <b>bold</b>".into(),
            tag: Some("C<#>".into()),
            badges: vec!["<i>".into()],
            image: Some(Image {
                src: "img/a\".png".into(),
                alt: "alt\" onerror=\"x".into(),
            }),
        };
        let more = Card {
            image: None,
            ..hostile.clone()
        };
        let ha = hostile.clone();
        let html = page(&Page {
            owner: "o".into(),
            name: "N".into(),
            eyebrow: "E".into(),
            headline: "H".into(),
            summary: "S".into(),
            featured: vec![hostile],
            home_assistant: vec![ha],
            more: vec![more],
            built: "2026-10-29 00:00".into(),
        });
        assert!(!html.contains("<script>alert"));
        assert!(!html.contains("<b>bold"));
        assert!(!html.contains("<i>"));
        assert!(!html.contains("\"onmouseover"));
        assert!(!html.contains("\" onerror"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(html.contains("A &amp; B &lt;b&gt;bold&lt;/b&gt;"));
    }

    #[test]
    fn counts_read_as_words() {
        assert_eq!(
            counted(10, "integration", "integrations"),
            "ten integrations"
        );
        assert_eq!(counted(1, "add-on", "add-ons"), "one add-on");
        assert_eq!(counted(42, "x", "xs"), "42 xs");
        assert_eq!(capitalise("ten things"), "Ten things");
    }
}
