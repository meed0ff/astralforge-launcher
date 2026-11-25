use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub online: bool,
    pub ping_ms: u64,
    pub online_players: u32,
    pub max_players: u32,
    pub motd: String,
}

fn write_varint(buf: &mut Vec<u8>, mut val: i32) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if val == 0 {
            break;
        }
    }
}

async fn read_varint<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<i32, String> {
    let mut num_read = 0;
    let mut result = 0;
    loop {
        let byte = reader.read_u8().await.map_err(|e| e.to_string())?;
        let value = (byte & 0x7F) as i32;
        result |= value << (7 * num_read);
        num_read += 1;
        if num_read > 5 {
            return Err("VarInt too big".into());
        }
        if (byte & 0x80) == 0 {
            break;
        }
    }
    Ok(result)
}

/// Выполняет официальный Minecraft Server Ping Protocol
pub async fn ping_server(host: &str, port: u16) -> Result<ServerStatus, String> {
    let addr = format!("{}:{}", host, port);
    let start_time = Instant::now();

    let mut stream = timeout(Duration::from_millis(3000), TcpStream::connect(&addr))
        .await
        .map_err(|_| "Таймаут подключения к серверу".to_string())?
        .map_err(|e| format!("Не удалось подключиться: {}", e))?;

    let ping_duration = start_time.elapsed().as_millis() as u64;

    // 1. Handshake Packet (id = 0, protocol = 767 (1.21.1), host, port, next_state = 1)
    let mut handshake_data = Vec::new();
    write_varint(&mut handshake_data, 0x00); // Packet ID
    write_varint(&mut handshake_data, 767); // Protocol version 1.21.1
    write_varint(&mut handshake_data, host.len() as i32);
    handshake_data.extend_from_slice(host.as_bytes());
    handshake_data.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake_data, 1); // Next state: 1 (status)

    let mut handshake_packet = Vec::new();
    write_varint(&mut handshake_packet, handshake_data.len() as i32);
    handshake_packet.extend(handshake_data);

    stream.write_all(&handshake_packet).await.map_err(|e| e.to_string())?;

    // 2. Status Request Packet (id = 0, length = 1)
    let request_packet = vec![0x01, 0x00];
    stream.write_all(&request_packet).await.map_err(|e| e.to_string())?;

    // 3. Status Response
    let _packet_len = read_varint(&mut stream).await?;
    let packet_id = read_varint(&mut stream).await?;
    if packet_id != 0 {
        return Err("Неверный ID пакета ответа".into());
    }

    let json_len = read_varint(&mut stream).await? as usize;
    let mut json_bytes = vec![0u8; json_len];
    stream.read_exact(&mut json_bytes).await.map_err(|e| e.to_string())?;

    let json_str = String::from_utf8_lossy(&json_bytes);
    let v: serde_json::Value = serde_json::from_str(&json_str).map_err(|e| e.to_string())?;

    let online_players = v["players"]["online"].as_u64().unwrap_or(0) as u32;
    let max_players = v["players"]["max"].as_u64().unwrap_or(0) as u32;
    let motd = v["description"]["text"].as_str().unwrap_or("AstralForge").to_string();

    Ok(ServerStatus {
        online: true,
        ping_ms: ping_duration.max(1),
        online_players,
        max_players,
        motd,
    })
}
