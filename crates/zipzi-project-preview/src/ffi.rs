//! C ABI shared by the Quick Look and Windows Shell frontends.
use super::*;
use std::ffi::c_void;

type ReadAt = unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> isize;
struct Source {
    context: *mut c_void,
    read: ReadAt,
    length: u64,
    position: u64,
}
impl Read for Source {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let count = bytes
            .len()
            .min(self.length.saturating_sub(self.position) as usize);
        if count == 0 {
            return Ok(0);
        }
        let actual = unsafe { (self.read)(self.context, self.position, bytes.as_mut_ptr(), count) };
        if actual < 0 || actual as usize > count {
            return Err(io::Error::other("preview source read"));
        }
        self.position += actual as u64;
        Ok(actual as usize)
    }
}
impl Seek for Source {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(v) => i128::from(v),
            SeekFrom::End(v) => i128::from(self.length) + i128::from(v),
            SeekFrom::Current(v) => i128::from(self.position) + i128::from(v),
        };
        if !(0..=i128::from(self.length)).contains(&position) {
            return Err(io::Error::other("preview source seek"));
        }
        self.position = position as u64;
        Ok(self.position)
    }
}

/// Returns PNG length, 0 when absent, -1 for invalid input. No pointers escape.
/// # Safety
/// The callback must remain valid for this call and write at most `count` bytes.
/// `output` must address `capacity` writable bytes disjoint from callback storage.
#[no_mangle]
pub unsafe extern "C" fn zipzi_preview_read(
    context: *mut c_void,
    read: Option<ReadAt>,
    length: u64,
    output: *mut u8,
    capacity: usize,
) -> i64 {
    std::panic::catch_unwind(|| {
        let Some(read) = read else {
            return -1;
        };
        if output.is_null() || capacity > MAX_PREVIEW_BYTES {
            return -1;
        }
        match read_preview(Source {
            context,
            read,
            length,
            position: 0,
        }) {
            Ok(Some(preview)) if preview.png().len() <= capacity => {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        preview.png().as_ptr(),
                        output,
                        preview.png().len(),
                    )
                };
                preview.png().len() as i64
            }
            Ok(None) => 0,
            _ => -1,
        }
    })
    .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe extern "C" fn short_read(_: *mut c_void, _: u64, _: *mut u8, _: usize) -> isize {
        -1
    }
    #[test]
    fn invalid_callback_inputs_return_errors_without_unwinding() {
        let mut output = [0; 16];
        assert_eq!(
            unsafe {
                zipzi_preview_read(
                    std::ptr::null_mut(),
                    Some(short_read),
                    100,
                    output.as_mut_ptr(),
                    output.len(),
                )
            },
            -1
        );
        assert_eq!(
            unsafe {
                zipzi_preview_read(
                    std::ptr::null_mut(),
                    None,
                    100,
                    output.as_mut_ptr(),
                    output.len(),
                )
            },
            -1
        );
        assert_eq!(
            unsafe {
                zipzi_preview_read(
                    std::ptr::null_mut(),
                    Some(short_read),
                    100,
                    std::ptr::null_mut(),
                    0,
                )
            },
            -1
        );
    }
}
