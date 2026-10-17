use cpal::{BuildStreamError, Device, InputCallbackInfo, SizedSample, Stream, StreamConfig};
use ringbuf::{
    HeapProd,
    traits::{Observer, Producer},
};
use rodio::DeviceTrait;
use std::sync::{Arc, atomic::Ordering};
use tracing::{debug, error};

use crate::{metrics::Metrics, whole_frames};

pub fn build_capture_stream<T>(
    device: Device,
    stream_config: StreamConfig,
    mut producer: HeapProd<T>,
    metrics: Arc<Metrics>,
) -> Result<Stream, BuildStreamError>
where
    T: SizedSample + Send + 'static,
{
    let channels = stream_config.channels;
    device.build_input_stream(
        &stream_config,
        move |data: &[T], _: &InputCallbackInfo| {
            // При переполнении пишем только целые кадры, иначе каналы сдвинутся
            let fits = whole_frames(producer.vacant_len().min(data.len()), channels);
            let pushed = producer.push_slice(&data[..fits]);

            debug!(
                "callback: получено {} сэмплов, записано {}",
                data.len(),
                pushed
            );
            if pushed < data.len() {
                metrics
                    .dropped_input_samples
                    .fetch_add((data.len() - pushed) as u64, Ordering::Relaxed);
            }
        },
        move |e| error!("Ошибка входного потока: {}", e),
        None,
    )
}
