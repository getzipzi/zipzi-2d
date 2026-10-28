//! Win32 token identities and current-user-only kernel object descriptors.
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::{io, mem, ptr};
use windows_sys::Win32::{
    Foundation::{LocalFree, HANDLE},
    Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    },
    Security::{GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER},
    System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    },
};

pub(super) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn token_sid(process: HANDLE) -> io::Result<String> {
    let mut token = ptr::null_mut();
    // SAFETY: process is a live handle, token is writable, and the returned
    // token handle is transferred immediately to its RAII owner.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            ptr::null_mut(),
            0,
            &mut length,
        );
    }
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    // TOKEN_USER contains pointers: align the buffer to pointer alignment.
    let mut data = vec![0usize; (length as usize).div_ceil(mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            data.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*data.as_ptr().cast::<TOKEN_USER>() };
    let mut text = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let allocation = LocalAllocation(text.cast());
    let mut count = 0;
    unsafe {
        while *text.add(count) != 0 {
            count += 1;
        }
    }
    let result = String::from_utf16(unsafe { std::slice::from_raw_parts(text, count) })
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
    drop(allocation);
    result
}

pub(super) fn current_sid() -> io::Result<String> {
    token_sid(unsafe { GetCurrentProcess() })
}

pub(super) fn verify_process_user(pid: u32, expected: &str) -> io::Result<()> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    if token_sid(process.as_raw_handle())? != expected {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Pipe peer belongs to another user",
        ));
    }
    Ok(())
}

struct LocalAllocation(*mut std::ffi::c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

pub(super) struct PrivateSecurity(LocalAllocation);
impl PrivateSecurity {
    pub fn new(sid: &str) -> io::Result<Self> {
        let sddl = wide(&format!("O:{sid}D:P(A;;GA;;;{sid})"));
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(LocalAllocation(descriptor)))
    }

    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0 .0,
            bInheritHandle: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::{GetSecurityDescriptorDacl, IsValidSecurityDescriptor};

    #[test]
    fn real_token_identity_and_private_descriptor() {
        let sid = current_sid().unwrap();
        assert!(sid.starts_with("S-1-"));
        verify_process_user(std::process::id(), &sid).unwrap();
        assert!(verify_process_user(std::process::id(), "S-1-0-0").is_err());
        let security = PrivateSecurity::new(&sid).unwrap();
        let attributes = security.attributes();
        assert_eq!(attributes.bInheritHandle, 0);
        assert_ne!(
            unsafe { IsValidSecurityDescriptor(attributes.lpSecurityDescriptor) },
            0
        );
        let mut present = 0;
        let mut defaulted = 0;
        let mut acl = ptr::null_mut();
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(
                    attributes.lpSecurityDescriptor,
                    &mut present,
                    &mut acl,
                    &mut defaulted,
                )
            },
            0
        );
        assert_ne!(present, 0);
        assert!(!acl.is_null());
        assert_eq!(unsafe { (*acl).AceCount }, 1);
    }
}
