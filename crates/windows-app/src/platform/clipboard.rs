use std::{ptr, slice};

use windows_sys::Win32::System::{
    DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
    Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    Ole::CF_UNICODETEXT,
};

pub(crate) fn unicode_clipboard_is_valid() -> bool {
    // SAFETY: the clipboard stays open and the handle stays locked while its buffer is borrowed.
    unsafe {
        if OpenClipboard(ptr::null_mut()) == 0 {
            return false;
        }
        let handle = GetClipboardData(u32::from(CF_UNICODETEXT));
        let mut valid = true;
        if !handle.is_null() {
            let data = GlobalLock(handle).cast::<u16>();
            valid = !data.is_null();
            if valid {
                let count = GlobalSize(handle) / size_of::<u16>();
                let units = slice::from_raw_parts(data, count);
                valid = char::decode_utf16(units.iter().copied().take_while(|&unit| unit != 0))
                    .all(|character| character.is_ok());
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        valid
    }
}
