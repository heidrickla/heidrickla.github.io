# heidrickla.github.io

The project portfolio at https://heidrickla.github.io, built by `site-gen`, a Rust program that reads the public GitHub repositories of `heidrickla` and writes a static site to `_site/`.

## How it builds

`.github/workflows/pages.yml` runs on every push to `main`, daily at 09:17 UTC and by hand. It runs the tests, runs `site-gen` with the workflow's `GITHUB_TOKEN`, and deploys `_site/` to GitHub Pages.

For each public repository that is not a fork and not archived, `site-gen` reads the git tree and the README (two API calls) and decides:

| Found in the tree | Result |
|---|---|
| `custom_components/<x>/manifest.json` | Home Assistant section, Integration badge |
| root `hacs.json` on an integration | HACS badge |
| `config.yaml` or `config.json` declaring a `slug`, at the root or one folder down | Home Assistant section, Add-on badge |
| anything else | More projects |

| Picture | Source |
|---|---|
| Home Assistant icon | first of `custom_components/*/brand/icon.png`, `brand/icon.png`, `<add-on folder>/icon.png`, `<add-on folder>/logo.png`; otherwise an initial |
| Featured screenshot | `image` in `site.toml`, else the README's first image that is not a badge |

Descriptions come from GitHub; an empty one is replaced by the README's first sentence. Images are downloaded into `_site/img/`.

## site.toml

| Key | Meaning |
|---|---|
| `owner` | GitHub account to read |
| `name`, `eyebrow`, `headline` | header text |
| `summary` | intro line; `{integrations}` and `{addons}` become counted phrases |
| `featured` | `{ repo, image? }` in display order; `image` is a repository path or a URL |
| `[groups]` | tag title to repository names for More projects, in display order |

Ungrouped repositories in More projects are tagged by topic (`firmware`, `reverse-engineering`, `research`) or by language.

## Run locally

```
cargo test
cargo run --release
```

`GITHUB_TOKEN` is used when set; without it the API's unauthenticated limit of 60 calls an hour applies. `SITE_GEN_CACHE=1` keeps every response in `target/api-cache/` and reuses it on the next run.

Preview with any static server, for example:

```
python -m http.server 8000 --directory _site
```
