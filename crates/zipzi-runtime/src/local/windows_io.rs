//! Bounded overlapped byte-pipe IO. Cancellation is drained before buffers drop.
use super::windows_security::{wide, PrivateSecurity};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::{
    io::{self, Read, Write},
    mem, ptr,
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*, IO::*},
};

pub(super) struct Pipe {
    handle: OwnedHandle,
    pub read_timeout: Duration,
    pub write_timeout: Duration,
}

impl Pipe {
    fn from_handle(handle: HANDLE) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: unsafe { OwnedHandle::from_raw_handle(handle) },
            read_timeout: Duration::from_secs(1),
            write_timeout: Duration::from_secs(5),
        })
    }

    pub fn server(name: &str, security: &PrivateSecurity, first: bool) -> io::Result<Self> {
        let name = wide(name);
        let flags = PIPE_ACCESS_DUPLEX
            | FILE_FLAG_OVERLAPPED
            | if first {
                FILE_FLAG_FIRST_PIPE_INSTANCE
            } else {
                0
            };
        let attributes = security.attributes();
        Self::from_handle(unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                16,
                65536,
                65536,
                0,
                &attributes,
            )
        })
    }

    pub fn client(name: &str) -> io::Result<Self> {
        let name = wide(name);
        Self::from_handle(unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                ptr::null_mut(),
            )
        })
    }

    pub fn raw(&self) -> HANDLE {
        self.handle.as_raw_handle()
    }

    fn operation(
        &self,
        timeout: Duration,
        connect: bool,
        start: impl FnOnce(*mut OVERLAPPED) -> i32,
    ) -> io::Result<usize> {
        let event = unsafe { CreateEventW(ptr::null(), 1, 0, ptr::null()) };
        if event.is_null() {
            return Err(io::Error::last_os_error());
        }
        let event = unsafe { OwnedHandle::from_raw_handle(event) };
        let mut overlapped: OVERLAPPED = unsafe { mem::zeroed() };
        overlapped.hEvent = event.as_raw_handle();
        if start(&mut overlapped) == 0 {
            let error = io::Error::last_os_error();
            if connect && error.raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32) {
                return Ok(0);
            }
            if error.raw_os_error() != Some(ERROR_IO_PENDING as i32) {
                return Err(error);
            }
            let wait = unsafe {
                WaitForSingleObject(
                    event.as_raw_handle(),
                    timeout.as_millis().clamp(1, u32::MAX as u128 - 1) as u32,
                )
            };
            if wait != WAIT_OBJECT_0 {
                let error = if wait == WAIT_TIMEOUT {
                    io::Error::new(io::ErrorKind::TimedOut, "Named Pipe operation timed out")
                } else {
                    io::Error::last_os_error()
                };
                // Cancellation is asynchronous. Always reap before freeing OVERLAPPED
                // or allowing the caller's borrowed read/write buffer to disappear.
                let mut ignored = 0;
                unsafe {
                    CancelIoEx(self.raw(), &overlapped);
                    GetOverlappedResult(self.raw(), &overlapped, &mut ignored, 1);
                }
                return Err(error);
            }
        }
        let mut transferred = 0;
        if unsafe { GetOverlappedResult(self.raw(), &overlapped, &mut transferred, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(transferred as usize)
    }

    pub fn accept(&self, timeout: Duration) -> io::Result<()> {
        let result = self.operation(timeout, true, |overlapped| unsafe {
            ConnectNamedPipe(self.raw(), overlapped)
        });
        if result
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(ERROR_NO_DATA as i32))
        {
            // A client can connect and time out before the host starts accepting.
            // Recycle this instance without relinquishing endpoint ownership.
            unsafe {
                DisconnectNamedPipe(self.raw());
            }
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Client left before accept",
            ));
        }
        result.map(|_| ())
    }
}

impl Read for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.operation(self.read_timeout, false, |overlapped| unsafe {
            ReadFile(
                self.raw(),
                bytes.as_mut_ptr(),
                bytes.len().min(u32::MAX as usize) as u32,
                ptr::null_mut(),
                overlapped,
            )
        })
    }
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.operation(self.write_timeout, false, |overlapped| unsafe {
            WriteFile(
                self.raw(),
                bytes.as_ptr(),
                bytes.len().min(u32::MAX as usize) as u32,
                ptr::null_mut(),
                overlapped,
            )
        })
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::windows_security::current_sid;
    #[test]
    fn blocked_writer_times_out_and_cancels_pending_buffer() {
        let name = format!(r"\\.\pipe\zipzi-test-{}", uuid::Uuid::new_v4());
        let security = PrivateSecurity::new(&current_sid().unwrap()).unwrap();
        let server = Pipe::server(&name, &security, true).unwrap();
        let mut client = Pipe::client(&name).unwrap();
        server.accept(Duration::from_secs(1)).unwrap();
        client.write_timeout = Duration::from_millis(25);
        let payload = vec![7; 2 << 20];
        assert_eq!(
            client.write_all(&payload).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        drop(payload);
        drop(client);
        drop(server);
    }

    #[test]
    fn private_pipe_timeout_cancels_then_reuses_buffer_and_detects_disconnect() {
        let name = format!(r"\\.\pipe\zipzi-test-{}", uuid::Uuid::new_v4());
        let security = PrivateSecurity::new(&current_sid().unwrap()).unwrap();
        let mut server = Pipe::server(&name, &security, true).unwrap();
        assert!(Pipe::server(&name, &security, true).is_err());
        let mut client = Pipe::client(&name).unwrap();
        server.accept(Duration::from_secs(1)).unwrap();
        server.read_timeout = Duration::from_millis(25);
        let mut bytes = [0; 5];
        assert_eq!(
            server.read(&mut bytes).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        client.write_all(b"hello").unwrap();
        server.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"hello");
        server.write_all(b"reply").unwrap();
        client.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"reply");
        drop(client);
        assert!(server.read(&mut bytes).is_err());
    }
}
