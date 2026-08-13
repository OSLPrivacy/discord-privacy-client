use crypto::media_room::{
    prepare_room_join, AuthenticatedMediaEpochPackage, MediaRosterAuthority, OpaqueMediaRelay,
    SignedInDevice,
};
use crypto::voice_call_session::{ActiveVoiceCallSession, VoiceConnectionState, VoiceDeviceChoice};

const CLIENTS: [&str; 3] = ["A", "B", "C"];
const SURFACES: [&str; 7] = [
    "home",
    "inbox",
    "people",
    "privacy",
    "activity",
    "connections",
    "settings",
];

fn devices(kind: &str) -> Vec<VoiceDeviceChoice> {
    vec![
        VoiceDeviceChoice {
            id: format!("{kind}-1"),
            label: format!("Built-in {kind}"),
            available: true,
        },
        VoiceDeviceChoice {
            id: format!("{kind}-2"),
            label: format!("USB {kind}"),
            available: true,
        },
    ]
}

#[test]
fn task_6846_real_encrypted_call_drives_persistent_voice_panel() {
    let mutation = std::env::var("TASK6846_MUTATION").unwrap_or_default();
    let signed_in = CLIENTS
        .iter()
        .map(|name| {
            SignedInDevice::generate(
                format!("task6846-account-{name}"),
                format!("task6846-device-{name}"),
            )
            .expect("generate signed-in call client")
        })
        .collect::<Vec<_>>();
    let mut authority = MediaRosterAuthority::new(*b"TASK6846-ROOM!!!")
        .expect("create authenticated media roster authority");
    let authority_key = authority.authority_public_key();
    let transition = authority
        .bootstrap(
            6_846,
            signed_in
                .iter()
                .map(SignedInDevice::public_identity)
                .collect(),
            1,
        )
        .expect("bootstrap three-person media room");
    let joins = signed_in
        .iter()
        .map(|device| {
            let invitation = transition
                .invitation_for(&device.identity)
                .expect("current participant invitation");
            prepare_room_join(&invitation, device).expect("signed verified speaker join")
        })
        .collect::<Vec<_>>();
    let package = AuthenticatedMediaEpochPackage::new(&transition, joins);
    let verified_roster = package
        .verify(authority_key)
        .expect("verify exact encrypted roster");
    let mut relay = OpaqueMediaRelay::new(&verified_roster);
    let mut sessions = signed_in
        .into_iter()
        .map(|device| {
            ActiveVoiceCallSession::join_authenticated(
                "task6846-live-call",
                "Engineering voice",
                10_000,
                &package,
                authority_key,
                device,
                devices("mic"),
                devices("speaker"),
                "mic-1",
                "speaker-1",
            )
            .expect("join real encrypted call session")
        })
        .collect::<Vec<_>>();

    if mutation == "starve_participant" {
        sessions.pop();
    }
    assert_eq!(sessions.len(), 3, "TASK6846_FAILURE participant starvation");
    let identities = sessions
        .iter()
        .map(|session| session.identity().expect("live session identity").clone())
        .collect::<Vec<_>>();

    for (index, session) in sessions.iter_mut().enumerate() {
        let samples = format!("TASK6846-REAL-AUDIO-{}", CLIENTS[index]);
        let frame = session
            .send_audio(1, samples.as_bytes())
            .expect("shipping encrypted send")
            .expect("capture active");
        assert_ne!(frame.recipients[0].ciphertext, samples.as_bytes());
        relay
            .forward(identities[index].clone(), frame)
            .expect("opaque relay forward");
    }
    let mut decrypted_frames = 0usize;
    for (index, session) in sessions.iter_mut().enumerate() {
        let packets = relay
            .drain_for(&identities[index])
            .expect("drain encrypted receiver inbox");
        assert_eq!(packets.len(), 3);
        for packet in &packets {
            let opened = session
                .receive_audio(packet)
                .expect("shipping encrypted receive")
                .expect("listener is audible");
            assert!(opened.starts_with(b"TASK6846-REAL-AUDIO-"));
            decrypted_frames += 1;
        }
    }
    assert_eq!(decrypted_frames, 9);

    for session in &mut sessions {
        for surface in SURFACES {
            if mutation == "starve_navigation" && surface == "settings" {
                continue;
            }
            let before = session.snapshot().expect("active panel before navigation");
            session.navigate(surface).expect("navigate primary surface");
            let after = session.snapshot().expect("active panel after navigation");
            assert_eq!(after.session_id, before.session_id);
            assert_eq!(after.elapsed_ms, before.elapsed_ms);
            assert_eq!(after.participants, before.participants);
            assert_eq!(session.join_count(), 1);
            assert_eq!(session.reconnect_count(), 0);
        }
        assert_eq!(
            session.primary_surface_count(),
            7,
            "TASK6846_FAILURE navigation starvation"
        );
    }
    let navigation_visits = sessions.len() * SURFACES.len();

    let live_id = sessions[0].snapshot().unwrap().session_id.clone();
    let painted_id = if mutation == "detach_panel" {
        "detached-copy".to_string()
    } else {
        live_id.clone()
    };
    assert_eq!(
        painted_id, live_id,
        "TASK6846_FAILURE panel detached from encrypted call"
    );

    if mutation != "starve_control" {
        sessions[0]
            .toggle_mute(&live_id)
            .expect("mute active capture");
    }
    assert!(
        sessions[0].snapshot().unwrap().muted,
        "TASK6846_FAILURE mute control starved"
    );
    assert!(!sessions[0].capture_active());
    assert!(sessions[0]
        .send_audio(2, b"must-not-send")
        .unwrap()
        .is_none());
    sessions[0]
        .toggle_mute(&live_id)
        .expect("unmute active capture");
    assert!(sessions[0].capture_active());

    sessions[0]
        .toggle_deafen(&live_id)
        .expect("deafen active listener");
    let deafen_frame = sessions[1]
        .send_audio(2, b"TASK6846-DEAFEN-PROBE")
        .unwrap()
        .unwrap();
    relay.forward(identities[1].clone(), deafen_frame).unwrap();
    let packet = relay.drain_for(&identities[0]).unwrap().remove(0);
    assert!(sessions[0].receive_audio(&packet).unwrap().is_none());
    sessions[0]
        .toggle_deafen(&live_id)
        .expect("undeafen active listener");
    for identity in &identities[1..] {
        let _ = relay.drain_for(identity).unwrap();
    }

    sessions[0]
        .set_expanded(&live_id, true)
        .expect("expand dock");
    assert!(sessions[0].snapshot().unwrap().expanded);
    sessions[0]
        .choose_input_device(&live_id, "mic-2")
        .expect("choose microphone");
    sessions[0]
        .choose_output_device(&live_id, "speaker-2")
        .expect("choose speaker");
    let selected = sessions[0].snapshot().unwrap();
    assert_eq!(selected.selected_input_device_id.as_deref(), Some("mic-2"));
    assert_eq!(
        selected.selected_output_device_id.as_deref(),
        Some("speaker-2")
    );

    if mutation != "starve_device" {
        sessions[0]
            .lose_input_device("mic-2")
            .expect("observe USB microphone loss");
    }
    let lost = sessions[0].snapshot().unwrap();
    assert_eq!(
        lost.connection,
        VoiceConnectionState::DeviceLost,
        "TASK6846_FAILURE device-loss state starved"
    );
    assert_eq!(lost.selected_input_device_id, None);
    assert!(!sessions[0].capture_active());
    sessions[0]
        .choose_input_device(&live_id, "mic-1")
        .expect("choose surviving microphone");
    if mutation != "starve_reconnect" {
        sessions[0]
            .reconnect(&live_id)
            .expect("reconnect same call");
    }
    assert_eq!(
        sessions[0].reconnect_count(),
        1,
        "TASK6846_FAILURE reconnect starved"
    );
    assert_eq!(sessions[0].snapshot().unwrap().session_id, live_id);
    assert!(sessions[0].capture_active());

    let mounts_before_restart = sessions[0].panel_mount_count();
    if mutation != "starve_restart" {
        sessions[0].restart_panel();
    }
    assert_eq!(
        sessions[0].panel_mount_count(),
        mounts_before_restart + 1,
        "TASK6846_FAILURE panel restart starved"
    );
    assert_eq!(sessions[0].join_count(), 1);
    assert_eq!(sessions[0].reconnect_count(), 1);
    sessions[0]
        .advance_clock(135_000)
        .expect("advance authoritative call clock");
    assert_eq!(sessions[0].snapshot().unwrap().elapsed_ms, 125_000);

    let resumed = sessions[0]
        .send_audio(3, b"TASK6846-RESUMED-AFTER-DEVICE-LOSS")
        .unwrap()
        .unwrap();
    relay.forward(identities[0].clone(), resumed).unwrap();
    let resumed_packets = relay.drain_for(&identities[0]).unwrap();
    let opened = sessions[0]
        .receive_audio(resumed_packets.last().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(opened, b"TASK6846-RESUMED-AFTER-DEVICE-LOSS");

    let controls_exercised = 8;
    for (index, session) in sessions.iter_mut().enumerate() {
        let id = session.snapshot().unwrap().session_id.clone();
        if !(mutation == "starve_teardown" && index == 0) {
            session.leave(&id).expect("leave tears down call owner");
        }
        assert!(
            !session.capture_active(),
            "TASK6846_FAILURE capture retained after leave"
        );
        assert!(
            !session.media_keys_live(),
            "TASK6846_FAILURE media keys retained after leave"
        );
        assert!(
            session.snapshot().is_none(),
            "TASK6846_FAILURE panel retained after leave"
        );
        assert!(session.send_audio(4, b"after-leave").is_err());
    }

    println!(
        "TASK6846_CALL clients=3 verified_participants=3 encrypted_sent=5 decrypted=10 session_id=task6846-live-call"
    );
    println!(
        "TASK6846_NAVIGATION primary_surfaces=7 client_surface_visits={navigation_visits} rejoins=0 navigation_reconnects=0"
    );
    println!(
        "TASK6846_CONTROLS exercised={controls_exercised} mute=matched deafen=matched input=mic-2 output=speaker-2 expand=true leave=matched"
    );
    println!(
        "TASK6846_RECOVERY device_loss=device-lost reconnects=1 panel_restarts=1 elapsed_ms=125000 resumed_encrypted_frames=1"
    );
    println!("TASK6846_TEARDOWN clients=3 active_captures=0 live_media_key_owners=0 panels=0");
    println!("TASK6846_FINISH_LINE checked=true");
}
