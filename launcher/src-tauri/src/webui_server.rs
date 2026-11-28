use crate::game_launcher::RunningGameState;
use crate::ui_bridge::UiHandle;
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Notify};

const INDEX_HTML: &str = include_str!("../../src/index.html");
const STYLE_CSS: &str = include_str!("../../src/style.css");
const APP_JS: &str = include_str!("../../src/app.js");
const LOGO_WEBP: &[u8] = include_bytes!("../../src/logo.webp");

const SHIM_JS: &str = r#"(function () {
  const listeners = new Map();
  let evSource = null;

  function ensureEvents() {
    if (evSource) return;
    evSource = new EventSource('/__api/events');
    evSource.onmessage = function (msg) {
      try {
        const parsed = JSON.parse(msg.data);
        const list = listeners.get(parsed.event);
        if (list) {
          for (const cb of list) {
            try { cb({ event: parsed.event, payload: parsed.payload }); } catch (_) {}
          }
        }
      } catch (_) {}
    };
  }

  window.__TAURI__ = {
    core: {
      invoke: async function (cmd, args) {
        const resp = await fetch('/__api/invoke', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ cmd: cmd, args: args || {} })
        });
        const body = await resp.json();
        if (!body.ok) {
          throw body.error || 'Unknown error';
        }
        return body.data;
      }
    },
    event: {
      listen: async function (eventName, cb) {
        ensureEvents();
        if (!listeners.has(eventName)) listeners.set(eventName, []);
        listeners.get(eventName).push(cb);
        return function () {
          const arr = listeners.get(eventName);
          if (!arr) return;
          const idx = arr.indexOf(cb);
          if (idx >= 0) arr.splice(idx, 1);
        };
      }
    }
  };
})();
"#;

pub type InvokeDispatcher = Arc<
    dyn Fn(String, Value, UiHandle, RunningGameState) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send>>
        + Send
        + Sync,
>;

pub fn run_webui_blocking(app_title: &str, dispatcher: InvokeDispatcher) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to start async runtime");

    let game_state = RunningGameState::default();
    let (tx, _rx) = broadcast::channel::<String>(256);
    let ui = UiHandle::from_web(tx.clone());
    let active_clients = Arc::new(AtomicUsize::new(0));
    let had_client = Arc::new(AtomicBool::new(false));
    let shutdown_notify = Arc::new(Notify::new());

    let url = rt.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind local WebUI port");
        let port = listener.local_addr().expect("Failed to get local port").port();
        let url = format!("http://127.0.0.1:{}", port);

        let clients_watch = active_clients.clone();
        let had_watch = had_client.clone();
        let state_watch = game_state.clone();
        let notify_watch = shutdown_notify.clone();
        tokio::spawn(async move {
            let mut idle_ticks = 0u32;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if had_watch.load(Ordering::Relaxed)
                    && clients_watch.load(Ordering::Relaxed) == 0
                    && !state_watch.is_running()
                {
                    idle_ticks += 1;
                    if idle_ticks >= 5 {
                        notify_watch.notify_one();
                        break;
                    }
                } else {
                    idle_ticks = 0;
                }
            }
        });

        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let tx_c = tx.clone();
                let ui_c = ui.clone();
                let state_c = game_state.clone();
                let disp_c = dispatcher.clone();
                let clients_c = active_clients.clone();
                let had_c = had_client.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, tx_c, ui_c, state_c, disp_c, clients_c, had_c).await;
                });
            }
        });

        url
    });

    println!("====================================================");
    println!("{} (WebUI режим без WebView2 / Windows 7)", app_title);
    println!("Интерфейс открыт в браузере: {}", url);
    println!("Не закрывайте это окно, пока пользуетесь программой.");
    println!("Для выхода закройте вкладку браузера или это окно.");
    println!("====================================================");

    if std::env::var_os("ETERNUM_NO_BROWSER").is_none() {
        let _ = open::that(&url);
    }

    #[cfg(feature = "webui")]
    {
        let notify_stdin = shutdown_notify.clone();
        std::thread::spawn(move || {
            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_ok_and(|n| n > 0) {
                notify_stdin.notify_one();
            }
        });
    }

    rt.block_on(shutdown_notify.notified());
}

async fn handle_connection(
    mut stream: TcpStream,
    tx: broadcast::Sender<String>,
    ui: UiHandle,
    state: RunningGameState,
    dispatcher: InvokeDispatcher,
    active_clients: Arc<AtomicUsize>,
    had_client: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    let header_end;

    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if buf.len() > 64 * 1024 {
            return write_http(&mut stream, "400 Bad Request", "text/plain", b"Bad Request").await;
        }
    }

    let (method, path, content_length) = {
        let headers_str = String::from_utf8_lossy(&buf[..header_end]);
        let mut lines = headers_str.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts
            .next()
            .unwrap_or("/")
            .split('?')
            .next()
            .unwrap_or("/")
            .to_string();

        let mut content_length: usize = 0;
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    content_length = v.trim().parse().unwrap_or(0);
                }
            }
        }
        (method, path, content_length)
    };

    while buf.len() < header_end + content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    let body = &buf[header_end..(header_end + content_length).min(buf.len())];

    match (method.as_str(), path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            let injected = INDEX_HTML.replace(
                "<script src=\"app.js\"></script>",
                "<script src=\"/__api/shim.js\"></script>\n  <script src=\"app.js\"></script>",
            );
            write_http(&mut stream, "200 OK", "text/html; charset=utf-8", injected.as_bytes()).await
        }
        ("GET", "/style.css") => {
            write_http(&mut stream, "200 OK", "text/css; charset=utf-8", STYLE_CSS.as_bytes()).await
        }
        ("GET", "/app.js") => {
            write_http(
                &mut stream,
                "200 OK",
                "application/javascript; charset=utf-8",
                APP_JS.as_bytes(),
            )
            .await
        }
        ("GET", "/logo.webp") => write_http(&mut stream, "200 OK", "image/webp", LOGO_WEBP).await,
        ("GET", "/__api/shim.js") => {
            write_http(
                &mut stream,
                "200 OK",
                "application/javascript; charset=utf-8",
                SHIM_JS.as_bytes(),
            )
            .await
        }
        ("GET", "/__api/events") => {
            had_client.store(true, Ordering::Relaxed);
            active_clients.fetch_add(1, Ordering::Relaxed);
            let res = handle_sse(stream, tx.subscribe()).await;
            active_clients.fetch_sub(1, Ordering::Relaxed);
            res
        }
        ("POST", "/__api/invoke") => {
            let req_val: Value = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(e) => {
                    let resp = json!({ "ok": false, "error": format!("Invalid JSON: {}", e) });
                    return write_http(
                        &mut stream,
                        "200 OK",
                        "application/json; charset=utf-8",
                        resp.to_string().as_bytes(),
                    )
                    .await;
                }
            };
            let cmd = req_val["cmd"].as_str().unwrap_or("").to_string();
            let args = req_val.get("args").cloned().unwrap_or_else(|| json!({}));
            let res = dispatcher(cmd, args, ui, state).await;
            let payload = match res {
                Ok(data) => json!({ "ok": true, "data": data }),
                Err(err) => json!({ "ok": false, "error": err }),
            };
            write_http(
                &mut stream,
                "200 OK",
                "application/json; charset=utf-8",
                payload.to_string().as_bytes(),
            )
            .await
        }
        _ => write_http(&mut stream, "404 Not Found", "text/plain", b"Not Found").await,
    }
}

async fn handle_sse(mut stream: TcpStream, mut rx: broadcast::Receiver<String>) -> std::io::Result<()> {
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n: connected\n\n";
    stream.write_all(head.as_bytes()).await?;
    stream.flush().await?;

    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(line) => {
                        let chunk = format!("data: {}\n\n", line);
                        if stream.write_all(chunk.as_bytes()).await.is_err() {
                            break;
                        }
                        let _ = stream.flush().await;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = ticker.tick() => {
                if stream.write_all(b": ping\n\n").await.is_err() {
                    break;
                }
                let _ = stream.flush().await;
            }
        }
    }
    Ok(())
}

async fn write_http(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}
