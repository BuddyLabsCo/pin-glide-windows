use std::ptr;

use windows_sys::Win32::{
    Foundation::{GlobalFree, HWND},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
            OpenClipboard, SetClipboardData,
        },
        Diagnostics::Debug::ReadProcessMemory,
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        Ole::CF_UNICODETEXT,
        Threading::GetCurrentProcess,
    },
};

#[derive(Debug)]
pub(crate) enum ClipboardWrite {
    Written,
    Busy,
    Failed,
}

pub(crate) fn write_unicode_clipboard(owner: HWND, text: &str) -> ClipboardWrite {
    let text = zeroize::Zeroizing::new(super::wide(text));
    // SAFETY: transfer the movable allocation to Windows only if SetClipboardData succeeds.
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, size_of_val(text.as_slice()));
        if handle.is_null() {
            return ClipboardWrite::Failed;
        }
        let data = GlobalLock(handle).cast::<u16>();
        if data.is_null() {
            GlobalFree(handle);
            return ClipboardWrite::Failed;
        }
        ptr::copy_nonoverlapping(text.as_ptr(), data, text.len());
        GlobalUnlock(handle);
        if OpenClipboard(owner) == 0 {
            GlobalFree(handle);
            return ClipboardWrite::Busy;
        }
        let written =
            EmptyClipboard() != 0 && !SetClipboardData(u32::from(CF_UNICODETEXT), handle).is_null();
        CloseClipboard();
        if written {
            ClipboardWrite::Written
        } else {
            GlobalFree(handle);
            ClipboardWrite::Failed
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClipboardError {
    Unavailable,
    InvalidText,
}

pub(crate) fn read_unicode_clipboard() -> Result<Option<String>, ClipboardError> {
    let format = u32::from(CF_UNICODETEXT);
    // SAFETY: keep the clipboard open and its memory locked through the bounded copy.
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return Err(ClipboardError::Unavailable);
        }
        let result = if IsClipboardFormatAvailable(format) == 0 {
            Ok(None)
        } else {
            let handle = GetClipboardData(format);
            let data = GlobalLock(handle).cast::<u16>();
            if data.is_null() {
                Err(ClipboardError::Unavailable)
            } else {
                let result = copy_text(data, GlobalSize(handle)).map(Some);
                GlobalUnlock(handle);
                result
            }
        };
        CloseClipboard();
        result
    }
}

fn copy_text(data: *const u16, bytes: usize) -> Result<String, ClipboardError> {
    let mut units = Vec::new();
    let capacity = bytes / size_of::<u16>();
    if capacity == 0 {
        return Err(ClipboardError::InvalidText);
    }
    units
        .try_reserve_exact(capacity)
        .map_err(|_| ClipboardError::Unavailable)?;
    units.resize(capacity, 0_u16);
    let bytes = size_of_val(units.as_slice());
    let mut copied = 0;
    // SAFETY: Windows checks the source address and copies into the initialized destination.
    let success = unsafe {
        ReadProcessMemory(
            GetCurrentProcess(),
            data.cast(),
            units.as_mut_ptr().cast(),
            bytes,
            &mut copied,
        )
    };
    if success == 0 || copied != bytes {
        return Err(ClipboardError::Unavailable);
    }
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or(ClipboardError::InvalidText)?;
    String::from_utf16(&units[..end]).map_err(|_| ClipboardError::InvalidText)
}

#[cfg(test)]
mod tests {
    use super::{ClipboardError, copy_text};

    #[test]
    fn copies_and_validates_bounded_unicode_text() {
        let valid = [0x529e, 0xd83d, 0xde80, 0, 0xd800];
        assert_eq!(
            copy_text(valid.as_ptr(), size_of_val(&valid)).as_deref(),
            Ok("办🚀")
        );
        assert_eq!(
            copy_text(valid.as_ptr(), size_of_val(&valid[..3])),
            Err(ClipboardError::InvalidText)
        );
        let invalid = [0xd800, 0];
        assert_eq!(
            copy_text(invalid.as_ptr(), size_of_val(&invalid)),
            Err(ClipboardError::InvalidText)
        );
    }
}
