use std::{
    fmt::{Display, Formatter},
    path::PathBuf,
};

use clap::Parser;
use git2::Repository;
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Parser, Debug)]
struct Cli {
    repo_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let repository = if let Some(repo_url) = cli.repo_url {
        repo_url
    } else {
        get_git_repo()?
        // "shuttle-hq/zero-to-production-newsletter-api".to_string()
    };

    let repo = GreptileRepository {
        remote: "github".to_string(),
        branch: "main".to_string(),
        repository,
    };

    let repo_id = repo.as_repo_id();

    let greptile = GreptileClient::from_env()?;

    // let req: GreptileIndexRequest = repo.clone().into();

    // greptile.index_repo(req).await?;

    let query = PROMPT.to_string();

    let req = GreptileQueryRequest::new(repo, GreptileMessage::user(query));

    let response = greptile.query_repo(req).await?;

    termimad::print_text(&response);

    Ok(())
}

fn get_git_repo() -> Result<String, Box<dyn std::error::Error>> {
    let repository = Repository::open(".")?;

    let remote = repository.find_remote("origin")?;

    let Some(repo_url) = remote.url() else {
        return Err("Could not find remote URL for origin remote".into());
    };

    let regex = Regex::new(r#"https?:\/\/(?:www\.)?github\.com\/([\w.-]+\/[\w.-]+)\.git"#)?;

    let caps = regex.captures(&repo_url).unwrap();

    Ok(caps.get(0).unwrap().as_str().to_string())
}

struct GreptileClient {
    client: reqwest::Client,
    github_token: String,
    greptile_api_token: String,
}

impl GreptileClient {
    fn new(github_token: String, greptile_api_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            github_token,
            greptile_api_token,
        }
    }

    fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let github_token = std::env::var("GITHUB_ACCESS_TOKEN")?;
        let greptile_api_token = std::env::var("GREPTILE_API_TOKEN")?;

        Ok(Self {
            client: reqwest::Client::new(),
            github_token,
            greptile_api_token,
        })
    }

    async fn check_repo_exists(&self, repo_id: String) -> Result<bool, Box<dyn std::error::Error>> {
        let url = format!("https://api.greptile.com/v2/repositories/github%253Amain%253Ashuttle-hq%252Fzero-to-production-newsletter-api");
        let res = self
            .client
            .get(&url)
            .bearer_auth(&self.greptile_api_token)
            .send()
            .await?;

        if res.status() != 200 {
            println!("Repo does not exist");
            return Ok(false);
        } else {
            return Ok(true);
        }
    }

    async fn index_repo(
        &self,
        req: GreptileIndexRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let response = self
            .client
            .post("https://api.greptile.com/v2/repositories")
            .bearer_auth(&self.greptile_api_token)
            .header("Content-Type", "application/json")
            .header("X-Github-Token", &self.github_token)
            .json(&req)
            .send()
            .await?;

        if response.status() != 200 {
            Err(response.text().await.unwrap().into())
        } else {
            Ok(())
        }
    }

    async fn query_repo(
        &self,
        req: GreptileQueryRequest,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut response = self
            .client
            .post("https://api.greptile.com/v2/query")
            .bearer_auth(&self.greptile_api_token)
            .header("Content-Type", "application/json")
            .header("X-Github-Token", &self.github_token)
            .json(&req);

        let mut response = response.send().await?;

        let response_body = response.text().await?;

        Ok(response_body)
    }
}
#[derive(Serialize)]
struct GreptileIndexRequest {
    remote: String,
    repository: String,
    branch: String,
    reload: bool,
    notify: bool,
}

impl From<GreptileRepository> for GreptileIndexRequest {
    fn from(value: GreptileRepository) -> Self {
        Self {
            remote: value.remote,
            repository: value.repository,
            branch: value.branch,
            reload: true,
            notify: true,
        }
    }
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GreptileQueryRequest {
    messages: Vec<GreptileMessage>,
    repositories: Vec<GreptileRepository>,
    session_id: String,
    stream: bool,
    genius: bool,
}

impl GreptileQueryRequest {
    fn new(repository: GreptileRepository, message: GreptileMessage) -> Self {
        Self {
            messages: vec![message],
            repositories: vec![repository],
            session_id: String::new(),
            stream: false,
            genius: false,
        }
    }

    fn with_messages(repository: GreptileRepository, messages: Vec<GreptileMessage>) -> Self {
        Self {
            messages,
            repositories: vec![repository],
            session_id: String::new(),
            stream: false,
            genius: false,
        }
    }

    fn push_message(mut self, message: GreptileMessage) -> Self {
        self.messages.push(message);

        self
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GreptileMessage {
    id: String,
    content: String,
    role: Role,
}

impl GreptileMessage {
    fn user(content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            role: Role::User,
        }
    }

    fn system(content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            role: Role::System,
        }
    }

    fn assistant(content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            content,
            role: Role::Assistant,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
enum Role {
    User,
    System,
    Assistant,
}

impl Display for Role {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::System => write!(f, "system"),
            Self::Assistant => write!(f, "assistant"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct GreptileRepository {
    remote: String,
    branch: String,
    repository: String,
}

impl GreptileRepository {
    fn new(remote: String, branch: String, repository: String) -> Self {
        Self {
            remote,
            branch,
            repository,
        }
    }

    fn as_repo_id(&self) -> String {
        format!("{}:{}:{}", self.remote, self.branch, self.repository)
    }
}

const PROMPT: &str = r#"Use this as a template to summarise the project and convert it to run on the Shuttle Rust framework and platform:

**Supported Shuttle Resources:**

Refer to the following list when evaluating the project's resource compatibility:

Shared Databases

Type: PostgreSQL

- Feature Flag: shuttle-shared-db::Postgres

Supported Output Types:

- sqlx::Pool (async connection via SQLx)
- Custom connection strings via Secrets.toml

Important Notes:

- Databases are isolated within a shared cluster
- Credentials must be managed through Secrets.toml

```json
{
  "resources": [
    {
      "type": "database",
      "flavour": "postgres",
      "schema": "src/schema.sql",
      "supported": true
    },
    {
      "type": "cache",
      "flavour": "redis",
      "supported": false
    }
  ],
  "framework": "actix-web",
  "framework-version": "4",
  "static-files": [
    "src/routes/login/home.html"
  ],
  "secrets": [
    "APP_APPLICATION__HMAC_SECRET",
    "APP_DATABASE__PASSWORD",
    "APP_EMAIL_CLIENT__AUTHORIZATION_TOKEN",
    "APP_EMAIL_CLIENT__SENDER_EMAIL"
  ],
  "rust-code-changes-to-support-resources": [
    {
      "filepath": "src/main.rs",
      "description": "Modify main.rs to use Shuttle's main macro and inject Postgres and Redis connections. Replace tokio::main with shuttle_runtime::main and modify the main function to accept PgPool and Redis from Shuttle."
    },
    {
      "filepath": "src/configuration.rs",
      "description": "Adapt configuration loading to use Shuttle's Secrets management instead of YAML files. Modify the Settings struct and related code to work with Shuttle's configuration injection."
    },
    {
      "filepath": "src/startup.rs",
      "description": "Modify application startup code to accept database pool and Redis connection from Shuttle instead of creating them. Adapt the Application::build function accordingly."
    },
    {
      "filepath": "Cargo.toml",
      "description": "Add Shuttle dependencies: shuttle-runtime, shuttle-actix-web, shuttle-shared-db, and shuttle-secrets. Update existing dependencies to versions compatible with Shuttle."
    }
  ]
}
```

Your output should be in Markdown."#;
