//! Current-user Named Pipe ownership and listener. No UI access.
use super::{
    windows_io::Pipe,
    windows_security::{current_sid, verify_process_user, PrivateSecurity},
    Inbound,
};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::ERROR_ACCESS_DENIED, System::Pipes::GetNamedPipeClientProcessId,
};

#[derive(Clone, Debug)]
pub struct Endpoint {
    pub socket: PathBuf,
    sid: String,
}
impl Endpoint {
    /// Connect to the current user's server with bounded overlapped I/O.
    pub fn connect(
        &self,
        read_timeout: Duration,
        write_timeout: Duration,
    ) -> io::Result<impl io::Read + io::Write> {
        let mut pipe = Pipe::client(&self.socket.to_string_lossy()).map_err(|error| {
            if error.raw_os_error() == Some(windows_sys::Win32::Foundation::ERROR_PIPE_BUSY as i32)
            {
                io::Error::new(io::ErrorKind::WouldBlock, error)
            } else {
                error
            }
        })?;
        let mut pid = 0;
        if unsafe {
            windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(pipe.raw(), &mut pid)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        verify_process_user(pid, &self.sid)?;
        pipe.read_timeout = read_timeout;
        pipe.write_timeout = write_timeout;
        Ok(pipe)
    }
    pub fn from_env() -> io::Result<Self> {
        let directory = std::env::var_os("ZIPZI_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("zipzi-runtime"));
        let profile = std::env::var("ZIPZI_RUNTIME_PROFILE").unwrap_or_else(|_| "default".into());
        Self::new(&directory, &profile)
    }
    pub fn new(directory: &Path, profile: &str) -> io::Result<Self> {
        if !directory.is_absolute()
            || profile.is_empty()
            || profile.len() > 24
            || !profile
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid absolute runtime directory or profile",
            ));
        }
        let sid = current_sid()?;
        let directory = directory
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        let digest = Sha256::digest(directory.trim_end_matches('/').as_bytes());
        let scope: String = digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Ok(Self {
            socket: PathBuf::from(format!(r"\\.\pipe\zipzi-{sid}-{scope}-{profile}")),
            sid,
        })
    }
    pub fn claim(&self) -> io::Result<Option<Owner>> {
        let security = PrivateSecurity::new(&self.sid)?;
        match Pipe::server(&self.socket.to_string_lossy(), &security, true) {
            Ok(pipe) => Ok(Some(Owner {
                endpoint: self.clone(),
                pipe,
            })),
            Err(error) if error.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32) => Ok(None),
            Err(error) => Err(error),
        }
    }
    pub fn inherit_or_claim(&self) -> io::Result<Option<Owner>> {
        self.claim()
    }
}

pub struct Owner {
    endpoint: Endpoint,
    pipe: Pipe,
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
        let (sender, receiver) = async_channel::bounded(16);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let accept = thread::Builder::new()
            .name("zipzi-pipe-listener".into())
            .spawn(move || {
                let mut pipe = self.pipe;
                let security = match PrivateSecurity::new(&self.endpoint.sid) {
                    Ok(value) => value,
                    Err(_) => return,
                };
                let mut connections: Vec<thread::JoinHandle<()>> = Vec::new();
                while !stopped.load(Ordering::Relaxed) {
                    connections.retain(|connection| !connection.is_finished());
                    match pipe.accept(Duration::from_millis(100)) {
                        Err(error) if error.kind() == io::ErrorKind::TimedOut => continue,
                        Err(_) => break,
                        Ok(()) => {}
                    }
                    // Keep one instance alive continuously so first-instance claim
                    // cannot succeed during the accept-to-dispatch handoff.
                    let next = match Pipe::server(
                        &self.endpoint.socket.to_string_lossy(),
                        &security,
                        false,
                    ) {
                        Ok(next) => next,
                        Err(_) => break,
                    };
                    let connected = std::mem::replace(&mut pipe, next);
                    let mut pid = 0;
                    if unsafe { GetNamedPipeClientProcessId(connected.raw(), &mut pid) } == 0
                        || verify_process_user(pid, &self.endpoint.sid).is_err()
                        || connections.len() >= 8
                    {
                        continue;
                    }
                    let sender = sender.clone();
                    let stopped = stopped.clone();
                    let budget = budget.clone();
                    match thread::Builder::new()
                        .name("zipzi-pipe-client".into())
                        .spawn(move || super::serve(connected, sender, stopped, budget))
                    {
                        Ok(thread) => connections.push(thread),
                        Err(_) => break,
                    }
                }
                // Closing the receiver in Server::drop unblocks dispatch sends.
                for connection in connections {
                    let _ = connection.join();
                }
            })?;
        Ok((
            Server {
                stop,
                accept: Some(accept),
                receiver: receiver.clone(),
            },
            receiver,
        ))
    }
}
pub struct Server {
    stop: Arc<AtomicBool>,
    accept: Option<thread::JoinHandle<()>>,
    receiver: async_channel::Receiver<Inbound>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.receiver.close();
        if let Some(thread) = self.accept.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn client_timeout_before_listener_does_not_release_owner() {
        let folder = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::new(folder.path(), "early").unwrap();
        let owner = endpoint.claim().unwrap().unwrap();
        drop(Pipe::client(&endpoint.socket.to_string_lossy()).unwrap());
        let (server, messages) = owner.listen().unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        let mut client = loop {
            assert!(endpoint.claim().unwrap().is_none());
            match Pipe::client(&endpoint.socket.to_string_lossy()) {
                Ok(client) => break client,
                Err(error) => {
                    assert!(std::time::Instant::now() < deadline, "{error}");
                    thread::sleep(Duration::from_millis(10));
                }
            }
        };
        super::super::write_frame(&mut client, &serde_json::json!({"probe": 2})).unwrap();
        match messages.recv_blocking().unwrap() {
            Inbound::Frame { value, .. } => assert_eq!(value["probe"], 2),
            _ => panic!("expected frame"),
        }
        drop(client);
        drop(server);
    }
    #[test]
    fn listener_dispatches_real_frame_and_releases_ownership() {
        let folder = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::new(folder.path(), "test").unwrap();
        let owner = endpoint.claim().unwrap().unwrap();
        assert!(endpoint.claim().unwrap().is_none());
        let (server, messages) = owner.listen().unwrap();
        let mut client = endpoint
            .connect(Duration::from_secs(1), Duration::from_secs(1))
            .unwrap();
        super::super::write_frame(&mut client, &serde_json::json!({"probe": 1})).unwrap();
        let message = messages.recv_blocking().unwrap();
        match message {
            Inbound::Frame { value, .. } => assert_eq!(value["probe"], 1),
            _ => panic!("expected frame"),
        }
        drop(client);
        drop(server);
        assert!(endpoint.claim().unwrap().is_some());
    }
}
