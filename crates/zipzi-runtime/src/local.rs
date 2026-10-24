//! Unix-only transport and endpoint ownership. Framing/threads have no UI access.
use crate::{
    protocol::{failure, MAX_FRAME},
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
impl Owner {
    pub fn listen(self) -> io::Result<(Server, async_channel::Receiver<Inbound>)> {
        // The exclusive lock proves no cooperating host can own this socket.
        match fs::symlink_metadata(&self.endpoint.socket) {
            Ok(m) if m.file_type().is_socket() => {
                fs::remove_file(&self.endpoint.socket)?;
            }
            Ok(_) => return Err(invalid("Refusing to remove a non-socket endpoint")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        let listener = UnixListener::bind(&self.endpoint.socket)?;
        fs::set_permissions(&self.endpoint.socket, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
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
                            let rollback = count.clone();
                            if thread::Builder::new()
                                .name("zipzi-connection".into())
                                .spawn(move || {
                                    serve(stream, sender, stopped);
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
        reply: mpsc::SyncSender<Value>,
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
fn serve(mut stream: UnixStream, sender: async_channel::Sender<Inbound>, stop: Arc<AtomicBool>) {
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
        let value = match read_frame(&mut stream) {
            Ok(value) => value,
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(_) => break,
        };
        let (tx, rx) = mpsc::sync_channel(1);
        if sender
            .send_blocking(Inbound::Frame {
                auth,
                value,
                reply: tx,
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
            if let Some(ok) = response.get("ok") {
                auth = serde_json::from_value(ok["client"].clone())
                    .ok()
                    .zip(ok["connection"].as_u64())
                    .map(|(client, connection)| Auth { client, connection });
            }
        }
        if let Err(error) = write_frame(&mut stream, &response) {
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
    let mut header = [0; 4];
    // Only an idle timeout before any header byte is safe to retry.
    reader.read_exact(&mut header[..1])?;
    reader.read_exact(&mut header[1..]).map_err(partial)?;
    let length = u32::from_be_bytes(header) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid frame length",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).map_err(partial)?;
    serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}
fn partial(e: io::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("Incomplete frame: {e}"))
}
pub fn write_frame(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
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
