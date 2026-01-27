use micround::core::{AppContext, Command, DeviceId, Event};
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn event_bus_message_flow_reaches_subscribers() {
    let (ctx, mut command_rx) = AppContext::new();
    let handle = ctx.handle();

    let mut subscriber_a = handle.subscribe_events();
    let mut subscriber_b = handle.subscribe_events();

    let engine_handle = handle.clone();
    let engine = tokio::spawn(async move {
        if let Some(command) = command_rx.recv().await {
            if let Command::StartCapture { device_id } = command {
                engine_handle.publish_event(Event::CaptureStarted {
                    device_id,
                    resolution: (640, 480),
                    fps: 30.0,
                });
            }
        }
    });

    let device_id = DeviceId("test-device".into());
    handle
        .send_command(Command::StartCapture {
            device_id: device_id.clone(),
        })
        .await
        .expect("command should send");

    let event_a = timeout(Duration::from_secs(1), subscriber_a.recv())
        .await
        .expect("subscriber A timed out")
        .expect("subscriber A should receive an event");

    let event_b = timeout(Duration::from_secs(1), subscriber_b.recv())
        .await
        .expect("subscriber B timed out")
        .expect("subscriber B should receive an event");

    match event_a {
        Event::CaptureStarted { device_id: id, .. } => {
            assert_eq!(id, device_id);
        }
        other => panic!("unexpected event for subscriber A: {:?}", other),
    }

    match event_b {
        Event::CaptureStarted { device_id: id, .. } => {
            assert_eq!(id, device_id);
        }
        other => panic!("unexpected event for subscriber B: {:?}", other),
    }

    engine.await.expect("engine task should complete");
}
