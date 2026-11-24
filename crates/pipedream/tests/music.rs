//! The user's MP3 decodes and seeks (no audio device needed).
#![cfg(not(target_arch = "wasm32"))]
use rodio::Source;
use std::time::Duration;

#[test]
fn music_decodes_and_seeks() {
    let p = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../ATI_Radeon-9700_PipeDream-Demo_v1.1/ATI Demos/PipeDream/Audio/AnimusicIntro.mp3"
    );
    let data = std::fs::read(p).expect("need AnimusicIntro.mp3");
    let len = data.len() as u64;
    let mut d = rodio::Decoder::builder()
        .with_data(std::io::Cursor::new(data))
        .with_byte_len(len)
        .with_seekable(true)
        .build()
        .unwrap();
    let total = d.total_duration().expect("duration").as_secs_f64();
    assert!((total - 204.07).abs() < 0.2, "{total}");
    for t in [100.0, 10.0, 180.0] {
        d.try_seek(Duration::from_secs_f64(t)).unwrap();
        let peak = d.by_ref().take(44100).fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.01, "silence at {t}");
    }
}
