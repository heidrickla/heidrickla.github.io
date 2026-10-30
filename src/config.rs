use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    pub owner: String,
    pub name: String,
    pub eyebrow: String,
    pub headline: String,
    pub summary: String,
    #[serde(default)]
    pub featured: Vec<Featured>,
    // preserve_order keeps the table in file order, which is the display order.
    #[serde(default)]
    pub groups: toml::Table,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Featured {
    pub repo: String,
    pub image: Option<String>,
}

impl SiteConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: SiteConfig = toml::from_str(text).map_err(|e| e.to_string())?;
        for (title, value) in &config.groups {
            let ok = value
                .as_array()
                .is_some_and(|a| a.iter().all(|v| v.is_str()));
            if !ok {
                return Err(format!(
                    "group {title:?} must be a list of repository names"
                ));
            }
        }
        Ok(config)
    }

    /// Group titles in file order with their repository names.
    pub fn groups(&self) -> Vec<(String, Vec<String>)> {
        self.groups
            .iter()
            .map(|(title, value)| {
                let names = value
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                (title.clone(), names)
            })
            .collect()
    }

    pub fn group_of(&self, repo: &str) -> Option<(usize, String)> {
        self.groups()
            .into_iter()
            .enumerate()
            .find(|(_, (_, names))| names.iter().any(|n| n.eq_ignore_ascii_case(repo)))
            .map(|(i, (title, _))| (i, title))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_site_toml_parses_with_groups_in_file_order() {
        let config = SiteConfig::parse(include_str!("../site.toml")).unwrap();
        assert_eq!(config.featured.len(), 4);
        assert_eq!(config.featured[0].repo, "SSLReverseProxy");
        let titles: Vec<String> = config.groups().into_iter().map(|(t, _)| t).collect();
        assert_eq!(titles, ["Firmware", "Reverse engineering", "Research"]);
        assert_eq!(
            config.group_of("glkvm-firmware"),
            Some((0, "Firmware".into()))
        );
        assert_eq!(config.group_of("crossword"), None);
    }

    #[test]
    fn a_group_that_is_not_a_list_of_names_is_refused() {
        let text = "owner='a'\nname='a'\neyebrow='a'\nheadline='a'\nsummary='a'\n[groups]\nX = 3\n";
        assert!(SiteConfig::parse(text).is_err());
    }
}
