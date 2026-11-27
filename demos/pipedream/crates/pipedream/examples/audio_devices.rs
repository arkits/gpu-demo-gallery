//! Lists what the audio backend sees; makes no sound.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let host = rodio::cpal::default_host();
    println!(
        "default: {:?}",
        host.default_output_device()
            .map(|d| d.default_output_config())
    );
    for d in host.output_devices().unwrap() {
        println!("{:?}", d.default_output_config());
    }
    match rodio::DeviceSinkBuilder::from_default_device()
        .unwrap()
        .with_buffer_size(rodio::cpal::BufferSize::Default)
        .open_sink_or_fallback()
    {
        Ok(_) => println!("opened"),
        Err(e) => println!("failed: {e:?}"),
    }
}
#[cfg(target_arch = "wasm32")]
fn main() {}
