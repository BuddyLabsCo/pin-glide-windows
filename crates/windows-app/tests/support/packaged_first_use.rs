use std::{
    ffi::OsString,
    fs, io,
    io::BufReader,
    net::TcpListener,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::HWND as AutomationWindow,
    System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    },
    UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationInvokePattern, TreeScope_Descendants,
        UIA_InvokePatternId,
    },
};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};

use super::{AppProcess, launch_duplicate, visible_window, wait_for};

#[path = "pairing.rs"]
#[allow(
    unused_imports,
    reason = "the shared fixture also exports Sender helpers"
)]
mod fixture;

#[test]
#[ignore = "requires an interactive disposable Windows profile and a local HTTPS package build"]
fn packaged_reinstall_enrolls_only_after_explicit_start() {
    assert_eq!(
        std::env::var("PINGLIDE_DISPOSABLE_PROFILE").as_deref(),
        Ok("1"),
        "run only in a disposable profile following the Windows packaging README"
    );
    let origin = reqwest::Url::parse(&std::env::var("PINGLIDE_SERVICE_URL").unwrap()).unwrap();
    assert_eq!(origin.scheme(), "https");
    assert_eq!(origin.host_str(), Some("localhost"));
    let listener = TcpListener::bind(("127.0.0.1", origin.port().unwrap())).unwrap();
    listener.set_nonblocking(true).unwrap();

    let profile = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap());
    let data = profile.join("Pin Glide");
    let installation = profile.join("Programs").join("Pin Glide");
    assert!(
        !data.exists() && !installation.exists(),
        "the disposable profile must begin without Pin Glide"
    );
    let setup = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/packaging/windows/Pin-Glide-Setup-x64-unsigned.exe");
    let cleanup = InstalledPackage {
        uninstaller: installation.join("Uninstall.exe"),
        scratch: tempfile::tempdir().unwrap(),
    };
    run_setup(&setup);
    fs::create_dir(&data).unwrap();
    fs::write(data.join("registration.json"), "previous Receiver fixture").unwrap();
    assert!(cleanup.uninstall().unwrap().success());
    assert!(
        !installation.exists() && !data.exists(),
        "uninstall must remove the old Receiver"
    );
    run_setup(&setup);
    assert!(!data.exists(), "reinstall must begin without local state");
    let receiver = installation.join("windows-app.exe");
    let mut primary = AppProcess(Command::new(&receiver).arg("--startup").spawn().unwrap());
    let pid = primary.0.id();
    wait_for(
        || tray_exists(pid),
        "the packaged Receiver must open its tray",
    );
    assert_quiet(&listener, || {
        assert!(
            visible_window(pid).is_null(),
            "startup must stay in the tray"
        );
        assert!(primary.0.try_wait().unwrap().is_none());
    });

    launch_duplicate(
        &mut Command::new(receiver),
        "explicit second launch must exit successfully",
    );
    wait_for(
        || !visible_window(pid).is_null(),
        "explicit launch must open Welcome in the tray process",
    );
    assert_quiet(&listener, || {});
    let welcome = AutomationWindow(visible_window(pid));
    let service = thread::spawn(move || {
        let mut connection = BufReader::new(fixture::accept(&fixture::acceptor(), &listener));
        let (request, headers, body) = fixture::read_request_parts(&mut connection);
        assert_eq!(request, "POST /v1/pairing-sessions HTTP/1.1\r\n");
        assert!(headers.iter().all(|header| {
            !header
                .split_once(':')
                .unwrap()
                .0
                .eq_ignore_ascii_case("authorization")
        }));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"mode": "new_receiver"})
        );
    });
    start_pairing(welcome).unwrap();
    service.join().unwrap();
    drop(primary);
    assert!(cleanup.uninstall().unwrap().success());
    assert!(
        !installation.exists() && !data.exists(),
        "final uninstall must remove the fixture Receiver"
    );
}

fn run_setup(path: &Path) {
    assert!(
        Command::new(path).arg("/S").status().unwrap().success(),
        "setup failed: {}",
        path.display()
    );
}

struct InstalledPackage {
    uninstaller: PathBuf,
    scratch: tempfile::TempDir,
}

impl InstalledPackage {
    fn uninstall(&self) -> io::Result<ExitStatus> {
        let copy = self.scratch.path().join("Uninstall.exe");
        fs::copy(&self.uninstaller, &copy)?;
        let mut installation = OsString::from("_?=");
        installation.push(self.uninstaller.parent().unwrap());
        Command::new(copy).arg("/S").raw_arg(installation).status()
    }
}

impl Drop for InstalledPackage {
    fn drop(&mut self) {
        if self.uninstaller.is_file() {
            let _ = self.uninstall();
        }
    }
}

fn tray_exists(process: u32) -> bool {
    let class: Vec<u16> = "BuddyLabsCo.PinGlide.Tray"
        .encode_utf16()
        .chain([0])
        .collect();
    let mut owner = 0;
    // SAFETY: class is NUL-terminated and owner is writable.
    unsafe {
        let tray = FindWindowW(class.as_ptr(), std::ptr::null());
        !tray.is_null() && GetWindowThreadProcessId(tray, &mut owner) != 0 && owner == process
    }
}

fn assert_quiet(listener: &TcpListener, mut verify_window: impl FnMut()) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        verify_window();
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock),
            "the Receiver contacted the Service before Start"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn start_pairing(window: AutomationWindow) -> windows::core::Result<()> {
    // SAFETY: COM is initialized on this test thread for the duration of the automation call.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let result = invoke_start(window);
        CoUninitialize();
        result
    }
}

fn invoke_start(window: AutomationWindow) -> windows::core::Result<()> {
    // SAFETY: COM is initialized and the observed HWND belongs to the fixture's live Receiver.
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
        let welcome = automation.ElementFromHandle(window)?;
        wait_for(
            || {
                let children = welcome
                    .FindAll(
                        TreeScope_Descendants,
                        &automation.CreateTrueCondition().unwrap(),
                    )
                    .unwrap();
                for index in 0..children.Length().unwrap() {
                    let element = children.GetElement(index).unwrap();
                    let name = element.CurrentName().unwrap();
                    if name == "Get started" || name == "开始使用" {
                        let start: IUIAutomationInvokePattern =
                            element.GetCurrentPatternAs(UIA_InvokePatternId).unwrap();
                        start.Invoke().unwrap();
                        return true;
                    }
                }
                false
            },
            "Welcome must expose the Start action",
        );
    }
    Ok(())
}
