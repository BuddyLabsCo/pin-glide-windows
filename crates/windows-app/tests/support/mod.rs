use std::thread;
use std::time::{Duration, Instant};

pub fn wait_for(mut condition: impl FnMut() -> bool, expectation: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "{expectation}");
        thread::sleep(Duration::from_millis(25));
    }
}
