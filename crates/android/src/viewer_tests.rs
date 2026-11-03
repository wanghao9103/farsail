use super::*;
use axum::{
    Json, Router,
    extract::{Request, State},
};
use farsail_client::SecureStore;
use farsail_transport::{Authority, Claims, Config, Transport};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
const SOURCE: &str = "10000000-0000-4000-8000-000000000001";
const TARGET: &str = "20000000-0000-4000-8000-000000000001";
const ID: &str = "30000000-0000-4000-8000-000000000001";
struct Memory(Mutex<HashMap<String, Vec<u8>>>);
impl SecureStore for Memory {
    fn read(&self, k: &str) -> farsail_client::Result<Option<Vec<u8>>> {
        Ok(self.0.lock().unwrap().get(k).cloned())
    }
    fn write(&self, k: &str, v: &[u8]) -> farsail_client::Result<()> {
        self.0.lock().unwrap().insert(k.into(), v.into());
        Ok(())
    }
    fn delete(&self, k: &str) -> farsail_client::Result<()> {
        self.0.lock().unwrap().remove(k);
        Ok(())
    }
}
struct Coordinator {
    claims: Claims,
    address: Value,
    approved: AtomicBool,
    requests: AtomicUsize,
    revoked: AtomicUsize,
}
impl Authority for Coordinator {
    async fn inspect(&self, id: &str, token: &str) -> farsail_transport::Result<Claims> {
        if id != ID || token != "synthetic-grant" {
            return Err(farsail_transport::Error::Denied);
        }
        let mut claims = self.claims.clone();
        claims.checked_at = Instant::now();
        Ok(claims)
    }
    async fn issue(&self, _: &str, _: bool) -> farsail_transport::Result<String> {
        Ok("synthetic-grant".into())
    }
}
async fn answer(State(c): State<Arc<Coordinator>>, request: Request) -> Json<Value> {
    let path = request.uri().path().to_string();
    let value = if path.ends_with("/refresh") {
        json!({"access_token":"synthetic-access","refresh_token":"synthetic-refresh","session_id":"synthetic-session","access_expires_in":3600})
    } else if path.ends_with("/heartbeat") {
        json!({"generation":1,"expires_in":60})
    } else if path == "/v1/remote/request" {
        let bytes = axum::body::to_bytes(request.into_body(), 4096)
            .await
            .unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["source_device_id"], SOURCE);
        assert_eq!(v["target_device_id"], TARGET);
        assert_eq!(v["permission"], json!(c.claims.permission));
        c.requests.fetch_add(1, Ordering::SeqCst);
        json!({"id":ID})
    } else if path.ends_with("/peer-address") {
        json!({"endpoint_addr":c.address})
    } else if path.ends_with("/transport-grant") {
        json!(c.claims)
    } else if path.ends_with("/revoke") {
        c.revoked.fetch_add(1, Ordering::SeqCst);
        json!({})
    } else if path == format!("/v1/remote/{ID}") {
        json!({"id":ID,"state":if c.approved.load(Ordering::SeqCst){"approved"}else{"pending"}})
    } else {
        json!({})
    };
    Json(value)
}
async fn fixture(
    permission: RemotePermission,
) -> (
    Arc<Host>,
    Transport,
    Arc<Coordinator>,
    tokio::task::JoinHandle<()>,
) {
    let target = Transport::bind([71; 32], Config::default()).await.unwrap();
    let source = iroh::SecretKey::from_bytes(&[72; 32]).public();
    let c = Arc::new(Coordinator {
        claims: Claims {
            session_id: ID.into(),
            permission,
            source_public_key: hex::encode(source.as_bytes()),
            target_public_key: hex::encode(target.id().as_bytes()),
            nonce: "ab".repeat(32),
            expires_in: 25,
            checked_at: Instant::now(),
        },
        address: serde_json::to_value(target.addr()).unwrap(),
        approved: AtomicBool::new(true),
        requests: AtomicUsize::new(0),
        revoked: AtomicUsize::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = Router::new().fallback(answer).with_state(c.clone());
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let store = Arc::new(Memory(Mutex::new(HashMap::new())));
    store.write("server", base.as_bytes()).unwrap();
    store.write("login",serde_json::to_vec(&json!({"access_token":"synthetic-access","refresh_token":"synthetic-refresh","session_id":"synthetic-session","access_expires_in":3600})).unwrap().as_slice()).unwrap();
    store.write("device-token",serde_json::to_vec(&json!({"id":SOURCE,"device_token":"synthetic-device-token","owner_id":"synthetic-owner","session_id":"synthetic-session"})).unwrap().as_slice()).unwrap();
    store
        .write(
            &format!(
                "identity-{}-synthetic-owner",
                hex::encode(Sha256::digest(base.as_bytes()))
            ),
            &[72; 32],
        )
        .unwrap();
    let host = Host::new(Arc::new(NativeClient::new(store).unwrap()));
    host.listen();
    (host, target, c, server)
}
async fn wait(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await
        }
    })
    .await
    .unwrap();
}
async fn packet(s: &Session, channel: Channel, bytes: Vec<u8>) {
    s.send(Frame { channel, bytes }).await.unwrap();
}
async fn receive(s: &Session) -> Frame {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match s.receive().await {
                Ok(f) => return f,
                Err(farsail_transport::Error::Timeout) => continue,
                Err(e) => panic!("receive: {e}"),
            }
        }
    })
    .await
    .unwrap()
}
async fn image(s: &Session, sequence: u64, layout: u64, display: u32) {
    let f = JpegFrame::encode_rgb(
        FrameMeta {
            monitor: display,
            layout,
            sequence,
            captured_ms: 0,
            width: 16,
            height: 12,
            origin_x: 0,
            origin_y: 0,
        },
        &[90; 16 * 12 * 3],
        65,
    )
    .unwrap();
    packet(s, Channel::Media, f.to_wire().unwrap()).await;
    let ack = receive(s).await;
    assert_eq!(ack.channel, Channel::Media);
    assert_eq!(&ack.bytes[..4], b"FSA1");
    assert_eq!(
        u64::from_be_bytes(ack.bytes[4..].try_into().unwrap()),
        sequence
    );
}
async fn connect(
    h: &Arc<Host>,
    target: &Transport,
    c: Arc<Coordinator>,
    permission: RemotePermission,
) -> (u64, Session) {
    let t = target.clone();
    let accept = tokio::spawn(async move { t.accept(c).await.unwrap() });
    let ticket = h.viewer.begin();
    h.call(
        "viewer_connect",
        json!({"ticket":ticket,"target":TARGET,"permission":permission}),
    )
    .await
    .unwrap();
    let peer = accept.await.unwrap();
    assert!(peer.is_open_now());
    assert!(!h.client.hosting_enabled());
    assert!(!h.local.lock().unwrap().capture);
    (ticket, peer)
}
#[tokio::test]
async fn mobile_controller_connects_android_and_desktop_without_local_sharing_and_keeps_roles_independent()
 {
    for android in [false, true] {
        let (h, target, c, server) = fixture(RemotePermission::Control).await;
        let (ticket, peer) = connect(&h, &target, c.clone(), RemotePermission::Control).await;
        let display = if android { 1 } else { 7 };
        if android {
            packet(&peer, Channel::Media, b"FSH1android".to_vec()).await;
        }
        let mut list = b"FSL1".to_vec();
        list.extend(serde_json::to_vec(&json!([{"id":display,"name":"Synthetic","x":0,"y":0,"width":16,"height":12,"dpi":160,"rotation":0}])).unwrap());
        packet(&peer, Channel::Media, list).await;
        image(&peer, 1, 55, display).await;
        wait(|| h.viewer.status(ticket)["control"] == true).await;
        if android {
            tokio::time::sleep(Duration::from_secs(6)).await;
            assert_eq!(
                h.viewer.status(ticket)["control"],
                true,
                "a valid connection with an unchanged screen must remain controllable"
            );
        }
        assert_eq!(
            h.viewer.status(ticket)["platform"],
            if android { "android" } else { "desktop" }
        );
        assert_eq!(
            JpegFrame::from_wire(&h.viewer.frame(ticket).unwrap())
                .unwrap()
                .meta
                .layout,
            55
        );
        h.call("viewer_input",json!({"ticket":ticket,"generation":0,"input":{"kind":"button","display":display,"layout":55,"x":0.25,"y":0.75,"button":"left","down":true}})).await.unwrap();
        let f = receive(&peer).await;
        assert_eq!(&f.bytes[..4], b"FSI1");
        assert_eq!(u64::from_be_bytes(f.bytes[4..12].try_into().unwrap()), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&f.bytes[12..]).unwrap()["x"],
            0.25
        );
        packet(
            &peer,
            Channel::Control,
            b"FSB1{\"generation\":1,\"blocked\":true,\"message\":\"synthetic pause\"}".to_vec(),
        )
        .await;
        wait(|| h.viewer.status(ticket)["input"]["blocked"] == true).await;
        h.call(
            "viewer_input",
            json!({"ticket":ticket,"generation":0,"input":{"kind":"key","vk":13,"down":true}}),
        )
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(120), peer.receive())
                .await
                .is_err()
        );
        h.call(
            "viewer_input",
            json!({"ticket":ticket,"input":{"kind":"resume_control"}}),
        )
        .await
        .unwrap();
        assert_eq!(&receive(&peer).await.bytes[12..], b"FSR2");
        packet(
            &peer,
            Channel::Control,
            b"FSB1{\"generation\":2,\"blocked\":false,\"message\":null}".to_vec(),
        )
        .await;
        wait(|| h.viewer.status(ticket)["input"]["generation"] == 2).await;
        image(&peer, 2, 56, display).await;
        let frame = h.viewer.frame(ticket).unwrap();
        assert_eq!(JpegFrame::from_wire(&frame).unwrap().meta.layout, 56);
        h.call("viewer_input",json!({"ticket":ticket,"generation":2,"input":{"kind":"move","display":display,"layout":55,"x":0.25,"y":0.75}})).await.unwrap();
        h.call(
            "viewer_input",
            json!({"ticket":ticket,"generation":0,"input":{"kind":"key","vk":13,"down":true}}),
        )
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(120), peer.receive())
                .await
                .is_err()
        );
        h.call("viewer_input",json!({"ticket":ticket,"generation":2,"input":{"kind":"key","vk":if android{172}else{13},"down":true}})).await.unwrap();
        assert_eq!(
            u64::from_be_bytes(receive(&peer).await.bytes[4..12].try_into().unwrap()),
            3
        );
        // Stopping local capture must not tear down an unrelated outgoing viewer.
        h.call("stop", json!({})).await.unwrap();
        assert!(peer.is_open_now());
        image(&peer, 3, 56, display).await;
        image(&peer, 4, 56, display).await;
        assert_eq!(
            JpegFrame::from_wire(&h.viewer.frame(ticket).unwrap())
                .unwrap()
                .meta
                .sequence,
            4
        );
        let next = h.viewer.begin();
        h.viewer.close(ticket);
        assert!(h.viewer.current(next));
        assert!(h.viewer.frame(next).is_none());
        assert!(
            h.call("viewer_input", json!({"ticket":ticket,"input":null}))
                .await
                .is_err()
        );
        h.viewer.close(next);
        wait(|| c.revoked.load(Ordering::SeqCst) > 0).await;
        h.client.stop_transport().await;
        target.close().await;
        server.abort();
    }
}
#[tokio::test]
async fn mobile_view_grant_cannot_send_input_but_can_select_screen() {
    let (h, target, c, server) = fixture(RemotePermission::View).await;
    let (ticket, peer) = connect(&h, &target, c, RemotePermission::View).await;
    let mut list = b"FSL1".to_vec();
    list.extend(serde_json::to_vec(&json!([{"id":7,"name":"Synthetic","x":0,"y":0,"width":16,"height":12,"dpi":160,"rotation":0}])).unwrap());
    packet(&peer, Channel::Media, list).await;
    image(&peer, 1, 1, 7).await;
    assert_eq!(h.viewer.status(ticket)["control"], false);
    assert!(
        h.call(
            "viewer_input",
            json!({"ticket":ticket,"input":{"kind":"resume_control"}})
        )
        .await
        .is_err()
    );
    h.call("viewer_select", json!({"ticket":ticket,"display":7}))
        .await
        .unwrap();
    let f = receive(&peer).await;
    assert_eq!(&f.bytes[..4], b"FSS1");
    assert!(h.viewer.frame(ticket).is_none());
    h.viewer.close(ticket);
    h.client.stop_transport().await;
    target.close().await;
    server.abort();
}
#[tokio::test]
async fn cancelling_pending_mobile_request_revokes_it_and_rejects_late_approval() {
    let (h, target, c, server) = fixture(RemotePermission::Control).await;
    c.approved.store(false, Ordering::SeqCst);
    let ticket = h.viewer.begin();
    let owner = h.clone();
    let job = tokio::spawn(async move {
        owner
            .call(
                "viewer_connect",
                json!({"ticket":ticket,"target":TARGET,"permission":"control"}),
            )
            .await
    });
    wait(|| c.requests.load(Ordering::SeqCst) > 0).await;
    h.viewer.close(ticket);
    c.approved.store(true, Ordering::SeqCst);
    assert!(job.await.unwrap().is_err());
    assert!(c.revoked.load(Ordering::SeqCst) > 0);
    assert_eq!(h.viewer.status(ticket)["control"], false);
    h.client.stop_transport().await;
    target.close().await;
    server.abort();
}
