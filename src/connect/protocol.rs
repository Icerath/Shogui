use std::{
    io::{self, ErrorKind},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use base64::prelude::*;
use iced::futures::channel::{mpsc, oneshot};
use petty_shogi::Action;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::net::{TcpListener, TcpStream};

use super::{GameSettings, Message};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Packet {
    PlayMove(Action),
    CloseConnection,
    Rejected,
}

macro_rules! try_local {
    ($expr:expr) => {
        match $expr {
            Ok(t) => t,
            Err(e) => return Message::LocalError(Arc::new(e)),
        }
    };
}

macro_rules! try_remote {
    ($expr:expr) => {
        match $expr {
            Ok(t) => t,
            Err(e) => return Message::RemoteError(Arc::new(e)),
        }
    };
}

pub async fn join(ip: IpAddr, port: u16, key: u64) -> Message {
    let stream = try_local!(TcpStream::connect((ip, port)).await);
    try_remote!(stream.writable().await);
    try_remote!(stream.try_write(&key.to_le_bytes()));
    let settings = try_remote!(recv_inner::<GameSettings>(&stream).await);
    Message::Connected(Arc::new(stream), (ip, port).into(), Some(settings))
}

pub async fn host(
    mut cancel: oneshot::Sender<()>,
    ip: IpAddr,
    port: u16,
    key: u64,
    settings: GameSettings,
) -> Message {
    tokio::select! {
        () = cancel.cancellation() => Message::None,
        msg = host_inner(ip, port, key, settings) => msg,
    }
}

pub async fn send(stream: Arc<TcpStream>, packet: Packet) -> Message {
    send_inner(&stream, &packet).await
}

pub async fn send_inner<P: Serialize>(stream: &TcpStream, packet: &P) -> Message {
    let json = try_local!(serde_json::to_string(packet).map_err(Into::into));
    let len: u32 = try_local!(json.len().try_into().map_err(io::Error::other));
    try_remote!(write_all(stream, &len.to_le_bytes()).await);
    try_remote!(write_all(stream, json.as_bytes()).await);
    eprintln!("Sent {json}");
    Message::None
}

pub async fn recv(
    stream: Arc<TcpStream>,
    mut sender: mpsc::Sender<Message>,
    mut cancel: oneshot::Sender<()>,
) {
    loop {
        tokio::select! {
            message = recv_inner::<Packet>(&stream) => {
                match message {
                    // TODO: handle this properly
                    Ok(message) => sender.try_send(Message::Recv(message)).unwrap(),
                    Err(e) => {
                        eprintln!("{e:?}");
                        sender.try_send(Message::CloseConnection).unwrap();
                        break;
                    }
                }
            }
            () = cancel.cancellation() => break,
        };
    }
}

pub async fn recv_inner<P: DeserializeOwned>(stream: &TcpStream) -> io::Result<P> {
    let mut len_bytes = [0u8; 4];
    read_exact(stream, &mut len_bytes).await?;
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut buf = vec![0u8; len];
    read_exact(stream, &mut buf).await?;
    eprintln!("Received {}", std::str::from_utf8(&buf).unwrap());
    Ok(serde_json::from_slice(&buf)?)
}

async fn write_all(stream: &TcpStream, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.writable().await?;
        match stream.try_write(bytes) {
            Ok(n) => bytes = &bytes[n..],
            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

async fn read_exact(stream: &TcpStream, mut bytes: &mut [u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        stream.readable().await?;
        match stream.try_read(bytes) {
            Ok(0) => return Err(ErrorKind::UnexpectedEof.into()),
            Ok(n) => bytes = &mut bytes[n..],
            Err(err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

async fn host_inner(ip: IpAddr, port: u16, key: u64, settings: GameSettings) -> Message {
    let listener = try_local!(TcpListener::bind((ip, port)).await);
    loop {
        let (stream, addr) = try_local!(listener.accept().await);
        if try_host(&stream, key).await {
            send_inner(&stream, &settings).await;
            return Message::Connected(Arc::new(stream), addr, None);
        }
    }
}

async fn try_host(stream: &TcpStream, expected_key: u64) -> bool {
    let mut buf = [0u8; 8];
    let mut total_read = 0;
    while total_read < 8 {
        let Ok(()) = stream.readable().await else { return false };
        match stream.try_read(&mut buf) {
            Ok(0) | Err(_) => return false,
            Ok(bytes_read) => total_read += bytes_read,
        }
    }
    let key = u64::from_le_bytes(buf);
    key == expected_key
}

const INVITE_VERSION: u8 = 0;

pub fn encode_invite(ip: IpAddr, port: u16, key: u64) -> String {
    let mut invite = vec![];
    invite.push(INVITE_VERSION);
    match ip {
        IpAddr::V4(addr) => {
            invite.push(0);
            invite.extend(addr.octets());
        }
        IpAddr::V6(addr) => {
            invite.push(1);
            invite.extend(addr.octets());
        }
    }
    invite.extend(port.to_le_bytes());
    invite.extend(key.to_le_bytes());
    BASE64_STANDARD.encode(invite)
}

pub fn decode_invite(rest: &[u8]) -> Option<(IpAddr, u16, u64)> {
    let rest = BASE64_STANDARD.decode(rest).ok()?;
    let (version, rest) = rest.split_at_checked(1)?;
    if version != [0] {
        return None;
    }
    let (ip_kind, rest) = rest.split_at_checked(1)?;
    let (ip, rest) = match ip_kind {
        [0] => {
            let (ip, rest) = rest.split_at_checked(4)?;
            let ip = Ipv4Addr::from_octets(ip.try_into().unwrap());
            (IpAddr::V4(ip), rest)
        }
        [1] => {
            let (ip, rest) = rest.split_at_checked(16)?;
            let ip = Ipv6Addr::from_octets(ip.try_into().unwrap());
            (IpAddr::V6(ip), rest)
        }
        _ => return None,
    };
    let (port, key) = rest.split_at(2);
    let port = u16::from_le_bytes(port.try_into().unwrap());
    let key = u64::from_le_bytes(key.try_into().ok()?);
    Some((ip, port, key))
}
