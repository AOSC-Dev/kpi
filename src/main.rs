use std::{collections::HashMap, env};

use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, level_filters::LevelFilter};
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
    _committer: Option<Author>,
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
    login: String,
    html_url: Option<String>,
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
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

    let mut map = HashMap::new();
    let token = env::var("GITHUB_TOKEN")?;
    let client = Client::builder().user_agent("aosc-kpi").build()?;
    let repos = client
        .get("https://api.github.com/orgs/aosc-dev/repos?per_page=100&sort=pushed")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<Repo>>()
        .await?;

    let mut filter_repos = vec![];

    for i in repos {
        let dt = DateTime::parse_from_rfc3339(&i.pushed_at)?.to_utc();
        if Utc::now() - dt <= Duration::days(31) {
            filter_repos.push(i);
        }
    }

    println!("Modified {} repos this month", filter_repos.len());
    println!("Repos:\n{:#?}", filter_repos);

    let mut filter_author = vec![];

    for i in filter_repos {
        let mut j = 1;
        if i.url.contains("AOSC-Dev/linux") {
            continue;
        }
        'a: loop {
            debug!("Getting repo: {} page: {}", i.url, j);
            let resp = client
                .get(format!("{}/commits?page={}", i.url, j))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .error_for_status();

            let resp = match resp {
                Ok(resp) => resp,
                Err(_) => break,
            };

            let json = resp.json::<Vec<Commit>>().await?;
            if json.is_empty() {
                break;
            }
            for i in json {
                if let Some(commit) = &i.commit {
                    let committer_date = &commit.committer.date;
                    let author_date = &commit.author.date;
                    let committer_dt = DateTime::parse_from_rfc3339(&committer_date)?.to_utc();
                    let author_dt = DateTime::parse_from_rfc3339(&author_date)?.to_utc();
                    if Utc::now() - committer_dt > Duration::days(31)
                        && Utc::now() - author_dt > Duration::days(31)
                    {
                        break 'a;
                    }
                    filter_author.push(i);
                }
            }

            j += 1;
        }
    }

    for i in filter_author {
        if let Some(author) = i.author {
            if let Some(url) = &author.html_url {
                map.insert(author.login.to_string(), url.to_string());
            }
        }
    }

    for (k, v) in map {
        println!("{k}: {v}");
    }

    // dbg!(filter_author);

    Ok(())
}
