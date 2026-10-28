//! Transport-independent local framing, authentication and connection dispatch.
use crate::{
    protocol::{failure, BinaryReply, MAX_BINARY_CHUNK, MAX_BINARY_HEADER, MAX_FRAME},
    ClientId,
};
use serde_json::Value;
use std::{
    io::{self, Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{Endpoint, Owner, Server};

#[cfg(windows)]
#[allow(dead_code)] // Connected to the host in the next transport integration step.
mod windows_io;
#[cfg(windows)]
#[allow(dead_code)] // Used by the Named Pipe implementation as it is brought up.
mod windows_security;

#[derive(Clone, Copy, Debug)]
pub struct Auth {
    pub client: ClientId,
    pub connection: u64,
}
pub enum Inbound {
    Frame {
        auth: Option<Auth>,
        value: Value,
        reply: mpsc::SyncSender<crate::protocol::EncodedReply>,
        _transfer: zipzi_project::TransferReservation,
    },
    Binary {
        auth: Auth,
        value: Value,
        bytes: Vec<u8>,
        reply: mpsc::SyncSender<BinaryReply>,
        _transfer: zipzi_project::TransferReservation,
    },
    Disconnected(Auth),
}
fn serve(
    mut stream: impl Read + Write,
    sender: async_channel::Sender<Inbound>,
    stop: Arc<AtomicBool>,
    budget: zipzi_project::ImageMemoryBudget,
) {
    let mut auth = None;
    // Incremental frame reads keep idle sockets alive while bounding partial
    // message time and memory. A sender cannot keep a half-frame forever.
    while !stop.load(Ordering::Relaxed) {
        let message = match read_message(&mut stream, &budget) {
            Ok(value) => value,
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        };
        let (value, transfer) = match message {
            Message::Control(value, transfer) => (value, transfer),
            Message::Binary {
                value,
                bytes,
                transfer,
            } => {
                let Some(auth) = auth else {
                    break;
                };
                let (reply, response) = mpsc::sync_channel(1);
                if sender
                    .send_blocking(Inbound::Binary {
                        auth,
                        value,
                        bytes,
                        reply,
                        _transfer: transfer,
                    })
                    .is_err()
                {
                    break;
                }
                let response = loop {
                    match response.recv_timeout(Duration::from_secs(1)) {
                        Ok(value) => break Some(value),
                        Err(mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Relaxed) => {
                            continue;
                        }
                        _ => break None,
                    }
                };
                let Some(response) = response else {
                    break;
                };
                if write_binary_reply(&mut stream, &response).is_err() {
                    break;
                }
                continue;
            }
        };
        let (tx, rx) = mpsc::sync_channel(1);
        if sender
            .send_blocking(Inbound::Frame {
                auth,
                value,
                reply: tx,
                _transfer: transfer,
            })
            .is_err()
        {
            break;
        }
        let response = loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(value) => break Some(value),
                Err(mpsc::RecvTimeoutError::Timeout) if !stop.load(Ordering::Relaxed) => continue,
                _ => break None,
            }
        };
        let Some(response) = response else {
            break;
        };
        if auth.is_none() {
            auth = response.authenticated;
        }
        if let Err(error) = stream
            .write_all(&(response.bytes.len() as u32).to_be_bytes())
            .and_then(|_| stream.write_all(&response.bytes))
        {
            eprintln!("Runtime response write failed: {error}");
            break;
        }
        if auth.is_none() {
            break;
        }
    }
    if let Some(auth) = auth {
        let _ = sender.send_blocking(Inbound::Disconnected(auth));
    }
}
/// Big-endian u32 byte length followed by one UTF-8 JSON document. No allocation
/// occurs for invalid/oversized lengths. Nonfinite numbers are rejected by serde.
pub fn read_frame(reader: &mut impl Read) -> io::Result<Value> {
    let budget = zipzi_project::ImageMemoryBudget::default();
    match read_message(reader, &budget)? {
        Message::Control(value, _) => Ok(value),
        Message::Binary { .. } => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Expected control frame",
        )),
    }
}
enum Message {
    Control(Value, zipzi_project::TransferReservation),
    Binary {
        value: Value,
        bytes: Vec<u8>,
        transfer: zipzi_project::TransferReservation,
    },
}
fn read_message(
    reader: &mut impl Read,
    budget: &zipzi_project::ImageMemoryBudget,
) -> io::Result<Message> {
    let mut header = [0; 4];
    // Only an idle timeout before any header byte is safe to retry.
    reader.read_exact(&mut header[..1])?;
    reader.read_exact(&mut header[1..]).map_err(partial)?;
    let raw = u32::from_be_bytes(header);
    let binary = raw & (1 << 31) != 0;
    let length = (raw & !(1 << 31)) as usize;
    let limit = if binary {
        MAX_BINARY_CHUNK + MAX_BINARY_HEADER + 4
    } else {
        MAX_FRAME
    };
    if length == 0 || length > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid frame length",
        ));
    }
    if binary {
        reader.read_exact(&mut header).map_err(partial)?;
        let json_length = u32::from_be_bytes(header) as usize;
        if json_length == 0
            || json_length > MAX_BINARY_HEADER
            || json_length.checked_add(4).is_none_or(|n| n > length)
            || length - json_length - 4 > MAX_BINARY_CHUNK
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid binary frame bounds",
            ));
        }
        let transfer = budget
            .reserve_transfer(
                json_length
                    .saturating_mul(129)
                    .saturating_add((length - json_length - 4).saturating_mul(2)),
            )
            .map_err(|e| io::Error::other(e.message))?;
        let mut json = vec![0; json_length];
        reader.read_exact(&mut json).map_err(partial)?;
        let value = crate::metrics::decode(&json)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut bytes = vec![0; length - json_length - 4];
        reader.read_exact(&mut bytes).map_err(partial)?;
        return Ok(Message::Binary {
            value,
            bytes,
            transfer,
        });
    }
    let transfer = budget
        .reserve_transfer(length.saturating_mul(129))
        .map_err(|e| io::Error::other(e.message))?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(partial)?;
    crate::metrics::decode(&body)
        .map(|value| Message::Control(value, transfer))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
pub fn write_binary_reply(writer: &mut impl Write, reply: &BinaryReply) -> io::Result<()> {
    let header = crate::metrics::encode(
        &serde_json::json!({"response": reply.value, "has_payload": reply.payload.is_some()}),
    )?;
    if header.len() > MAX_BINARY_HEADER {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Binary response header limit",
        ));
    }
    let bytes = reply.payload.as_ref().map_or(&[][..], |v| v.bytes());
    let length = 4 + header.len() + bytes.len();
    writer.write_all(&((length as u32) | (1 << 31)).to_be_bytes())?;
    writer.write_all(&(header.len() as u32).to_be_bytes())?;
    writer.write_all(&header)?;
    for chunk in bytes.chunks(4096) {
        writer.write_all(chunk)?;
    }
    writer.flush()
}
fn partial(e: io::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("Incomplete frame: {e}"))
}
pub fn write_frame(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = crate::metrics::encode(value)?;
    if body.len() > MAX_FRAME {
        return write_frame(
            writer,
            &failure("BUDGET_EXCEEDED", "Response exceeds control-frame budget"),
        );
    }
    writer.write_all(&(body.len() as u32).to_be_bytes())?;
    // macOS AF_UNIX can reject a single write larger than SO_SNDBUF.
    for chunk in body.chunks(4096) {
        writer.write_all(chunk)?;
    }
    writer.flush()
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{Endpoint, Owner, Server};
