use receiver_core::delivery::Sms;
use slint::ComponentHandle;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE,
    WTS_SESSION_LOCK, WTS_SESSION_UNLOCK,
};

use crate::{platform, startup_tests::select_test_backend, ui};

#[test]
#[ignore = "requires an unlocked interactive Windows desktop"]
fn lock_immediately_closes_every_sms_and_unlock_only_shows_new_sms() {
    select_test_backend().unwrap();
    let name: Vec<_> = "Pin Glide session fixture"
        .encode_utf16()
        .chain([0])
        .collect();
    let tray = platform::Tray::new(&name).unwrap();
    let handle = tray.window() as usize;
    slint::spawn_local(async move {
        let first = show("First synthetic SMS").await.unwrap();
        let second = show("Second synthetic SMS").await.unwrap();
        assert!(first.window().is_visible() && second.window().is_visible());
        let signal = |message, event| {
            // SAFETY: the test owns the live tray window on the current thread.
            unsafe { SendMessageW(handle as _, message, event as usize, 0) };
        };
        signal(WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK);
        for popup in [&first, &second] {
            assert!(!popup.window().is_visible());
            assert!(popup.get_sms().is_empty() && popup.get_otp().is_empty());
        }
        assert!(show("SMS received during lock").await.is_none());
        signal(WM_POWERBROADCAST, PBT_APMSUSPEND);
        signal(WM_POWERBROADCAST, PBT_APMRESUMEAUTOMATIC);
        assert!(show("SMS received on resume while locked").await.is_none());
        signal(WM_WTSSESSION_CHANGE, WTS_SESSION_UNLOCK);
        assert!(!first.window().is_visible() && !second.window().is_visible());
        assert!(
            show("First SMS after unlock")
                .await
                .unwrap()
                .window()
                .is_visible()
        );
        ui::close_sms();
        slint::quit_event_loop().unwrap();
    })
    .unwrap();
    slint::run_event_loop_until_quit().unwrap();
    drop(tray);
}

async fn show(body: &str) -> Option<crate::SmsWindow> {
    ui::show_sms(
        Sms {
            body: body.into(),
            otp: "123456".into(),
        },
        "Fixture phone".into(),
        false,
    )
    .await
    .unwrap()
}
