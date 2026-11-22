use std::{ptr, slice};

use windows_sys::Win32::System::{
    DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard},
    Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    Ole::CF_UNICODETEXT,
};

pub(crate) enum ClipboardError {
    Unavailable,
    InvalidText,
}

pub(crate) fn read_unicode_clipboard() -> Result<Option<String>, ClipboardError> {
    let format = u32::from(CF_UNICODETEXT);
    // SAFETY: read only while the clipboard is open, and keep its memory locked through decoding.
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
                let capacity = GlobalSize(handle) / size_of::<u16>();
                let result = (0..capacity)
                    .find(|&index| data.add(index).read() == 0)
                    .ok_or(ClipboardError::InvalidText)
                    .and_then(|length| {
                        String::from_utf16(slice::from_raw_parts(data, length))
                            .map(Some)
                            .map_err(|_| ClipboardError::InvalidText)
                    });
                GlobalUnlock(handle);
                result
            }
        };
        CloseClipboard();
        result
    }
}
