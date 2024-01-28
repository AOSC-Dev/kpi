use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use eyre::{bail, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use tracing::{debug, error, info, level_filters::LevelFilter};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[derive(Deserialize, Debug)]
struct Repo {
    url: String,
    pushed_at: String,
}

#[derive(Deserialize, Debug)]
struct Commit {
    commit: Option<RepoCommit>,
    author: Option<Author>,
    committer: Option<Author>,
}

#[derive(Deserialize, Debug)]
struct RepoCommit {
    author: RepoAuthor,
    committer: RepoAuthor,
}

#[derive(Deserialize, Debug)]
struct RepoAuthor {
    date: String,
}

#[derive(Deserialize, Debug)]
struct Author {
    login: Option<String>,
    html_url: Option<String>,
}

#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    /// result output to markdown format
    #[arg(long)]
    to_markdown: bool,
    /// Github token
    #[arg(long, env = "GITHUB_TOKEN")]
    token: String,
    /// Days for query kpi
    #[arg(long)]
    days: u64,
    /// Filter is organization user
    #[arg(long)]
    filter_org_user: bool,
    /// Organization name
    #[arg(long)]
    org: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let env_log = EnvFilter::try_from_default_env();

    if let Ok(filter) = env_log {
        tracing_subscriber::registry()
            .with(fmt::layer().with_filter(filter))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(fmt::layer())
            .with(LevelFilter::INFO)
            .init();
    }
    let args = Args::parse();
    let Args {
        to_markdown,
        token,
        days,
        filter_org_user,
        org,
    } = args;

    let days = days as i64;
    let days_duration = Duration::days(days);

    let mut map = HashMap::new();

    let client = Client::builder().user_agent("aosc-kpi").build()?;

    let repos = get_repos(&client, &token, &org).await?;

    let mut filter_repos = vec![];

    for i in repos {
        let dt = DateTime::parse_from_rfc3339(&i.pushed_at)?.to_utc();
        if Utc::now() - dt <= days_duration {
            filter_repos.push(i);
        }
    }

    info!(
        "A total of {} repos have been modified in the last {} days.",
        filter_repos.len(),
        days
    );

    debug!("Repos: {:?}", filter_repos);

    let mut filter_author = vec![];

    for i in filter_repos {
        let mut page = 1;
        'a: loop {
            info!("Getting repo: {} page: {}", i.url, page);

            let json = match get_commits(&client, &token, &i.url, page).await {
                Ok(json) => json,
                Err(e) => match e.status() {
                    Some(StatusCode::CONFLICT) => {
                        error!("Git Repository is empty: {}", e);
                        break;
                    }
                    _ => bail!("Failed to get commits {}: {e}", i.url),
                },
            };

            if json.is_empty() {
                break;
            }

            for i in json {
                if let Some(commit) = &i.commit {
                    let committer_date = &commit.committer.date;
                    let author_date = &commit.author.date;
                    let committer_dt = DateTime::parse_from_rfc3339(committer_date)?.to_utc();
                    let author_dt = DateTime::parse_from_rfc3339(author_date)?.to_utc();
                    if Utc::now() - committer_dt > days_duration
                        && Utc::now() - author_dt > days_duration
                    {
                        break 'a;
                    }
                    filter_author.push(i);
                }
            }

            page += 1;
        }
    }

    for i in filter_author {
        if let Some(author) = i.author {
            if let Some(url) = &author.html_url {
                if let Some(login) = author.login {
                    map.insert(login.to_string(), url.to_string());
                }
            }
        }

        if let Some(committer) = i.committer {
            if let Some(url) = &committer.html_url {
                if let Some(login) = committer.login {
                    map.insert(login.to_string(), url.to_string());
                }
            }
        }
    }

    for (k, v) in map {
        if filter_org_user && !is_org_user(&client, &k, &token).await? {
            continue;
        }

        if to_markdown {
            println!("- [{}]({})", k, v);
        } else {
            println!("{k}: {v}");
        }
    }

    Ok(())
}

async fn get_repos(client: &Client, token: &str, org: &str) -> Result<Vec<Repo>> {
    Ok(client
        .get(format!(
            "https://api.github.com/orgs/{org}/repos?per_page=100&sort=pushed"
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Repo>>()
        .await?)
}

async fn get_commits(
    client: &Client,
    token: &str,
    repo_api_url: &str,
    page: u64,
) -> std::result::Result<Vec<Commit>, reqwest::Error> {
    client
        .get(format!(
            "{}/commits?page={}&per_page=100",
            repo_api_url, page
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Commit>>()
        .await
}

async fn is_org_user(client: &Client, user: &str, token: &str) -> Result<bool> {
    let resp = client
        .get(format!(
            "https://api.github.com/orgs/aosc-dev/memberships/{}",
            user
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .and_then(|x| x.error_for_status());

    match resp {
        Ok(_) => Ok(true),
        Err(e) => match e.status() {
            Some(StatusCode::NOT_FOUND) => Ok(false),
            _ => bail!("Network is not reachable: {e}"),
        },
    }
}
