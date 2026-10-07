//! Google installed-app OAuth: external browser, loopback redirect, PKCE and state.
//! Tokens live in Secret Service. Publisher client metadata may be bundled in the app.
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

const SCOPES: &str = "https://www.googleapis.com/auth/calendar.events https://www.googleapis.com/auth/calendar.calendarlist.readonly";

#[derive(Debug)]
pub struct SignInCancelled;
impl std::fmt::Display for SignInCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sign-in cancelled")
    }
}
impl std::error::Error for SignInCancelled {}

#[derive(Deserialize)]
pub struct ClientFile {
    pub installed: ClientConfig,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
}
impl ClientConfig {
    /// Publisher configuration is compiled into the app, never supplied by each user.
    pub fn bundled() -> Result<Self> {
        Self::from_publisher(
            option_env!("OCAL_GOOGLE_CLIENT_ID"),
            option_env!("OCAL_GOOGLE_CLIENT_SECRET"),
        )
    }

    fn from_publisher(id: Option<&str>, secret: Option<&str>) -> Result<Self> {
        let client = Self {
            client_id: id.unwrap_or_default().trim().to_owned(),
            client_secret: secret.unwrap_or_default().trim().to_owned(),
        };
        client.validate().context(
            "Google sign-in is not configured in this build. Install a publisher-configured build of Ocal",
        )?;
        Ok(client)
    }

    fn validate(&self) -> Result<()> {
        let prefix = self.client_id.strip_suffix(".apps.googleusercontent.com");
        anyhow::ensure!(
            prefix.is_some_and(|value| !value.is_empty()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')),
            "Invalid Google Desktop app client ID"
        );
        Ok(())
    }

    fn token_fields(&self) -> Vec<(&str, &str)> {
        let mut fields = vec![("client_id", self.client_id.as_str())];
        if !self.client_secret.is_empty() {
            fields.push(("client_secret", self.client_secret.as_str()));
        }
        fields
    }
}

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    client: ClientConfig,
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}
impl Drop for Tokens {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.access_token.zeroize();
        self.refresh_token.zeroize();
        self.client.client_secret.zeroize();
    }
}

impl Tokens {
    pub(crate) fn access_token(&self) -> &str {
        &self.access_token
    }
}

pub fn http() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Ocal/0.1")
        .build()?)
}

fn entry(id: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new("ocal.google", id)?)
}

pub fn store(id: &str, tokens: &Tokens) -> Result<()> {
    let json = Zeroizing::new(serde_json::to_string(tokens)?);
    entry(id)?
        .set_password(&json)
        .context("Unlock your desktop keyring to save this account")
}

pub fn remove(id: &str) -> Result<()> {
    entry(id)?
        .delete_credential()
        .context("Unlock your desktop keyring to remove unused credentials")
}

pub fn access_token(id: &str) -> Result<Zeroizing<String>> {
    let json = Zeroizing::new(
        entry(id)?
            .get_password()
            .context("Unlock your desktop keyring or reconnect Google Calendar")?,
    );
    let mut tokens: Tokens = serde_json::from_str(&json)?;
    if chrono::Utc::now().timestamp() + 60 >= tokens.expires_at {
        let mut fields = tokens.client.token_fields();
        fields.extend([
            ("refresh_token", tokens.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ]);
        let response = http()?
            .post("https://oauth2.googleapis.com/token")
            .form(&fields)
            .send()?;
        anyhow::ensure!(
            response.status().is_success(),
            "Google sign-in expired. Reconnect this account"
        );
        let refreshed: TokenResponse = response.json()?;
        tokens.access_token = refreshed.access_token;
        if let Some(refresh) = refreshed.refresh_token {
            tokens.refresh_token = refresh;
        }
        tokens.expires_at = chrono::Utc::now().timestamp() + refreshed.expires_in;
        store(id, &tokens)?;
    }
    Ok(Zeroizing::new(tokens.access_token.clone()))
}

/// Publish the original Google link while keeping its loopback listener and PKCE
/// verifier alive. The UI decides whether to copy the link or explicitly open it.
pub fn authorize(
    config: ClientConfig,
    link_ready: impl FnOnce(&str),
    cancelled: &AtomicBool,
) -> Result<Tokens> {
    config.validate()?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let redirect = format!(
        "http://127.0.0.1:{}/oauth/callback",
        listener.local_addr()?.port()
    );
    let verifier = Zeroizing::new(format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    ));
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = uuid::Uuid::new_v4().to_string();
    let url = authorization_url(&config, &redirect, &state, &challenge)?;
    link_ready(url.as_str());
    let code = wait_for_code(
        &listener,
        &state,
        Instant::now() + Duration::from_secs(600),
        cancelled,
    )?;
    if cancelled.load(Ordering::Relaxed) {
        return Err(SignInCancelled.into());
    }
    let mut fields = config.token_fields();
    fields.extend([
        ("code", code.as_str()),
        ("code_verifier", verifier.as_str()),
        ("redirect_uri", redirect.as_str()),
        ("grant_type", "authorization_code"),
    ]);
    let response = http()?
        .post("https://oauth2.googleapis.com/token")
        .form(&fields)
        .send()?;
    anyhow::ensure!(
        response.status().is_success(),
        "Google could not finish sign-in. Check the Desktop app client configuration"
    );
    let tokens: TokenResponse = response.json()?;
    Ok(Tokens {
        access_token: tokens.access_token,
        refresh_token: tokens
            .refresh_token
            .context("Google did not grant offline access")?,
        expires_at: chrono::Utc::now().timestamp() + tokens.expires_in,
        client: config,
    })
}

fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<String> {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(SignInCancelled.into());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Sign-in link expired. Copy a fresh sign-in link and try again"
        );
        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                let mut line = String::new();
                if BufReader::new(&stream)
                    .take(8192)
                    .read_line(&mut line)
                    .is_err()
                {
                    continue;
                }
                let target = line.split_whitespace().nth(1).unwrap_or("");
                let callback = match url::Url::parse(&format!("http://127.0.0.1{target}")) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                let result = callback_code(&callback, state);
                let denied = callback.path() == "/oauth/callback"
                    && callback.query_pairs().any(|(k, _)| k == "error")
                    && callback
                        .query_pairs()
                        .any(|(k, v)| k == "state" && v == state);
                let body = if result.is_ok() {
                    "Google authorization received. Return to Ocal to finish connecting your calendar."
                } else if denied {
                    "Sign-in was cancelled. Return to Ocal to try again."
                } else {
                    "This callback does not match the active sign-in. Return to Ocal and copy its Google sign-in link; keep Ocal open while signing in."
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    if result.is_ok() || denied {
                        "200 OK"
                    } else {
                        "400 Bad Request"
                    },
                    body.len(),
                    body
                );
                // Ignore unrelated/forged local callbacks without consuming the flow.
                if let Ok(code) = result {
                    return Ok(code);
                }
                if denied {
                    return Err(SignInCancelled.into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn authorization_url(
    config: &ClientConfig,
    redirect: &str,
    state: &str,
    challenge: &str,
) -> Result<url::Url> {
    let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", config.client_id.as_str()),
        ("redirect_uri", redirect),
        ("response_type", "code"),
        ("scope", SCOPES),
        ("access_type", "offline"),
        // Always let the user choose, including when the browser has a default account.
        ("prompt", "select_account consent"),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
    ]);
    Ok(url)
}

fn callback_code(url: &url::Url, state: &str) -> Result<String> {
    anyhow::ensure!(url.path() == "/oauth/callback", "Wrong OAuth callback path");
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    anyhow::ensure!(
        params.get("state").is_some_and(|value| value == state),
        "OAuth state mismatch"
    );
    anyhow::ensure!(
        !params.contains_key("error"),
        "Google sign-in was cancelled"
    );
    params
        .get("code")
        .filter(|c| !c.is_empty())
        .cloned()
        .context("Missing OAuth authorization code")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preparing_a_link_does_not_launch_a_browser_and_cancellation_releases_the_attempt() {
        let config = ClientConfig::from_publisher(
            Some("123-example.apps.googleusercontent.com"),
            Some("test-secret"),
        )
        .unwrap();
        let cancelled = AtomicBool::new(false);
        let mut copied = String::new();
        let result = authorize(
            config,
            |url| {
                copied = url.to_owned();
                cancelled.store(true, Ordering::Relaxed);
            },
            &cancelled,
        );
        assert!(
            result
                .err()
                .unwrap()
                .downcast_ref::<SignInCancelled>()
                .is_some()
        );
        let url = url::Url::parse(&copied).unwrap();
        assert_eq!(url.host_str(), Some("accounts.google.com"));
        assert_eq!(url.path(), "/o/oauth2/v2/auth");
        assert!(!copied.contains("test-secret"));
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert!(params.contains_key("code_challenge"));
        assert!(!params.contains_key("code_verifier"));
        assert!(!params.contains_key("client_secret"));
        let redirect = url::Url::parse(&params["redirect_uri"]).unwrap();
        assert!(std::net::TcpStream::connect(("127.0.0.1", redirect.port().unwrap())).is_err());
    }

    #[test]
    fn copied_link_keeps_the_callback_active_after_rejecting_a_stale_attempt() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            wait_for_code(
                &listener,
                "current-state",
                Instant::now() + Duration::from_secs(5),
                &AtomicBool::new(false),
            )
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let stale = client
            .get(format!(
                "http://{address}/oauth/callback?state=old-state&code=old-code"
            ))
            .send()
            .unwrap();
        assert_eq!(stale.status(), reqwest::StatusCode::BAD_REQUEST);
        assert!(
            stale
                .text()
                .unwrap()
                .contains("copy its Google sign-in link")
        );
        let accepted = client
            .get(format!(
                "http://{address}/oauth/callback?state=current-state&code=fresh-code"
            ))
            .send()
            .unwrap();
        assert!(accepted.status().is_success());
        assert_eq!(server.join().unwrap().unwrap(), "fresh-code");
    }

    #[test]
    fn expired_links_and_google_denials_end_the_attempt_cleanly() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let expired = wait_for_code(&listener, "state", Instant::now(), &AtomicBool::new(false))
            .unwrap_err()
            .to_string();
        assert!(expired.contains("Copy a fresh sign-in link"));
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            wait_for_code(
                &listener,
                "state",
                Instant::now() + Duration::from_secs(5),
                &AtomicBool::new(false),
            )
        });
        let denied = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap()
            .get(format!(
                "http://{address}/oauth/callback?state=state&error=access_denied"
            ))
            .send()
            .unwrap();
        assert!(denied.status().is_success());
        assert!(denied.text().unwrap().contains("cancelled"));
        assert!(
            server
                .join()
                .unwrap()
                .unwrap_err()
                .downcast_ref::<SignInCancelled>()
                .is_some()
        );
    }

    #[test]
    fn adding_an_account_prompts_for_account_selection_and_keeps_pkce_and_offline_access() {
        let client = super::ClientConfig::from_publisher(
            Some("123-example.apps.googleusercontent.com"),
            None,
        )
        .unwrap();
        let url = super::authorization_url(
            &client,
            "http://127.0.0.1:1234/oauth/callback",
            "state",
            "challenge",
        )
        .unwrap();
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(params["prompt"], "select_account consent");
        assert_eq!(params["access_type"], "offline");
        assert_eq!(params["code_challenge_method"], "S256");
        assert_eq!(params["code_challenge"], "challenge");
        assert_eq!(params["state"], "state");
        assert!(!params.contains_key("login_hint"));
    }

    #[test]
    fn publisher_configuration_requires_a_real_client_id() {
        for id in [
            None,
            Some(""),
            Some(".apps.googleusercontent.com"),
            Some("bad id.apps.googleusercontent.com"),
            Some("https://example.com"),
        ] {
            assert!(super::ClientConfig::from_publisher(id, None).is_err());
        }
        let client = super::ClientConfig::from_publisher(
            Some(" 123-example.apps.googleusercontent.com "),
            Some(" desktop-secret "),
        )
        .unwrap();
        assert_eq!(client.client_id, "123-example.apps.googleusercontent.com");
        assert_eq!(
            client.token_fields()[1],
            ("client_secret", "desktop-secret")
        );
    }

    #[test]
    fn native_clients_without_secrets_omit_the_secret_in_token_requests() {
        let client: super::ClientFile = serde_json::from_str(
            r#"{"installed":{"client_id":"123-example.apps.googleusercontent.com"}}"#,
        )
        .unwrap();
        assert!(client.installed.client_secret.is_empty());
        assert_eq!(
            client.installed.token_fields(),
            vec![("client_id", "123-example.apps.googleusercontent.com")]
        );
    }

    #[test]
    fn rejects_forged_callback_and_wrong_route() {
        assert!(
            super::callback_code(
                &url::Url::parse("http://127.0.0.1/oauth/callback?state=bad&code=a").unwrap(),
                "expected"
            )
            .is_err()
        );
        assert!(
            super::callback_code(
                &url::Url::parse("http://127.0.0.1/?state=expected&code=a").unwrap(),
                "expected"
            )
            .is_err()
        );
        assert_eq!(
            super::callback_code(
                &url::Url::parse("http://127.0.0.1/oauth/callback?state=expected&code=a").unwrap(),
                "expected"
            )
            .unwrap(),
            "a"
        );
    }
}
