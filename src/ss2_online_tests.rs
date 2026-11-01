//! Integration tests launch a digest-verified private COMMGR SS2 process only.
//! Observations are not atomic PLC scan snapshots or physical PLC evidence.
use super::*;

fn row(snapshot: &OnlineSnapshot, device: &str, kind: &str) -> u32 {
    let device: Device = device.parse().unwrap();
    snapshot
        .rows
        .iter()
        .find(|r| r.device == device && r.kind == kind)
        .unwrap()
        .value
}

#[test]
#[ignore = "requires installed digest-verified COMMGR 2.11 SS2 simulator"]
fn online_run_poll_stop_restart_and_reupload() {
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let source = "LD X0\nOUT Y0\nLD X0\nTMR T0 K1\nEND";
    session.upload(source).unwrap();
    let trace_start = session.trace().len();
    let before_start = Instant::now();
    let initial = session.start_online().unwrap();
    let after_start = Instant::now();
    assert!(initial.observed_at >= before_start && initial.observed_at <= after_start);
    std::thread::sleep(Duration::from_millis(30));
    assert!(initial.observed_at.elapsed() >= Duration::from_millis(30));
    assert!(initial.running);
    for address in [0x0c0fu16, STEP_MODE] {
        let mut request = address.to_be_bytes().to_vec();
        request.extend([0, 1]);
        assert_eq!(session.request(1, &request).unwrap()[1] & 1, 0);
    }
    assert_eq!(row(&initial, "Y0", "bit"), 0);
    assert!(
        initial
            .rows
            .iter()
            .any(|r| r.device.to_string() == "T0" && r.kind == "word")
    );
    assert!(session.step(&[]).is_err());
    assert!(session.read_program_words().is_err());
    assert!(session.discover_program_words().is_err());
    let x0: Device = "X0".parse().unwrap();
    let on_start = Instant::now();
    let mut on = session.poll_online(&[(x0, true)]).unwrap();
    assert!(on.observed_at >= on_start && on.observed_at <= Instant::now());
    let deadline = Instant::now() + Duration::from_secs(3);
    while row(&on, "Y0", "bit") != 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        on = session.poll_online(&[]).unwrap();
    }
    assert_eq!(row(&on, "X0", "bit"), 1);
    assert_eq!(row(&on, "Y0", "bit"), 1);
    eprintln!("first X0=1 to observed Y0=1: {:?}", on_start.elapsed());
    let mut off = session.poll_online(&[(x0, false)]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while row(&off, "Y0", "bit") != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        off = session.poll_online(&[]).unwrap();
    }
    assert_eq!(row(&off, "Y0", "bit"), 0);
    assert!(off.sequence > initial.sequence);
    let stopped = session.stop_online().unwrap();
    assert!(!stopped.running);
    assert_eq!(row(&stopped, "Y0", "bit"), 0);
    let expected = es2_encoder::encode(&plc::compile(source).unwrap())
        .unwrap()
        .words;
    assert_eq!(session.discover_program_words().unwrap(), expected);
    assert_eq!(session.read_program_words().unwrap(), expected);
    assert!(session.poll_online(&[]).is_err());
    assert!(session.step(&[]).is_err());
    assert!(
        !session.trace()[trace_start..]
            .iter()
            .any(|t| t.function == 5 && t.request.starts_with(&STEP.to_be_bytes()))
    );
    let restarted = session.start_online().unwrap();
    assert!(restarted.running);
    assert!(restarted.sequence > stopped.sequence);
    session.stop_online().unwrap();
    session.upload(source).unwrap();
    assert_eq!(session.step(&[]).unwrap().mismatches, 0);
}

#[test]
#[ignore = "requires installed digest-verified COMMGR 2.11 SS2 simulator"]
fn online_transport_loss_retires_owned_running_process() {
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    session.upload("LD X0\nOUT Y0\nEND").unwrap();
    session.start_online().unwrap();
    let invalid: Device = "Y0".parse().unwrap();
    assert!(session.poll_online(&[(invalid, true)]).is_err());
    assert!(session.online_active && !session.faulted);
    let native = session.native.as_mut().unwrap();
    native._child.0.as_mut().unwrap().kill().unwrap();
    native._child.0.as_mut().unwrap().wait().unwrap();
    assert!(session.poll_online(&[]).is_err());
    assert!(session.faulted && !session.online_active && session.native.is_none());
}

#[test]
#[ignore = "requires installed digest-verified COMMGR 2.11 SS2 simulator"]
fn async_bridge_online_operations_are_serialized() {
    use std::{
        future::Future,
        task::{Context, Poll, Wake, Waker},
    };
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::park(),
            }
        }
    }
    block_on(async {
        let bridge = launch(DEFAULT_SS2_SIMULATOR.to_string()).await.unwrap();
        assert!(bridge.supports_online());
        bridge.upload("LD X0\nOUT Y0\nEND".into()).await.unwrap();
        assert!(bridge.start_online().await.unwrap().running);
        let x0: Device = "X0".parse().unwrap();
        let mut snapshot = bridge.poll_online(vec![(x0, true)]).await.unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while row(&snapshot, "Y0", "bit") != 1 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            snapshot = bridge.poll_online(vec![]).await.unwrap();
        }
        assert_eq!(row(&snapshot, "Y0", "bit"), 1);
        assert!(!bridge.stop_online().await.unwrap().running);
        assert!(bridge.step(vec![]).await.is_err());
        bridge.disconnect();
    });
}
