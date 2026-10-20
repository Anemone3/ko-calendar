//! Shared desktop OAuth client; each OS user owns a separate vault entry and Google grant.
//! No passwords, authorization codes or tokens are logged or stored in SQLite.
use chrono::{Local, Utc};
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl, basic::BasicClient,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use url::Url;

pub mod events;
pub use events::Snapshot;
const SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/calendar.events.readonly",
    "https://www.googleapis.com/auth/calendar.calendarlist.readonly",
];
const MISSING_CLIENT: &str = "Google Sync aún no está disponible en esta compilación. Se necesita una versión de ko-calendar con la conexión de Google habilitada.";

// Set once by the maintainer when compiling releases, never by each end user.
fn client_id() -> Result<String, String> {
    option_env!("KOKONUT_GOOGLE_CLIENT_ID")
        .filter(|id| !id.trim().is_empty() && id.ends_with(".apps.googleusercontent.com"))
        .map(str::to_owned)
        .ok_or_else(|| MISSING_CLIENT.into())
}
pub fn configured() -> bool {
    client_id().is_ok()
}
fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "No se pudo iniciar la conexión segura con Google.".into())
}
fn client(
    id: String,
) -> BasicClient<
    oauth2::EndpointSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointNotSet,
    oauth2::EndpointSet,
> {
    let mut client = BasicClient::new(ClientId::new(id))
        .set_auth_uri(AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into()).unwrap())
        .set_token_uri(TokenUrl::new("https://oauth2.googleapis.com/token".into()).unwrap())
        .set_auth_type(AuthType::RequestBody);
    if let Some(secret) = option_env!("KOKONUT_GOOGLE_CLIENT_SECRET").filter(|s| !s.is_empty()) {
        client = client.set_client_secret(ClientSecret::new(secret.into()));
    }
    client
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub account: String,
    client_id: String,
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}
impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("credentials", &"[redacted]")
            .finish()
    }
}

/// The browser is opened only after the user presses Google Sync.
pub async fn connect(
    open_browser: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Session, String> {
    let id = client_id()?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| {
            "No se pudo preparar el regreso desde Google. Vuelve a intentarlo.".to_string()
        })?;
    let address = listener
        .local_addr()
        .map_err(|_| "No se pudo preparar la conexión local.".to_string())?;
    let redirect = format!("http://{address}/oauth/callback");
    let client = client(id.clone()).set_redirect_uri(RedirectUrl::new(redirect).unwrap());
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (url, state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(SCOPES[0].into()))
        .add_scope(Scope::new(SCOPES[1].into()))
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent select_account")
        .set_pkce_challenge(challenge)
        .url();
    open_browser(url.as_str())?;
    let code = tokio::time::timeout(Duration::from_secs(180), callback(&listener, &state))
        .await
        .map_err(|_| {
            "La conexión caducó. Pulsa Google Sync para intentarlo otra vez.".to_string()
        })??;
    let http = http()?;
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(&http)
        .await
        .map_err(|_| {
            "No se pudo completar la autorización de Google. Vuelve a conectar.".to_string()
        })?;
    if token.scopes().is_some_and(|scopes| {
        SCOPES
            .iter()
            .any(|required| !scopes.iter().any(|s| s.as_str() == *required))
    }) {
        return Err("Google no concedió el acceso de lectura al calendario. Vuelve a conectar y permite ese acceso.".into());
    }
    let refresh_token = token
        .refresh_token()
        .map(|t| t.secret().clone())
        .ok_or("Google no concedió una sesión persistente. Vuelve a conectar la cuenta.")?;
    let mut session = Session {
        account: String::new(),
        client_id: id,
        access_token: token.access_token().secret().clone(),
        refresh_token,
        expires_at: Utc::now().timestamp()
            + token
                .expires_in()
                .unwrap_or(Duration::from_secs(3600))
                .as_secs() as i64,
    };
    session.account = events::account(&http, &session.access_token).await?;
    Ok(session)
}

fn callback_code(target: &str, expected: &CsrfToken) -> Result<Option<String>, String> {
    if !target.starts_with('/') || target.starts_with("//") {
        return Err("Invalid callback".into());
    }
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Invalid callback".to_string())?;
    if url.path() != "/oauth/callback" {
        return Ok(None);
    }
    let pairs = url.query_pairs().collect::<Vec<_>>();
    let values = |key: &str| {
        pairs
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.to_string())
            .collect::<Vec<_>>()
    };
    let states = values("state");
    if states.len() != 1 || CsrfToken::new(states[0].clone()) != *expected {
        return Err("Invalid state".into());
    }
    if !values("error").is_empty() {
        return Err("No se autorizó la conexión con Google.".into());
    }
    let codes = values("code");
    if codes.len() != 1 || codes[0].is_empty() {
        return Err("Invalid code".into());
    }
    Ok(Some(codes[0].clone()))
}
async fn callback(listener: &TcpListener, state: &CsrfToken) -> Result<String, String> {
    loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|_| "Se interrumpió el regreso desde Google.".to_string())?;
        let read = tokio::time::timeout(Duration::from_secs(3), async {
            let mut data = Vec::new();
            let mut buffer = [0; 1024];
            while data.len() < 16_384 {
                let count = socket.read(&mut buffer).await?;
                if count == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..count]);
                if data.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Ok::<_, std::io::Error>(data)
        })
        .await;
        let Ok(Ok(data)) = read else { continue };
        let text = String::from_utf8_lossy(&data);
        let parts = text
            .lines()
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>();
        let result = if parts.len() == 3 && parts[0] == "GET" && parts[2] == "HTTP/1.1" {
            callback_code(parts[1], state)
        } else {
            Ok(None)
        };
        let valid = matches!(&result, Ok(Some(_)));
        let denied =
            matches!(&result,Err(error) if error=="No se autorizó la conexión con Google.");
        let body = if valid {
            "Autorización recibida. Puedes cerrar esta pestaña y volver a ko-calendar."
        } else {
            "No se completó la conexión. Vuelve a ko-calendar para intentarlo otra vez."
        };
        let status = if valid { "200 OK" } else { "400 Bad Request" };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = tokio::time::timeout(
            Duration::from_secs(2),
            socket.write_all(response.as_bytes()),
        )
        .await;
        if valid || denied {
            return result.and_then(|r| r.ok_or_else(|| "No se recibió la autorización.".into()));
        }
    }
}

fn entry(directory: &Path) -> Result<keyring::Entry, String> {
    let digest = Sha256::digest(directory.to_string_lossy().as_bytes());
    let name = format!("google-{:x}", digest);
    keyring::Entry::new("Kokonut.Calendar", &name)
        .map_err(|_| "No se pudo acceder al almacén seguro de sesiones del sistema.".into())
}
pub async fn load(directory: PathBuf) -> Result<Option<Session>, String> {
    if !configured() {
        return Ok(None);
    }
    tokio::task::spawn_blocking(move || match entry(&directory)?.get_password() {
        Ok(value) => {
            let session: Session = serde_json::from_str(&value).map_err(|_| {
                "La sesión guardada no se puede leer. Vuelve a conectar Google.".to_string()
            })?;
            if session.client_id != client_id()? {
                return Ok(None);
            }
            Ok(Some(session))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("No se pudo recuperar la sesión del almacén seguro del sistema.".into()),
    })
    .await
    .map_err(|_| "No se pudo recuperar la sesión de Google.".to_string())?
}
pub async fn save(directory: PathBuf, session: Session) -> Result<Session, String> {
    tokio::task::spawn_blocking(move||{
        let value=serde_json::to_string(&session).map_err(|_|"No se pudo preparar la sesión.".to_string())?;
        entry(&directory)?.set_password(&value).map_err(|_|"No se pudo guardar la sesión en el almacén seguro del sistema. Comprueba que esté disponible y desbloqueado.".to_string())?;
        Ok(session)
    }).await.map_err(|_|"No se pudo guardar la sesión de Google.".to_string())?
}
/// Forget locally first, then revoke Google's grant. Network failure never restores local tokens.
pub async fn disconnect(directory: PathBuf, session: Session) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || match entry(&directory)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(
            "No se pudo eliminar la sesión del almacén seguro. Intenta desconectar otra vez."
                .to_string(),
        ),
    })
    .await
    .map_err(|_| "No se pudo desconectar la cuenta.".to_string())??;
    let response = match http(){
        Ok(http)=>http.post("https://oauth2.googleapis.com/revoke").form(&[("token",session.refresh_token)]).send().await,
        Err(_)=>return Ok(Some("Cuenta desconectada de este equipo. Retira el permiso desde tu cuenta de Google cuando tengas conexión.".into())),
    };
    if matches!(response,Ok(ref r) if r.status().is_success()) {
        Ok(None)
    } else {
        Ok(Some("Cuenta desconectada de este equipo. No se pudo retirar el permiso en Google; puedes hacerlo desde tu cuenta de Google.".into()))
    }
}
pub async fn sync(mut session: Session) -> Result<(Session, Snapshot), String> {
    let http = http()?;
    if session.expires_at <= Utc::now().timestamp() + 60 {
        let token = client(session.client_id.clone())
            .exchange_refresh_token(&RefreshToken::new(session.refresh_token.clone()))
            .request_async(&http)
            .await
            .map_err(|_| {
                "No se pudo renovar la conexión. Revisa Internet o vuelve a conectar Google."
                    .to_string()
            })?;
        session.access_token = token.access_token().secret().clone();
        session.expires_at = Utc::now().timestamp()
            + token
                .expires_in()
                .unwrap_or(Duration::from_secs(3600))
                .as_secs() as i64;
    }
    let snapshot = events::fetch(&http, &session.access_token, Local::now()).await?;
    Ok((session, snapshot))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_requires_state_and_rejects_duplicate_or_injected_parameters() {
        let state = CsrfToken::new("expected".into());
        assert_eq!(
            callback_code("/oauth/callback?state=expected&code=one%2Btwo", &state).unwrap(),
            Some("one+two".into())
        );
        for bad in [
            "/oauth/callback?state=wrong&code=x",
            "/oauth/callback?code=x",
            "/oauth/callback?state=expected&state=expected&code=x",
            "/oauth/callback?state=expected&code=x&code=y",
            "//attacker/oauth/callback?state=expected&code=x",
        ] {
            assert!(callback_code(bad, &state).is_err());
        }
        assert!(callback_code("/favicon.ico", &state).unwrap().is_none());
        assert!(
            callback_code("/oauth/callback?state=expected&error=access_denied", &state).is_err()
        );
    }
    #[test]
    fn diagnostic_debug_never_contains_credentials_or_identity() {
        let session = Session {
            account: "someone@example.org".into(),
            client_id: "id".into(),
            access_token: "access-secret".into(),
            refresh_token: "refresh-secret".into(),
            expires_at: 0,
        };
        let debug = format!("{session:?}");
        for private in ["someone@example.org", "access-secret", "refresh-secret"] {
            assert!(!debug.contains(private));
        }
    }
    #[test]
    fn real_loopback_listener_ignores_invalid_state_and_accepts_the_matching_callback() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async{
            let listener=TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0)).await.unwrap();
            let address=listener.local_addr().unwrap();
            let receive=tokio::spawn(async move{callback(&listener,&CsrfToken::new("random-state".into())).await});
            for (state,expected) in [("invalid","400"),("random-state","200")]{
                let mut socket=tokio::net::TcpStream::connect(address).await.unwrap();
                socket.write_all(format!("GET /oauth/callback?state={state}&code=code-from-provider HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes()).await.unwrap();
                let mut response=String::new();socket.read_to_string(&mut response).await.unwrap();assert!(response.starts_with(&format!("HTTP/1.1 {expected}")));
            }
            assert_eq!(tokio::time::timeout(Duration::from_secs(5),receive).await.unwrap().unwrap().unwrap(),"code-from-provider");
        });
    }
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "Requires the current user's Windows Credential Manager"]
    fn isolated_windows_vault_roundtrip() {
        let directory = std::env::temp_dir().join(format!(
            "kokonut-vault-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let item = entry(&directory).unwrap();
        item.set_password("kokonut-test-value-not-a-real-token")
            .unwrap();
        let read = item.get_password();
        let removed = item.delete_credential();
        assert_eq!(read.unwrap(), "kokonut-test-value-not-a-real-token");
        assert!(removed.is_ok());
        assert!(matches!(item.get_password(), Err(keyring::Error::NoEntry)));
    }
}
