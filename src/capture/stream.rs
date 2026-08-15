use cpal::{BuildStreamError, Device, InputCallbackInfo, SizedSample, Stream, StreamConfig};
use ringbuf::{traits::Producer, HeapProd};
use rodio::DeviceTrait;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tracing::{error, info};

pub fn build_capture_stream<T>(
    device: Device,
    stream_config: StreamConfig,
    mut producer: HeapProd<T>,
    dropped_samples: Arc<AtomicU64>,
) -> Result<Stream, BuildStreamError>
where
    T: SizedSample + Send + 'static,
{
    device.build_input_stream(
        &stream_config,
        move |data: &[T], _: &InputCallbackInfo| {
            let pushed = producer.push_slice(data);

            info!(
                "callback: получено {} сэмплов, записано {}",
                data.len(),
                pushed
            );
            if pushed < data.len() {
                dropped_samples.fetch_add((data.len() - pushed) as u64, Ordering::Relaxed);
            }
        },
        move |e| error!("Ошибка входного потока: {}", e),
        None,
    )
}
