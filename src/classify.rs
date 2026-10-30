/// Most add-on config files fetched per repository; real add-on repositories hold a handful.
const MAX_CONFIG_CANDIDATES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Integration { hacs: bool },
    Addon,
    Other,
}

impl Kind {
    pub fn label(&self) -> Option<&'static str> {
        match self {
            Kind::Integration { .. } => Some("Integration"),
            Kind::Addon => Some("Add-on"),
            Kind::Other => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub kind: Kind,
    /// Repository path of the icon, for Home Assistant repositories only.
    pub icon: Option<String>,
}

fn is_addon_config_name(name: &str) -> bool {
    name == "config.yaml" || name == "config.json"
}

/// Paths whose contents decide whether this is an add-on repository: `config.yaml` or
/// `config.json` at the root or one folder down, root first.
pub fn addon_config_candidates(paths: &[String]) -> Vec<String> {
    let mut found: Vec<&String> = paths
        .iter()
        .filter(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            match parts.as_slice() {
                [name] => is_addon_config_name(name),
                [dir, name] => !dir.starts_with('.') && is_addon_config_name(name),
                _ => false,
            }
        })
        .collect();
    found.sort_by_key(|p| (p.contains('/'), p.as_str()));
    found
        .into_iter()
        .take(MAX_CONFIG_CANDIDATES)
        .cloned()
        .collect()
}

/// True when an add-on config file declares a non-empty `slug`.
pub fn declares_slug(path: &str, content: &str) -> bool {
    if path.ends_with(".json") {
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(content);
        return parsed.ok().is_some_and(|v| {
            v.get("slug")
                .and_then(|s| s.as_str())
                .is_some_and(|s| !s.trim().is_empty())
        });
    }
    content.lines().any(|line| {
        line.strip_prefix("slug:").is_some_and(|value| {
            let value = value.split('#').next().unwrap_or("").trim();
            !value.trim_matches(['"', '\'']).is_empty()
        })
    })
}

fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Classifies a repository from its tree paths and the config files found to declare a slug.
pub fn classify(paths: &[String], slug_configs: &[String]) -> Classification {
    let has = |p: &str| paths.iter().any(|x| x == p);
    let integration_dirs: Vec<&str> = paths
        .iter()
        .filter_map(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            match parts.as_slice() {
                ["custom_components", domain, "manifest.json"] => Some(*domain),
                _ => None,
            }
        })
        .collect();
    let addon_dir = slug_configs.first().map(|p| parent(p));

    let kind = if !integration_dirs.is_empty() {
        Kind::Integration {
            hacs: has("hacs.json"),
        }
    } else if addon_dir.is_some() {
        Kind::Addon
    } else {
        Kind::Other
    };
    if kind == Kind::Other {
        return Classification { kind, icon: None };
    }

    let mut order: Vec<String> = integration_dirs
        .iter()
        .map(|d| format!("custom_components/{d}/brand/icon.png"))
        .collect();
    order.push("brand/icon.png".into());
    if let Some(dir) = addon_dir {
        order.push(join(dir, "icon.png"));
        order.push(join(dir, "logo.png"));
    }
    let icon = order.into_iter().find(|p| has(p));
    Classification { kind, icon }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        paths: Vec<String>,
        #[serde(default)]
        files: std::collections::BTreeMap<String, String>,
        kind: String,
        hacs: Option<bool>,
        icon: Option<String>,
    }

    fn run(case: &Case) -> Classification {
        let slug_configs: Vec<String> = addon_config_candidates(&case.paths)
            .into_iter()
            .filter(|p| case.files.get(p).is_some_and(|c| declares_slug(p, c)))
            .collect();
        classify(&case.paths, &slug_configs)
    }

    #[test]
    fn fixture_trees_classify_as_recorded() {
        let cases: Vec<Case> =
            serde_json::from_str(include_str!("../tests/fixtures/trees.json")).unwrap();
        assert!(cases.len() >= 6);
        for case in &cases {
            let got = run(case);
            let kind = match got.kind {
                Kind::Integration { .. } => "integration",
                Kind::Addon => "addon",
                Kind::Other => "other",
            };
            assert_eq!(kind, case.kind, "{}", case.name);
            if let Some(hacs) = case.hacs {
                assert_eq!(got.kind, Kind::Integration { hacs }, "{}", case.name);
            }
            assert_eq!(got.icon, case.icon, "{}", case.name);
        }
    }

    #[test]
    fn slug_detection_reads_yaml_and_json() {
        assert!(declares_slug("a/config.yaml", "name: X\nslug: x_y\n"));
        assert!(declares_slug("a/config.yaml", "slug: \"x\"  # comment\n"));
        assert!(!declares_slug("a/config.yaml", "name: X\n  slug: nested\n"));
        assert!(!declares_slug("a/config.yaml", "slug: ''\n"));
        assert!(declares_slug("config.json", r#"{"name":"X","slug":"x"}"#));
        assert!(!declares_slug("config.json", r#"{"name":"X"}"#));
        assert!(!declares_slug("config.json", "not json"));
    }

    #[test]
    fn candidates_are_root_first_and_skip_hidden_and_deep_folders() {
        let paths: Vec<String> = [
            "b/config.yaml",
            ".github/config.yaml",
            "config.json",
            "a/b/config.yaml",
            "a/config.json",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            addon_config_candidates(&paths),
            ["config.json", "a/config.json", "b/config.yaml"]
        );
    }
}
