//! Unix-only transport and endpoint ownership. Framing/threads have no UI access.
use super::Inbound;
use std::{
    fs::{self, File, OpenOptions},
    io,
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
        Arc,
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
    /// Connect with the same bounded client interface as the Windows transport.
    pub fn connect(
        &self,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> io::Result<impl io::Read + io::Write> {
        let stream = UnixStream::connect(&self.socket)?;
        stream.set_read_timeout(Some(read_timeout))?;
        stream.set_write_timeout(Some(write_timeout))?;
        Ok(stream)
    }
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
    stream: UnixStream,
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
    super::serve(stream, sender, stop, budget);
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

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
