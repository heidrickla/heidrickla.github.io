use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

const API: &str = "https://api.github.com";
const BODY_LIMIT: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Repo {
    pub name: String,
    pub html_url: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub topics: Vec<String>,
    pub default_branch: String,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    #[serde(default)]
    pub stargazers_count: u64,
}

impl Repo {
    /// Only public, original, live repositories are published.
    pub fn is_listed(&self) -> bool {
        !self.private && !self.fork && !self.archived
    }
}

#[derive(Debug, Deserialize)]
struct Tree {
    #[serde(default)]
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

pub struct Response {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// GET with an optional on-disk cache for local runs. The token is sent to api.github.com only.
pub struct Http {
    agent: ureq::Agent,
    token: Option<String>,
    cache: Option<PathBuf>,
    pub api_calls: u32,
}

impl Http {
    pub fn new(token: Option<String>, cache: Option<PathBuf>) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .user_agent("site-gen (heidrickla.github.io)")
            .build()
            .into();
        if let Some(dir) = &cache {
            let _ = std::fs::create_dir_all(dir);
        }
        Http {
            agent,
            token,
            cache,
            api_calls: 0,
        }
    }

    pub fn get(&mut self, url: &str, accept: &str) -> Result<Response, String> {
        let key = cache_key(url, accept);
        if let Some(hit) = self.cache_read(&key) {
            return Ok(hit);
        }
        let is_api = url.starts_with(API);
        let mut request = self.agent.get(url).header("Accept", accept);
        if is_api {
            self.api_calls += 1;
            request = request.header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(token) = &self.token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
        }
        let mut response = request.call().map_err(|e| format!("GET {url}: {e}"))?;
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(String::from);
        let body = if status == 200 {
            response
                .body_mut()
                .with_config()
                .limit(BODY_LIMIT)
                .read_to_vec()
                .map_err(|e| format!("GET {url}: {e}"))?
        } else {
            Vec::new()
        };
        let fetched = Response {
            status,
            content_type,
            body,
        };
        if status == 200 || status == 404 {
            self.cache_write(&key, &fetched);
        }
        Ok(fetched)
    }

    fn cache_read(&self, key: &str) -> Option<Response> {
        let dir = self.cache.as_ref()?;
        let meta = std::fs::read_to_string(dir.join(format!("{key}.meta"))).ok()?;
        let body = std::fs::read(dir.join(format!("{key}.body"))).ok()?;
        let (status, content_type) = meta.split_once('\n')?;
        Some(Response {
            status: status.parse().ok()?,
            content_type: Some(content_type.to_string()).filter(|c| !c.is_empty()),
            body,
        })
    }

    fn cache_write(&self, key: &str, response: &Response) {
        let Some(dir) = &self.cache else { return };
        let meta = format!(
            "{}\n{}",
            response.status,
            response.content_type.as_deref().unwrap_or("")
        );
        let _ = std::fs::write(dir.join(format!("{key}.body")), &response.body);
        let _ = std::fs::write(dir.join(format!("{key}.meta")), meta);
    }

    pub fn list_repos(&mut self, owner: &str) -> Result<Vec<Repo>, String> {
        let mut repos = Vec::new();
        for page in 1.. {
            let url = format!("{API}/users/{owner}/repos?per_page=100&type=owner&page={page}");
            let response = self.get(&url, "application/vnd.github+json")?;
            if response.status != 200 {
                return Err(format!("listing repositories: HTTP {}", response.status));
            }
            let batch: Vec<Repo> =
                serde_json::from_slice(&response.body).map_err(|e| format!("listing: {e}"))?;
            let done = batch.len() < 100;
            repos.extend(batch);
            if done {
                break;
            }
        }
        Ok(repos)
    }

    /// Blob paths of the default branch; empty for an empty repository.
    pub fn tree(&mut self, owner: &str, repo: &Repo) -> Result<Vec<String>, String> {
        let url = format!(
            "{API}/repos/{owner}/{}/git/trees/{}?recursive=1",
            repo.name, repo.default_branch
        );
        let response = self.get(&url, "application/vnd.github+json")?;
        match response.status {
            200 => {}
            404 | 409 => return Ok(Vec::new()),
            s => return Err(format!("tree of {}: HTTP {s}", repo.name)),
        }
        let tree: Tree =
            serde_json::from_slice(&response.body).map_err(|e| format!("tree: {e}"))?;
        if tree.truncated {
            eprintln!("note: the tree of {} is truncated", repo.name);
        }
        Ok(tree
            .tree
            .into_iter()
            .filter(|e| e.kind == "blob")
            .map(|e| e.path)
            .collect())
    }

    pub fn readme(&mut self, owner: &str, repo: &Repo) -> Result<Option<String>, String> {
        let url = format!("{API}/repos/{owner}/{}/readme", repo.name);
        let response = self.get(&url, "application/vnd.github.raw")?;
        match response.status {
            200 => Ok(Some(String::from_utf8_lossy(&response.body).into_owned())),
            404 => Ok(None),
            s => Err(format!("README of {}: HTTP {s}", repo.name)),
        }
    }

    /// A file from the default branch through raw.githubusercontent.com, outside the API limit.
    pub fn raw_text(&mut self, owner: &str, repo: &Repo, path: &str) -> Option<String> {
        let url = raw_url(owner, &repo.name, &repo.default_branch, path);
        let response = self.get(&url, "*/*").ok()?;
        (response.status == 200).then(|| String::from_utf8_lossy(&response.body).into_owned())
    }
}

pub fn raw_url(owner: &str, repo: &str, branch: &str, path: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{}",
        path.replace(' ', "%20")
    )
}

// FNV-1a: a stable file name per request without another dependency.
fn cache_key(url: &str, accept: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in url.bytes().chain([0]).chain(accept.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(private: bool, fork: bool, archived: bool) -> Repo {
        Repo {
            name: "x".into(),
            html_url: "https://github.com/o/x".into(),
            description: None,
            language: None,
            topics: vec![],
            default_branch: "main".into(),
            private,
            fork,
            archived,
            stargazers_count: 0,
        }
    }

    #[test]
    fn only_public_original_unarchived_repositories_are_listed() {
        assert!(repo(false, false, false).is_listed());
        assert!(!repo(true, false, false).is_listed());
        assert!(!repo(false, true, false).is_listed());
        assert!(!repo(false, false, true).is_listed());
    }

    #[test]
    fn the_listing_fixture_parses_and_filters() {
        let repos: Vec<Repo> =
            serde_json::from_str(include_str!("../tests/fixtures/repos.json")).unwrap();
        let listed: Vec<&str> = repos
            .iter()
            .filter(|r| r.is_listed())
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(listed, ["ha-example", "tool"]);
    }

    #[test]
    fn cache_keys_differ_by_accept_header() {
        assert_ne!(cache_key("u", "a"), cache_key("u", "b"));
        assert_eq!(cache_key("u", "a").len(), 16);
    }
}
