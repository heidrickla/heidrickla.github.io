mod classify;
mod config;
mod github;
mod readme;
mod render;

use classify::{Classification, Kind};
use config::SiteConfig;
use github::{Http, Repo};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const OUT_DIR: &str = "_site";
const CACHE_DIR: &str = "target/api-cache";
const STYLE: &str = include_str!("../assets/style.css");
const FAVICON: &str = include_str!("../assets/favicon.svg");

/// Topics that tag an ungrouped repository when site.toml does not.
const TOPIC_TAGS: [(&str, &str); 4] = [
    ("firmware", "Firmware"),
    ("reverse-engineering", "Reverse engineering"),
    ("research", "Research"),
    ("security-research", "Research"),
];

struct Project {
    repo: Repo,
    class: Classification,
    desc: String,
    shot: Option<readme::Image>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("site-gen: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "site.toml".into());
    let text =
        std::fs::read_to_string(&config_path).map_err(|e| format!("reading {config_path}: {e}"))?;
    let config = SiteConfig::parse(&text).map_err(|e| format!("{config_path}: {e}"))?;

    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty());
    let cache =
        (std::env::var("SITE_GEN_CACHE").as_deref() == Ok("1")).then(|| PathBuf::from(CACHE_DIR));
    eprintln!(
        "site-gen: {} GitHub API, cache {}",
        if token.is_some() {
            "authenticated"
        } else {
            "unauthenticated"
        },
        if cache.is_some() { CACHE_DIR } else { "off" }
    );
    let mut http = Http::new(token, cache);

    let repos: Vec<Repo> = http
        .list_repos(&config.owner)?
        .into_iter()
        .filter(Repo::is_listed)
        .collect();
    let mut projects = Vec::with_capacity(repos.len());
    for repo in repos {
        let project = inspect(&mut http, &config.owner, repo)?;
        eprintln!(
            "  {:<40} {:<12} icon={} shot={}",
            project.repo.name,
            project.class.kind.label().unwrap_or("-"),
            project.class.icon.as_deref().unwrap_or("-"),
            project.shot.as_ref().map_or("-", |s| s.url.as_str())
        );
        projects.push(project);
    }

    let out = Path::new(OUT_DIR);
    clear_dir(out).map_err(|e| format!("clearing {OUT_DIR}: {e}"))?;
    std::fs::create_dir_all(out.join("img")).map_err(|e| format!("creating {OUT_DIR}: {e}"))?;

    let page = build_page(&mut http, &config, &projects, out);
    let html = render::page(&page);
    let write = |name: &str, bytes: &[u8]| {
        std::fs::write(out.join(name), bytes).map_err(|e| format!("writing {name}: {e}"))
    };
    write("index.html", html.as_bytes())?;
    write("style.css", STYLE.as_bytes())?;
    write("favicon.svg", FAVICON.as_bytes())?;
    write(".nojekyll", b"")?;
    eprintln!(
        "site-gen: {} repositories ({} featured, {} Home Assistant, {} more), {} API calls, wrote {OUT_DIR}/",
        projects.len(),
        page.featured.len(),
        page.home_assistant.len(),
        page.more.len(),
        http.api_calls
    );
    Ok(())
}

fn inspect(http: &mut Http, owner: &str, repo: Repo) -> Result<Project, String> {
    let paths = http.tree(owner, &repo)?;
    let readme_text = http.readme(owner, &repo)?;
    let slug_configs: Vec<String> = classify::addon_config_candidates(&paths)
        .into_iter()
        .filter(|p| {
            http.raw_text(owner, &repo, p)
                .is_some_and(|content| classify::declares_slug(p, &content))
        })
        .collect();
    let class = classify::classify(&paths, &slug_configs);
    let source = readme::Source {
        owner,
        repo: &repo.name,
        branch: &repo.default_branch,
        dir: readme::readme_dir(&paths),
    };
    let shot = readme_text
        .as_deref()
        .and_then(|md| readme::first_image(md, &source));
    let desc = repo
        .description
        .clone()
        .map(|d| d.trim().to_string())
        .filter(|d| !d.is_empty())
        .or_else(|| {
            readme_text
                .as_deref()
                .and_then(readme::first_paragraph_sentence)
        })
        .unwrap_or_default();
    Ok(Project {
        repo,
        class,
        desc,
        shot,
    })
}

fn build_page(
    http: &mut Http,
    config: &SiteConfig,
    projects: &[Project],
    out: &Path,
) -> render::Page {
    let find = |name: &str| {
        projects
            .iter()
            .find(|p| p.repo.name.eq_ignore_ascii_case(name))
    };

    let mut featured_names = Vec::new();
    let mut featured = Vec::new();
    for entry in &config.featured {
        let Some(project) = find(&entry.repo) else {
            eprintln!(
                "note: featured {} is not a listed public repository",
                entry.repo
            );
            continue;
        };
        featured_names.push(project.repo.name.clone());
        let shot = match &entry.image {
            Some(path) => {
                let source = readme::Source {
                    owner: &config.owner,
                    repo: &project.repo.name,
                    branch: &project.repo.default_branch,
                    dir: "",
                };
                readme::resolve(path, &source).map(|url| readme::Image {
                    url,
                    alt: format!("{} screenshot", project.repo.name),
                })
            }
            None => project.shot.clone(),
        };
        let image = shot.and_then(|shot| {
            let src = download_image(
                http,
                &shot.url,
                out,
                &format!("{}-shot", file_stem(&project.repo.name)),
            )?;
            let alt = if shot.alt.is_empty() {
                format!("{} screenshot", project.repo.name)
            } else {
                shot.alt
            };
            Some(render::Image { src, alt })
        });
        featured.push(render::Card {
            name: project.repo.name.clone(),
            url: project.repo.html_url.clone(),
            desc: project.desc.clone(),
            tag: project
                .class
                .kind
                .label()
                .map(String::from)
                .or_else(|| project.repo.language.clone()),
            badges: Vec::new(),
            image,
        });
    }
    let is_featured = |p: &&Project| featured_names.contains(&p.repo.name);

    let mut home_assistant: Vec<&Project> = projects
        .iter()
        .filter(|p| p.class.kind != Kind::Other)
        .filter(|p| !is_featured(p))
        .collect();
    home_assistant.sort_by(|a, b| {
        let addon = |p: &Project| p.class.kind == Kind::Addon;
        (
            addon(a),
            std::cmp::Reverse(a.repo.stargazers_count),
            a.repo.name.to_lowercase(),
        )
            .cmp(&(
                addon(b),
                std::cmp::Reverse(b.repo.stargazers_count),
                b.repo.name.to_lowercase(),
            ))
    });
    let home_assistant_cards = home_assistant
        .into_iter()
        .map(|p| {
            let mut badges: Vec<String> =
                p.class.kind.label().map(String::from).into_iter().collect();
            if p.class.kind == (Kind::Integration { hacs: true }) {
                badges.push("HACS".into());
            }
            let image = p.class.icon.as_ref().and_then(|path| {
                let url =
                    github::raw_url(&config.owner, &p.repo.name, &p.repo.default_branch, path);
                let src = download_image(
                    http,
                    &url,
                    out,
                    &format!("{}-icon", file_stem(&p.repo.name)),
                )?;
                Some(render::Image {
                    src,
                    alt: String::new(),
                })
            });
            render::Card {
                name: p.repo.name.clone(),
                url: p.repo.html_url.clone(),
                desc: readme::first_sentence(&p.desc),
                tag: None,
                badges,
                image,
            }
        })
        .collect();

    let mut more: Vec<(usize, String, &Project)> = projects
        .iter()
        .filter(|p| p.class.kind == Kind::Other)
        .filter(|p| !is_featured(p))
        .map(|p| {
            let (order, tag) = config
                .group_of(&p.repo.name)
                .or_else(|| {
                    TOPIC_TAGS
                        .iter()
                        .find(|(topic, _)| p.repo.topics.iter().any(|t| t == topic))
                        .map(|(_, tag)| (usize::MAX - 1, tag.to_string()))
                })
                .or_else(|| p.repo.language.clone().map(|l| (usize::MAX, l)))
                .unwrap_or((usize::MAX, String::new()));
            (order, tag, p)
        })
        .collect();
    more.sort_by_key(|(order, _, p)| (*order, p.repo.name.to_lowercase()));
    let more_cards = more
        .into_iter()
        .map(|(_, tag, p)| render::Card {
            name: p.repo.name.clone(),
            url: p.repo.html_url.clone(),
            desc: readme::first_sentence(&p.desc),
            tag: Some(tag).filter(|t| !t.is_empty()),
            badges: Vec::new(),
            image: None,
        })
        .collect();

    let integrations = projects
        .iter()
        .filter(|p| matches!(p.class.kind, Kind::Integration { .. }))
        .count();
    let addons = projects
        .iter()
        .filter(|p| p.class.kind == Kind::Addon)
        .count();
    let summary = config
        .summary
        .replace(
            "{integrations}",
            &render::counted(
                integrations,
                "Home Assistant integration",
                "Home Assistant integrations",
            ),
        )
        .replace("{addons}", &render::counted(addons, "add-on", "add-ons"));

    render::Page {
        owner: config.owner.clone(),
        name: config.name.clone(),
        eyebrow: config.eyebrow.clone(),
        headline: config.headline.clone(),
        summary: render::capitalise(&summary),
        featured,
        home_assistant: home_assistant_cards,
        more: more_cards,
        built: utc_now(),
    }
}

/// Empties the output folder but keeps it, so a local server running in it keeps working.
fn clear_dir(dir: &Path) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

fn file_stem(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// The image type from its leading bytes; anything else (SVG, HTML error pages) is refused.
fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.len() > 12 && &bytes[4..12] == b"ftypavif" {
        Some("avif")
    } else {
        None
    }
}

/// Saves an image under `img/` and returns its site path, or None with a note.
fn download_image(http: &mut Http, url: &str, out: &Path, stem: &str) -> Option<String> {
    let response = match http.get(url, "image/*") {
        Ok(r) if r.status == 200 => r,
        Ok(r) => {
            eprintln!("note: image {url}: HTTP {}", r.status);
            return None;
        }
        Err(e) => {
            eprintln!("note: {e}");
            return None;
        }
    };
    let Some(ext) = image_extension(&response.body) else {
        eprintln!("note: {url} is not a PNG, JPEG, GIF, WebP or AVIF image");
        return None;
    };
    let name = format!("img/{stem}.{ext}");
    match std::fs::write(out.join(&name), &response.body) {
        Ok(()) => Some(name),
        Err(e) => {
            eprintln!("note: writing {name}: {e}");
            None
        }
    }
}

/// "YYYY-MM-DD HH:MM" in UTC from the system clock.
fn utc_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_utc(secs)
}

fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's days-to-civil conversion.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rem / 3_600,
        rem % 3_600 / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formatting() {
        assert_eq!(format_utc(0), "1970-01-01 00:00");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00");
        assert_eq!(format_utc(1_793_276_100), "2026-10-29 12:15");
    }

    #[test]
    fn images_are_typed_by_content() {
        assert_eq!(image_extension(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(image_extension(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_extension(b"GIF89a..."), Some("gif"));
        assert_eq!(image_extension(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_extension(b"<svg xmlns="), None);
        assert_eq!(image_extension(b"<!doctype html>"), None);
    }

    #[test]
    fn file_stems_are_safe() {
        assert_eq!(file_stem("SSLReverseProxy"), "sslreverseproxy");
        assert_eq!(file_stem("../a b"), "---a-b");
    }
}
