use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::Deserialize;

const DEFAULT_REGISTRY_URL: &str = "https://gray.alignment.id/api";
const REGISTRY_URL_ENV: &str = "GRAY_REGISTRY_URL";
const SITE_ACCOUNT_URL: &str = "https://gray.alignment.id/account";
const CODE_TTL_MINUTES: u32 = 5;

const USAGE: &str = "gray-cloud — your Gray Cloud machine on gray.alignment.id

usage: gray cloud [command] [args]

commands:
  connect [code]   pair this terminal with a one-time code from the account page
  status           provision if needed and print the machine's state
  logout           revoke the stored token and forget the pairing
  <message>        chat with your machine (first call provisions it)

environment:
  GRAY_REGISTRY_URL   registry base (default https://gray.alignment.id/api)";

#[derive(Debug, Default, Deserialize)]
struct StatusOut {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    suspended: bool,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ProvisionOut {
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChatOut {
    #[serde(default)]
    response: String,
}

#[derive(Debug, Default, Deserialize)]
struct Account {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    provider: Option<String>,
}

impl Account {
    fn label(&self) -> String {
        self.display_name
            .clone()
            .or_else(|| self.username.clone().map(|u| format!("@{u}")))
            .or_else(|| self.provider.clone())
            .unwrap_or_else(|| "your account".to_string())
    }
}

#[derive(Deserialize)]
struct TokenExchangeResponse {
    token: String,
    #[serde(default)]
    user: Option<Account>,
}

fn gray_home() -> anyhow::Result<PathBuf> {
    std::env::var_os("GRAY_HOME")
        .filter(|v| !v.to_string_lossy().trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home().map(|p| p.join(".gray")))
        .context("cannot resolve home: set GRAY_HOME or the platform user profile")
}

fn user_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::home_dir()
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn save_private_json(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let body = serde_json::to_vec_pretty(value)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let unique = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    );
    let tmp = parent.join(format!(".gray-{unique}.tmp"));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = (|| -> std::io::Result<()> {
        let mut f = options.open(&tmp)?;
        f.write_all(&body)?;
        f.sync_all()?;
        Ok(())
    })();
    match written {
        Ok(()) => {
            std::fs::rename(&tmp, path)?;
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e.into())
        }
    }
}

fn token_path() -> anyhow::Result<PathBuf> {
    Ok(gray_home()?.join("registry-token.json"))
}

fn load_token() -> Option<String> {
    let path = token_path().ok()?;
    let body = std::fs::read_to_string(path).ok()?;
    let token = serde_json::from_str::<serde_json::Value>(&body).ok()?;
    token
        .get("token")
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn save_token(token: &str) -> anyhow::Result<()> {
    save_private_json(&token_path()?, &serde_json::json!({ "token": token }))
}

fn normalize_base_url(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        DEFAULT_REGISTRY_URL.to_string()
    } else {
        raw.trim_end_matches('/').to_string()
    }
}

fn base_url() -> String {
    normalize_base_url(&std::env::var(REGISTRY_URL_ENV).unwrap_or_default())
}

fn is_loopback_host(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

fn endpoint(path: &str) -> anyhow::Result<String> {
    let base = reqwest::Url::parse(&format!("{}/", base_url()))
        .map_err(|e| anyhow::anyhow!("{REGISTRY_URL_ENV} is not a valid absolute URL: {e}"))?;
    if base.scheme() != "https" && !is_loopback_host(base.host_str().unwrap_or_default()) {
        anyhow::bail!(
            "{REGISTRY_URL_ENV} must be https (http is allowed only for loopback): {base}"
        );
    }
    Ok(base.join(path.trim_start_matches('/'))?.to_string())
}

fn strip_terminal_escapes(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() || c == '~' {
                    break;
                }
            }
            continue;
        }
        if !c.is_control() {
            out.push(c);
        }
    }
    out
}

fn normalize_code(raw: &str) -> Option<String> {
    let code = strip_terminal_escapes(raw);
    let code = code.trim();
    (!code.is_empty()).then(|| code.to_string())
}

fn prompt_for_code() -> Option<String> {
    eprint!("code: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => normalize_code(&line),
    }
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("gray-cloud/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

async fn error_message(resp: reqwest::Response) -> String {
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(msg) = parsed.get("error").and_then(|v| v.as_str())
    {
        return msg.to_string();
    }
    format!("registry returned {status}")
}

async fn login_with_code(code: &str) -> anyhow::Result<(String, Account)> {
    let url = endpoint("auth/token")?;
    let resp = http_client()?
        .post(&url)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("could not reach {url}: {e}"))?;
    if !resp.status().is_success() {
        anyhow::bail!(error_message(resp).await);
    }
    let parsed: TokenExchangeResponse = resp.json().await?;
    Ok((parsed.token, parsed.user.unwrap_or_default()))
}

async fn revoke_token(token: &str) -> bool {
    let client = match http_client() {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = match endpoint("auth/tokens") {
        Ok(u) => u,
        Err(_) => return false,
    };
    let sent = client
        .delete(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await;
    matches!(sent, Ok(r) if r.status().is_success())
}

async fn connect_with_code(code: Option<&str>) -> anyhow::Result<String> {
    let code = match code.and_then(normalize_code) {
        Some(code) => code,
        None => {
            eprintln!("Connect this terminal to your Gray Cloud machine.\n");
            eprintln!(
                "1. Open {SITE_ACCOUNT_URL} and sign in (GitHub, Google, or Discord).\n\
                 2. Choose \"Generate CLI login code\" — it expires in {CODE_TTL_MINUTES} minutes.\n\
                 3. Paste it here.\n"
            );
            prompt_for_code().ok_or_else(|| {
                anyhow::anyhow!("no enrollment code given — run `gray cloud connect <code>`")
            })?
        }
    };
    let (token, account) = login_with_code(&code).await?;
    let previous = load_token().filter(|p| *p != token);
    match save_token(&token) {
        Ok(()) => {
            if let Some(previous) = previous {
                revoke_token(&previous).await;
            }
        }
        Err(e) => {
            revoke_token(&token).await;
            return Err(e);
        }
    }
    println!("connected as {}", account.label());
    Ok(token)
}

async fn require_token() -> anyhow::Result<String> {
    match load_token() {
        Some(token) => Ok(token),
        None => connect_with_code(None).await,
    }
}

async fn post<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    token: &str,
    path: &str,
    body: &serde_json::Value,
) -> anyhow::Result<(reqwest::StatusCode, T)> {
    let url = endpoint(path)?;
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("could not reach {url}: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(msg) = parsed.get("error").and_then(|v| v.as_str())
        && !status.is_success()
    {
        anyhow::bail!("{msg}");
    }
    if !status.is_success() {
        anyhow::bail!("registry returned {status}");
    }
    Ok((status, serde_json::from_str(&text)?))
}

async fn provision(client: &reqwest::Client, token: &str) -> anyhow::Result<ProvisionOut> {
    let (_, out): (_, ProvisionOut) =
        post(client, token, "agent/provision", &serde_json::json!({})).await?;
    Ok(out)
}

async fn fetch_status(client: &reqwest::Client, token: &str) -> anyhow::Result<StatusOut> {
    let url = endpoint("agent/status")?;
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("could not reach {url}: {e}"))?;
    if !resp.status().is_success() {
        anyhow::bail!(error_message(resp).await);
    }
    Ok(resp.json().await?)
}

async fn chat(
    client: &reqwest::Client,
    token: &str,
    message: &str,
) -> anyhow::Result<ChatOut> {
    let (_, out): (_, ChatOut) = post(
        client,
        token,
        "agent/chat",
        &serde_json::json!({ "message": message }),
    )
    .await?;
    Ok(out)
}

async fn show_status(client: &reqwest::Client, token: &str) -> anyhow::Result<()> {
    let prov = provision(client, token).await?;
    let status = fetch_status(client, token).await.unwrap_or(StatusOut {
        status: prov.status.clone(),
        suspended: false,
        agent_id: None,
    });
    if status.suspended {
        println!("your Gray machine is suspended — check your seat at /plans");
        return Ok(());
    }
    match status.status.as_deref() {
        Some("active") => match status.agent_id.as_deref() {
            Some(id) => {
                println!("your Gray machine {id} is up — chat with it: gray cloud <message>")
            }
            None => println!("your Gray machine is up — chat with it: gray cloud <message>"),
        },
        Some(s) => println!("your Gray machine is {s} — first boot takes a couple of minutes"),
        None => println!("your Gray machine is provisioned"),
    }
    Ok(())
}

async fn run_status() -> anyhow::Result<()> {
    let client = http_client()?;
    let token = require_token().await?;
    show_status(&client, &token).await
}

async fn run_chat(words: &[String]) -> anyhow::Result<()> {
    let client = http_client()?;
    let token = require_token().await?;
    provision(&client, &token).await?;
    let out = chat(&client, &token, &words.join(" ")).await?;
    println!("{}", out.response.trim_end());
    Ok(())
}

fn run(result: anyhow::Result<()>) -> anyhow::Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("manifest") => {
            println!(
                "{}",
                serde_json::json!({
                    "name": "cloud",
                    "version": env!("CARGO_PKG_VERSION"),
                    "protocol": "1.1",
                    "tools": [],
                    "commands": [],
                    "completion": ["connect", "status", "logout"],
                })
            );
            Ok(())
        }
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some("-V" | "--version" | "version") => {
            println!("gray-cloud {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("connect") => {
            let r = connect_with_code(args.get(1).map(String::as_str))
                .await
                .and_then(|token| {
                    let client = http_client()?;
                    Ok((client, token))
                });
            match r {
                Ok((client, token)) => run(show_status(&client, &token).await),
                Err(e) => run(Err(e)),
            }
        }
        Some("status") => run(run_status().await),
        Some("logout") => run(async {
            let outcome = match load_token() {
                Some(token) => match revoke_token(&token).await {
                    true => "logged out — registry token revoked",
                    false => {
                        "logged out locally — the registry could not revoke the token, so it may still be valid"
                    }
                },
                None => "logged out — no registry token was stored",
            };
            let path = token_path()?;
            if path.exists() {
                std::fs::remove_file(path)?;
            }
            println!("{outcome}");
            Ok(())
        }
        .await),
        None => run(run_status().await),
        Some(_) => run(run_chat(&args).await),
    }
}
