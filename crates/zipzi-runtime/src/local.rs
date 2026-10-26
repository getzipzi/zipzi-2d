//! Unix-only transport and endpoint ownership. Framing/threads have no UI access.
use crate::{
    protocol::{failure, BinaryReply, MAX_BINARY_CHUNK, MAX_BINARY_HEADER, MAX_FRAME},
    ClientId,
};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub socket: PathBuf,
    pub lock: PathBuf,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
impl Endpoint {
    pub fn from_env() -> io::Result<Self> {
        // SAFETY: getuid has no preconditions.
        let uid = unsafe { libc::getuid() };
        let dir = std::env::var_os("ZIPZI_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/tmp/zipzi-runtime-{uid}")));
        let profile = std::env::var("ZIPZI_RUNTIME_PROFILE").unwrap_or_else(|_| "default".into());
        Self::new(&dir, &profile)
    }
    pub fn new(dir: &Path, profile: &str) -> io::Result<Self> {
        if !dir.is_absolute()
            || profile.is_empty()
            || profile.len() > 24
            || !profile
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        {
            return Err(invalid("Invalid absolute runtime directory or profile"));
        }
        match fs::DirBuilder::new().mode(0o700).create(dir) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        let meta = fs::symlink_metadata(dir)?;
        if !meta.is_dir() || meta.uid() != unsafe { libc::getuid() } || meta.mode() & 0o077 != 0 {
            return Err(invalid(
                "Runtime directory must be owned by this user, private and not a symlink",
            ));
        }
        let socket = dir.join(format!("{profile}.sock"));
        if socket.as_os_str().len() >= 104 {
            return Err(invalid("Runtime socket path is too long"));
        }
        Ok(Self {
            socket,
            lock: dir.join(format!("{profile}.lock")),
        })
    }
    /// Never unlink a lock file: its inode identifies ownership across launchers.
    pub fn claim(&self) -> io::Result<Option<Owner>> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&self.lock)?;
        self.claim_file(file)
    }
    pub fn inherit_or_claim(&self) -> io::Result<Option<Owner>> {
        if let Ok(fd) = std::env::var("ZIPZI_RUNTIME_OWNER_FD") {
            let fd: i32 = fd
                .parse()
                .map_err(|_| invalid("Invalid inherited owner fd"))?;
            if fd < 3 {
                return Err(invalid("Invalid inherited owner fd"));
            }
            // SAFETY: ownership of this explicitly passed fd is transferred by
            // the launcher; no other Rust object owns it in the new process.
            let file = unsafe { File::from_raw_fd(fd) };
            let expected = fs::symlink_metadata(&self.lock)?;
            let actual = file.metadata()?;
            if expected.dev() != actual.dev() || expected.ino() != actual.ino() {
                return Err(invalid("Inherited owner does not identify this endpoint"));
            }
            self.claim_file(file)
        } else {
            self.claim()
        }
    }
    fn claim_file(&self, file: File) -> io::Result<Option<Owner>> {
        let meta = file.metadata()?;
        if !meta.is_file() || meta.uid() != unsafe { libc::getuid() } || meta.mode() & 0o077 != 0 {
            return Err(invalid("Unsafe endpoint lock"));
        }
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let e = io::Error::last_os_error();
            return if e.kind() == io::ErrorKind::WouldBlock {
                Ok(None)
            } else {
                Err(e)
            };
        }
        // Ensure children cannot retain the owner through an unrelated exec.
        unsafe {
            libc::fcntl(file.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC);
        }
        Ok(Some(Owner {
            endpoint: self.clone(),
            _file: file,
        }))
    }
}
use std::os::unix::fs::DirBuilderExt;
pub struct Owner {
    endpoint: Endpoint,
    _file: File,
}
fn bind_private_listener(endpoint: &Path) -> io::Result<UnixListener> {
    // Publish the pathname only once its inode has private permissions. Bind
    // followed by chmod at the public path lets strict cold-start clients see
    // the temporary umask-derived permissions and reject a legitimate host.
    // Six bytes fit every pathname accepted by Endpoint::new ("a.sock").
    for _ in 0..32 {
        let name = uuid::Uuid::new_v4().simple().to_string();
        let staging = endpoint.with_file_name(format!(".{}", &name[..5]));
        let listener = match UnixListener::bind(&staging) {
            Ok(listener) => listener,
            Err(error) if error.kind() == io::ErrorKind::AddrInUse => continue,
            Err(error) => return Err(error),
        };
        let publish = (|| {
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o600))?;
            listener.set_nonblocking(true)?;
            // A hard link atomically installs the same socket inode and fails
            // if a file has appeared at the endpoint; never overwrite it.
            fs::hard_link(&staging, endpoint)?;
            Ok(listener)
        })();
        let _ = fs::remove_file(&staging);
        return publish;
    }
    Err(io::Error::new(
        io::ErrorKind::AddrInUse,
        "Private socket staging names occupied",
    ))
}
impl Owner {
    pub fn listen(self) -> io::Result<(Server, async_channel::Receiver<Inbound>)> {
        let budget = zipzi_project::ImageMemoryBudget::new(usize::MAX);
        budget.bind_host_budget(0, 2 << 30);
        self.listen_with_budget(budget)
    }
    pub fn listen_with_budget(
        self,
        budget: zipzi_project::ImageMemoryBudget,
    ) -> io::Result<(Server, async_channel::Receiver<Inbound>)> {
        // The exclusive lock proves no cooperating host can own this socket.
        match fs::symlink_metadata(&self.endpoint.socket) {
            Ok(m) if m.file_type().is_socket() => {
                fs::remove_file(&self.endpoint.socket)?;
            }
            Ok(_) => return Err(invalid("Refusing to remove a non-socket endpoint")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        let listener = bind_private_listener(&self.endpoint.socket)?;
        let (sender, receiver) = async_channel::bounded(16);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let accept = thread::Builder::new()
            .name("zipzi-listener".into())
            .spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            if count.load(Ordering::Relaxed) >= 8 {
                                continue;
                            }
                            count.fetch_add(1, Ordering::Relaxed);
                            let count = count.clone();
                            let sender = sender.clone();
                            let stopped = stopped.clone();
                            let budget = budget.clone();
                            let rollback = count.clone();
                            if thread::Builder::new()
                                .name("zipzi-connection".into())
                                .spawn(move || {
                                    serve(stream, sender, stopped, budget);
                                    count.fetch_sub(1, Ordering::Relaxed);
                                })
                                .is_err()
                            {
                                rollback.fetch_sub(1, Ordering::Relaxed);
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok((
            Server {
                owner: self,
                stop,
                accept: Some(accept),
            },
            receiver,
        ))
    }
}
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
pub struct Server {
    owner: Owner,
    stop: Arc<AtomicBool>,
    accept: Option<thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        let _ = fs::remove_file(&self.owner.endpoint.socket);
    }
}
fn serve(
    mut stream: UnixStream,
    sender: async_channel::Sender<Inbound>,
    stop: Arc<AtomicBool>,
    budget: zipzi_project::ImageMemoryBudget,
) {
    // accept inherits O_NONBLOCK on macOS; partial frames must use blocking IO.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
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
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_private_socket_accepts_connections_without_staging_remnants() {
        let folder = tempfile::tempdir_in("/tmp").unwrap();
        let endpoint = folder.path().join("test.sock");
        let listener = bind_private_listener(&endpoint).unwrap();
        let meta = fs::symlink_metadata(&endpoint).unwrap();
        assert!(meta.file_type().is_socket());
        assert_eq!(meta.mode() & 0o777, 0o600);
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1);
        let mut client = UnixStream::connect(&endpoint).unwrap();
        let (mut peer, _) = listener.accept().unwrap();
        client.write_all(b"private").unwrap();
        let mut payload = [0; 7];
        peer.read_exact(&mut payload).unwrap();
        assert_eq!(&payload, b"private");
    }

    #[test]
    fn failed_private_socket_publication_preserves_existing_file_and_cleans_staging() {
        let folder = tempfile::tempdir_in("/tmp").unwrap();
        let endpoint = folder.path().join("test.sock");
        fs::write(&endpoint, b"owned content").unwrap();
        assert!(bind_private_listener(&endpoint).is_err());
        assert_eq!(fs::read(&endpoint).unwrap(), b"owned content");
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[test]
    fn private_socket_staging_fits_the_longest_accepted_endpoint() {
        let folder = tempfile::tempdir_in("/tmp").unwrap();
        let extra = 103 - folder.path().as_os_str().len() - 1 - 1 - "a.sock".len();
        let endpoint = Endpoint::new(&folder.path().join("x".repeat(extra)), "a").unwrap();
        assert_eq!(endpoint.socket.as_os_str().len(), 103);
        let listener = bind_private_listener(&endpoint.socket).unwrap();
        let _client = UnixStream::connect(&endpoint.socket).unwrap();
        listener.accept().unwrap();
    }
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
