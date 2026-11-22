use std::{
    ffi::c_void,
    ptr::{null, null_mut},
    sync::atomic::{AtomicU64, Ordering},
};

use windows_sys::Win32::{
    Foundation::{HANDLE, HWND},
    NetworkManagement::IpHelper::{CancelMibChangeNotify2, NotifyNetworkConnectivityHintChange},
    Networking::WinSock::{
        NL_NETWORK_CONNECTIVITY_HINT,
        NetworkConnectivityLevelHintConstrainedInternetAccess as CONSTRAINED_ACCESS,
        NetworkConnectivityLevelHintInternetAccess as INTERNET_ACCESS,
        NetworkConnectivityLevelHintLocalAccess as LOCAL_ACCESS,
    },
    System::RemoteDesktop::{
        NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
    },
};

#[cfg(test)]
mod tests;

static POPUP_EPOCH: AtomicU64 = AtomicU64::new(0);
static CONNECTIVITY_RANK: AtomicU64 = AtomicU64::new(0);

pub(super) struct SessionNotifications {
    window: HWND,
    registered: bool,
    network: HANDLE,
}

impl SessionNotifications {
    pub(super) fn new(window: HWND) -> Self {
        session_changed(false);
        let mut network = null_mut();
        CONNECTIVITY_RANK.store(0, Ordering::Relaxed);
        #[expect(
            clippy::undocumented_unsafe_blocks,
            reason = "live HWND and writable registration handle"
        )]
        unsafe {
            NotifyNetworkConnectivityHintChange(
                Some(network_changed),
                null(),
                true,
                &raw mut network,
            );
        }
        Self {
            window,
            registered: false,
            network,
        }
    }

    pub(super) fn register_session(&mut self) -> bool {
        #[expect(
            clippy::undocumented_unsafe_blocks,
            reason = "window is owned by the tray thread"
        )]
        let registered = self.registered
            || unsafe { WTSRegisterSessionNotification(self.window, NOTIFY_FOR_THIS_SESSION) != 0 };
        if registered && !self.registered {
            session_changed(super::popup::session_unlocked());
        }
        self.registered = registered;
        registered
    }
}

impl Drop for SessionNotifications {
    fn drop(&mut self) {
        #[expect(
            clippy::undocumented_unsafe_blocks,
            reason = "only this monitor owns the registrations"
        )]
        unsafe {
            if self.registered {
                WTSUnRegisterSessionNotification(self.window);
            }
            if !self.network.is_null() {
                CancelMibChangeNotify2(self.network);
            }
        }
        session_changed(super::popup::session_unlocked());
    }
}

unsafe extern "system" fn network_changed(_: *const c_void, hint: NL_NETWORK_CONNECTIVITY_HINT) {
    let rank = match hint.ConnectivityLevel {
        INTERNET_ACCESS => 3,
        CONSTRAINED_ACCESS => 2,
        LOCAL_ACCESS => 1,
        _ => 0,
    };
    if CONNECTIVITY_RANK.swap(rank, Ordering::Relaxed) < rank {
        crate::service::connectivity_improved();
    }
}

pub(super) fn session_changed(unlocked: bool) {
    let _ = POPUP_EPOCH.try_update(Ordering::AcqRel, Ordering::Acquire, |previous| {
        Some((previous & !1).wrapping_add(2) | u64::from(!unlocked))
    });
    if !unlocked {
        crate::ui::close_sms();
    }
}

pub(crate) fn popup_epoch() -> Option<u64> {
    let epoch = POPUP_EPOCH.load(Ordering::Acquire);
    (epoch & 1 == 0
        && super::popup::session_unlocked()
        && POPUP_EPOCH.load(Ordering::Acquire) == epoch)
        .then_some(epoch)
}
