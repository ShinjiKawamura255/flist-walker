//! Stage-A controls only: no collector, native display claim, or numeric policy.
//! Guard order: request/root/source identity, result oracle, retained state,
//! work/allocation contract, terminal/owner cleanup. UNKNOWN is never success.

#[derive(Debug)]
pub(super) struct ActivationIntent {
    id: u64,
    target: u64,
    t0: u64,
    last_attempt: u64,
    attempts: usize,
    canceled: bool,
}

impl ActivationIntent {
    pub(super) fn new(id: u64, target: u64, t0: u64) -> Self {
        Self {
            id,
            target,
            t0,
            last_attempt: t0,
            attempts: 1,
            canceled: false,
        }
    }

    pub(super) fn origin(&self) -> Option<(u64, u64, u64)> {
        (!self.canceled).then_some((self.id, self.target, self.t0))
    }

    pub(super) fn retry(&mut self, target: u64, at: u64) -> bool {
        if self.canceled || target != self.target || at < self.last_attempt {
            return false;
        }
        self.last_attempt = at;
        self.attempts += 1;
        true
    }

    fn coalesce(&mut self, target: u64, at: u64) -> bool {
        self.retry(target, at)
    }
    fn cancel(&mut self) {
        self.canceled = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Guard {
    Pass,
    Fail,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    NewIndex,
    Retained,
    CancelCleanup,
}

#[derive(Clone, Debug)]
struct Observation {
    scope: Scope,
    guards: [Guard; 5],
    // Each immutable first-model/frame receipt is independently checked.
    // Actual bounded receipt capture and source/work schema belong to B0.
    first_model_receipt: Option<Guard>,
    first_frame_receipt: Option<Guard>,
    later_frame_receipts: Vec<Guard>,
    clock_valid: bool,
    timed_out: bool,
    normal_exit: bool,
    target_requests: usize,
    t0: Option<u64>,
    send_window: Option<(u64, u64)>,
    worker: Option<u64>,
    source_started: Option<u64>,
    first_model: Option<u64>,
    first_frame: Option<u64>,
    terminal: Option<u64>,
    cleanup: Option<u64>,
    reference_valid: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Admission {
    CorrectnessFail,
    Indeterminate,
    ObservationValid,
    CancelCleanup,
}

fn admit(row: &Observation) -> Admission {
    // Candidate violations dominate unknown instrumentation or a bad reference.
    if row.guards.contains(&Guard::Fail)
        || row.first_model_receipt == Some(Guard::Fail)
        || row.first_frame_receipt == Some(Guard::Fail)
        || row.later_frame_receipts.contains(&Guard::Fail)
        || matches!(row.scope, Scope::NewIndex) && row.target_requests != 1
        || matches!(row.scope, Scope::Retained) && row.target_requests != 0
    {
        return Admission::CorrectnessFail;
    }
    if row.guards.contains(&Guard::Unknown)
        || row.first_model_receipt == Some(Guard::Unknown)
        || row.first_frame_receipt == Some(Guard::Unknown)
        || row.later_frame_receipts.contains(&Guard::Unknown)
        || !row.clock_valid
        || row.timed_out
        || !row.normal_exit
        || !row.reference_valid
    {
        return Admission::Indeterminate;
    }
    let (Some(t0), Some(cleanup)) = (row.t0, row.cleanup) else {
        return Admission::Indeterminate;
    };
    if cleanup < t0 {
        return Admission::Indeterminate;
    }
    if row.scope == Scope::CancelCleanup {
        return if row.terminal.is_some_and(|t| t >= t0 && t <= cleanup) {
            Admission::CancelCleanup
        } else {
            Admission::Indeterminate
        };
    }
    let (Some(model), Some(frame)) = (row.first_model, row.first_frame) else {
        return Admission::Indeterminate;
    };
    if row.first_model_receipt.is_none()
        || row.first_frame_receipt.is_none()
        || model < t0
        || frame < model
        || cleanup < frame
    {
        return Admission::Indeterminate;
    }
    if row.scope == Scope::NewIndex {
        let (Some((send, returned)), Some(worker), Some(started), Some(terminal)) = (
            row.send_window,
            row.worker,
            row.source_started,
            row.terminal,
        ) else {
            return Admission::Indeterminate;
        };
        // send return and dequeue/worker are concurrent. Do not require return<=worker.
        if send < t0
            || returned < send
            || cleanup < returned
            || worker < send
            || started < worker
            || model < started
            || terminal < started
            || cleanup < terminal
        {
            return Admission::Indeterminate;
        }
    } else if row.send_window.is_some()
        || row.worker.is_some()
        || row.source_started.is_some()
        || row.terminal.is_some()
    {
        // Retained restoration cannot invent an indexing start/completion.
        return Admission::Indeterminate;
    }
    Admission::ObservationValid
}

#[test]
fn tc_234_activation_contract_initial_origin_survives_retry_and_coalesced_input() {
    let mut intent = ActivationIntent::new(1, 7, 10);
    assert!(intent.retry(7, 30));
    assert!(intent.coalesce(7, 40));
    assert_eq!(intent.origin(), Some((1, 7, 10)));
    assert_eq!(intent.attempts, 3);
    assert!(!intent.retry(8, 50));
    assert_eq!(intent.origin(), Some((1, 7, 10)));
    intent.cancel();
    assert_eq!(intent.origin(), None);
    assert!(!intent.retry(7, 60));
    assert!(!intent.coalesce(7, 60));
    let next = ActivationIntent::new(2, 8, 70);
    assert_eq!(next.origin(), Some((2, 8, 70)));
}

#[test]
fn tc_234_activation_contract_rejects_backdated_attempt_without_resetting_origin() {
    let mut intent = ActivationIntent::new(1, 7, 10);
    assert!(intent.retry(7, 30));
    assert!(!intent.coalesce(7, 20));
    assert_eq!(intent.origin(), Some((1, 7, 10)));
    assert_eq!(intent.attempts, 2);
}

fn complete_control() -> Observation {
    Observation {
        scope: Scope::NewIndex,
        guards: [Guard::Pass; 5],
        first_model_receipt: Some(Guard::Pass),
        first_frame_receipt: Some(Guard::Pass),
        later_frame_receipts: vec![Guard::Pass],
        clock_valid: true,
        timed_out: false,
        normal_exit: true,
        target_requests: 1,
        t0: Some(10),
        send_window: Some((20, 40)),
        worker: Some(25), // A worker may start BEFORE successful send returns.
        source_started: Some(45),
        first_model: Some(50),
        first_frame: Some(60),
        terminal: Some(55), // Producer completion can precede the first frame.
        cleanup: Some(70),
        reference_valid: true,
    }
}

#[test]
fn tc_234_activation_contract_healthy_controls_are_observational_without_numeric_pass() {
    let new = complete_control();
    assert_eq!(admit(&new), Admission::ObservationValid);
    let mut retained = new.clone();
    retained.scope = Scope::Retained;
    retained.target_requests = 0;
    retained.send_window = None;
    retained.worker = None;
    retained.source_started = None;
    retained.terminal = None;
    assert_eq!(admit(&retained), Admission::ObservationValid);
    let mut canceled = new;
    canceled.scope = Scope::CancelCleanup;
    canceled.worker = None;
    canceled.source_started = None;
    canceled.first_model = None;
    canceled.first_frame = None;
    canceled.first_model_receipt = None;
    canceled.first_frame_receipt = None;
    canceled.later_frame_receipts.clear();
    assert_eq!(admit(&canceled), Admission::CancelCleanup);
    // Admission deliberately has no numeric/UX PASS variant or threshold input.
}

#[test]
fn tc_234_activation_contract_rejects_started_only_missing_clocks_and_false_success() {
    let control = complete_control();
    for missing in 0..8 {
        let mut row = control.clone();
        match missing {
            0 => row.t0 = None,
            1 => row.worker = None,
            2 => row.terminal = None,
            3 => row.first_model = None,
            4 => row.first_frame = None,
            5 => row.cleanup = None,
            6 => row.send_window = None,
            _ => row.source_started = None,
        }
        assert_eq!(
            admit(&row),
            Admission::Indeterminate,
            "missing phase {missing}"
        );
    }
    for malformed in 0..9 {
        let mut row = control.clone();
        match malformed {
            0 => row.clock_valid = false,
            1 => row.timed_out = true,
            2 => row.normal_exit = false,
            3 => row.worker = Some(0), // missing-time zero filling
            4 => row.send_window = Some((40, 20)),
            5 => row.first_frame = Some(49),
            6 => row.cleanup = Some(59),
            7 => row.first_frame_receipt = None,
            _ => row.reference_valid = false,
        }
        assert_eq!(
            admit(&row),
            Admission::Indeterminate,
            "malformed {malformed}"
        );
    }
    for guard in 0..5 {
        let mut row = control.clone();
        row.guards[guard] = Guard::Unknown;
        assert_eq!(admit(&row), Admission::Indeterminate);
        row.guards[guard] = Guard::Fail;
        row.reference_valid = false;
        row.worker = None;
        assert_eq!(admit(&row), Admission::CorrectnessFail);
    }
}

#[test]
fn tc_234_activation_contract_rejects_unnecessary_reindex_and_transient_wrong_frame() {
    let mut row = complete_control();
    row.scope = Scope::Retained;
    assert_eq!(admit(&row), Admission::CorrectnessFail);
    row = complete_control();
    row.target_requests = 2;
    assert_eq!(admit(&row), Admission::CorrectnessFail);
    row = complete_control();
    row.first_frame_receipt = Some(Guard::Fail); // correct sentinel, wrong other result/selection
    assert_eq!(row.later_frame_receipts.last(), Some(&Guard::Pass));
    assert_eq!(admit(&row), Admission::CorrectnessFail);
    row.first_frame_receipt = Some(Guard::Unknown); // overflow / absent immutable receipt
    assert_eq!(admit(&row), Admission::Indeterminate);
}

#[test]
fn tc_234_activation_contract_missing_first_receipt_cannot_be_replaced_by_later_success() {
    let mut row = complete_control();
    row.first_frame_receipt = Some(Guard::Fail);
    assert_eq!(admit(&row), Admission::CorrectnessFail);
    row.first_frame_receipt = None; // lost initial incorrect frame, later frame still healthy
    assert_eq!(row.later_frame_receipts, vec![Guard::Pass]);
    assert_eq!(admit(&row), Admission::Indeterminate);
    row = complete_control();
    row.first_model_receipt = None;
    assert_eq!(admit(&row), Admission::Indeterminate);
}
