use std::time::{Duration, SystemTime};

use receiver_core::pairing::{
    EnrollmentAction as Action, EnrollmentEvent as Event, EnrollmentPhase as Phase,
};

const NOW: SystemTime = SystemTime::UNIX_EPOCH;
const B: &str = "restricted credential";
const R: &str = "receiver credential";

#[test]
fn only_an_uncertain_original_attempt_can_retransmit_before_expiry() {
    let expires = NOW + Duration::from_secs(1);
    assert_eq!(
        Phase::Attempt0(B).advance(Event::Start, NOW, expires),
        Action::Redeem(0, Phase::Attempt0(B))
    );
    for event in [Event::Uncertain, Event::Resume] {
        assert_eq!(
            Phase::Attempt0(B).advance(event, NOW, expires),
            Action::Redeem(1, Phase::Attempt1(B))
        );
    }
    for event in [Event::Start, Event::Uncertain, Event::Resume] {
        assert_eq!(
            Phase::Attempt0(B).advance(event, expires, expires),
            Action::Fail
        );
    }
    for event in [Event::Uncertain, Event::Resume] {
        assert_eq!(
            Phase::Attempt1(B).advance(event, NOW, expires),
            Action::Fail
        );
    }
    assert_eq!(
        Phase::Attempt0(B).advance(Event::Rejected, NOW, expires),
        Action::Fail
    );
}

#[test]
fn only_the_issued_credential_can_connect() {
    for phase in [Phase::Attempt0(B), Phase::Attempt1(B)] {
        assert_eq!(
            phase.advance(Event::Issued(R), NOW, NOW),
            Action::Connect(Phase::Issued(R))
        );
    }
    assert_eq!(
        Phase::Issued(R).advance(Event::Resume, NOW, NOW),
        Action::Connect(Phase::Issued(R))
    );
}
