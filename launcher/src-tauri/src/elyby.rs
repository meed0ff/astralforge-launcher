use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{timeout, Duration};
use url::Url;

pub const ELYBY_CLIENT_ID: &str = "astralforge1";
pub const ELYBY_CLIENT_SECRET: &str = "ZA1LTuQpD_SmWwRyF3Foh0MgKjQpd7McMosEjXFpnovF4wVGlTCtwK-ZN80S_ihN";
pub const REDIRECT_URI: &str = "astralforge://callback";
pub const CALLBACK_IPC_ADDR: &str = "127.0.0.1:45679";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElybyUser {
    pub username: String,
    pub uuid: String,
    #[serde(rename = "accessToken", alias = "access_token")]
    pub access_token: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    id: u64,
    username: String,
    uuid: Option<String>,
}

/// Регистрирует обработчик astralforge:// в системе на Linux (~/.local/share/applications)
#[cfg(target_os = "linux")]
pub fn register_uri_scheme_linux() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(data_dir) = crate::ui_bridge::user_data_local_dir() else {
        return;
    };
    let apps_dir = data_dir.join("applications");
    if std::fs::create_dir_all(&apps_dir).is_err() {
        return;
    }
    let desktop_path = apps_dir.join("astralforge-launcher.desktop");
    let desktop_content = format!(
        "[Desktop Entry]\nName=AstralForge Launcher\nExec={} %u\nType=Application\nTerminal=false\nNoDisplay=true\nMimeType=x-scheme-handler/astralforge;\n",
        exe_path.to_string_lossy()
    );
    if std::fs::write(&desktop_path, desktop_content).is_ok() {
        let _ = std::process::Command::new("xdg-mime")
            .args(["default", "astralforge-launcher.desktop", "x-scheme-handler/astralforge"])
            .output();
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps_dir)
            .output();
    }
}

/// Регистрирует обработчик astralforge:// в реестре Windows (HKCU\Software\Classes\astralforge)
#[cfg(target_os = "windows")]
pub fn register_uri_scheme_windows() {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let command_value = format!("\"{}\" \"%1\"", exe_path.to_string_lossy());

    let entries: &[&[&str]] = &[
        &[
            "add",
            r"HKCU\Software\Classes\astralforge",
            "/ve",
            "/d",
            "URL:AstralForge Protocol",
            "/f",
        ],
        &[
            "add",
            r"HKCU\Software\Classes\astralforge",
            "/v",
            "URL Protocol",
            "/d",
            "",
            "/f",
        ],
        &[
            "add",
            r"HKCU\Software\Classes\astralforge\shell\open\command",
            "/ve",
            "/d",
            &command_value,
            "/f",
        ],
    ];

    for args in entries {
        let mut cmd = std::process::Command::new("reg");
        cmd.args(*args).creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.output();
    }
}

/// Кроссплатформенная регистрация URI-схемы astralforge:// в текущей ОС
pub fn register_uri_scheme() {
    #[cfg(target_os = "linux")]
    register_uri_scheme_linux();
    #[cfg(target_os = "windows")]
    register_uri_scheme_windows();
}

/// Если бинарник запущен с аргументом astralforge://..., пересылает его в основной процесс и возвращает true
pub fn try_forward_deep_link() -> bool {
    if let Some(arg) = std::env::args().skip(1).find(|a| a.starts_with("astralforge://")) {
        if let Ok(mut stream) = std::net::TcpStream::connect(CALLBACK_IPC_ADDR) {
            let _ = std::io::Write::write_all(&mut stream, arg.as_bytes());
            let _ = std::io::Write::flush(&mut stream);
        }
        return true;
    }
    false
}

/// Открывает браузер для авторизации через Ely.by OAuth2 и ждет callback astralforge://callback
pub async fn login_via_elyby() -> Result<ElybyUser, String> {
    register_uri_scheme();

    let listener = TcpListener::bind(CALLBACK_IPC_ADDR)
        .await
        .map_err(|e| format!("Не удалось запустить обработчик callback OAuth: {}", e))?;

    let auth_url = format!(
        "https://account.ely.by/oauth2/v1?client_id={}&redirect_uri={}&response_type=code&scope=account_info%20account_email",
        ELYBY_CLIENT_ID,
        urlencoding::encode(REDIRECT_URI)
    );

    open::that(&auth_url).map_err(|e| format!("Не удалось открыть браузер: {}", e))?;

    let accept_res = timeout(Duration::from_secs(180), listener.accept())
        .await
        .map_err(|_| "Время ожидания авторизации Ely.by истекло (3 минуты).".to_string())?
        .map_err(|e| format!("Ошибка ожидания callback OAuth: {}", e))?;

    let (mut socket, _) = accept_res;
    let mut buf = Vec::new();
    socket
        .read_to_end(&mut buf)
        .await
        .map_err(|e| format!("Ошибка чтения callback OAuth: {}", e))?;
    let _ = socket.shutdown().await;

    let callback_url = String::from_utf8_lossy(&buf).trim().to_string();
    let parsed_url = Url::parse(&callback_url)
        .map_err(|e| format!("Не удалось разобрать URL callback ({}): {}", callback_url, e))?;
    let query_pairs: HashMap<_, _> = parsed_url.query_pairs().into_owned().collect();

    let code = match query_pairs.get("code") {
        Some(c) => c.clone(),
        None => {
            let error_msg = query_pairs
                .get("error")
                .cloned()
                .unwrap_or_else(|| "Неизвестная ошибка".into());
            return Err(format!("Ely.by ошибка: {}", error_msg));
        }
    };

    // Обмениваем code на access_token
    let client = reqwest::Client::new();
    let mut token_params = HashMap::new();
    token_params.insert("grant_type", "authorization_code");
    token_params.insert("client_id", ELYBY_CLIENT_ID);
    token_params.insert("client_secret", ELYBY_CLIENT_SECRET);
    token_params.insert("redirect_uri", REDIRECT_URI);
    token_params.insert("code", &code);

    let token_resp = client
        .post("https://account.ely.by/api/oauth2/v1/token")
        .form(&token_params)
        .send()
        .await
        .map_err(|e| format!("Не удалось обменять токен в Ely.by: {}", e))?;

    if !token_resp.status().is_success() {
        return Err(format!("Ошибка получения токена: {}", token_resp.status()));
    }

    let token_data: TokenResponse = token_resp
        .json()
        .await
        .map_err(|e| format!("Ошибка парсинга ответа токена: {}", e))?;

    // Получаем информацию об аккаунте
    let user_resp = client
        .get("https://account.ely.by/api/account/v1/info")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|e| format!("Ошибка запроса профиля Ely.by: {}", e))?;

    let user_info: UserInfoResponse = user_resp
        .json()
        .await
        .map_err(|e| format!("Ошибка парсинга профиля Ely.by: {}", e))?;

    let final_uuid = user_info.uuid.unwrap_or_else(|| {
        uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, user_info.username.as_bytes()).to_string()
    });

    Ok(ElybyUser {
        username: user_info.username,
        uuid: final_uuid,
        access_token: token_data.access_token,
    })
}
