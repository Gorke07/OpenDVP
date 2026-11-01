//! Opt-in test against a separately owned installed SS2 simulator process.
use crate::es2_bridge::{DEFAULT_SS2_SIMULATOR, Session, Target};
use crate::plc::Device;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn repeated_scans_observe_native_io_and_independent_ladder_state() {
    let mut session = Session::launch(
        Path::new(DEFAULT_SS2_SIMULATOR),
        Arc::new(AtomicBool::new(false)),
    )
    .expect("launch isolated SS2 session");
    assert_eq!(session.target(), Target::Ss2);
    let x0: Device = "X0".parse().unwrap();
    let y0: Device = "Y0".parse().unwrap();
    let initial = session
        .upload("LD X0\nOUT Y0\nEND")
        .expect("upload and verify");
    assert_eq!(initial.scan, 0);
    let inputs = [false, true, true, false, true, false];
    for (index, on) in inputs.into_iter().enumerate() {
        let result = session.step(&[(x0, on)]).expect("native and local scan");
        assert_eq!(result.scan, (index + 1) as u64);
        assert_eq!(result.mismatches, 0);
        for device in [x0, y0] {
            let row = result
                .rows
                .iter()
                .find(|row| row.device == device && row.kind == "bit")
                .expect("native bit snapshot");
            assert_eq!(
                row.original,
                u32::from(on),
                "{device} native scan {}",
                index + 1
            );
            assert_eq!(
                row.local,
                u32::from(on),
                "{device} local scan {}",
                index + 1
            );
        }
        assert_eq!(result.local.power.first(), Some(&on));
        assert_eq!(
            result.local.action_power.first().and_then(|v| v.first()),
            Some(&on)
        );
    }
}

#[test]
#[ignore = "requires the installed, digest-verified COMMGR 2.11 SS2 simulator"]
fn worker_accepts_repeated_async_steps_and_returns_fresh_snapshots() {
    use std::{
        future::Future,
        task::{Context, Poll, Wake, Waker},
        thread,
    };
    struct ThreadWake(thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => thread::park(),
            }
        }
    }
    block_on(async {
        let bridge = crate::es2_bridge::launch(DEFAULT_SS2_SIMULATOR.to_string())
            .await
            .expect("launch isolated SS2 worker");
        assert!(bridge.read_program_words().await.is_err());
        let x0: Device = "X0".parse().unwrap();
        let y0: Device = "Y0".parse().unwrap();
        let initial = bridge
            .upload("LD X0\nOUT Y0\nEND".into())
            .await
            .expect("upload");
        assert_eq!(initial.scan, 0);
        let words = bridge
            .read_program_words()
            .await
            .expect("serialized program readback");
        let expected =
            crate::es2_encoder::encode(&crate::plc::compile("LD X0\nOUT Y0\nEND").unwrap())
                .unwrap();
        assert_eq!(words, expected.words);
        let discovered = bridge
            .discover_program_words()
            .await
            .expect("boundary-based readback");
        assert_eq!(discovered, expected.words);
        for (i, on) in [true, false, true, false].into_iter().enumerate() {
            let snapshot = bridge.step(vec![(x0, on)]).await.expect("step");
            assert_eq!(snapshot.scan, (i + 1) as u64);
            let output = snapshot
                .rows
                .iter()
                .find(|r| r.device == y0 && r.kind == "bit")
                .expect("Y0 snapshot");
            assert_eq!(output.original, u32::from(on));
            assert_eq!(snapshot.mismatches, 0);
        }
        bridge.disconnect();
    });
}
