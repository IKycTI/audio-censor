use cpal::{BuildStreamError, Device, OutputCallbackInfo, SizedSample, Stream, StreamConfig};
use ringbuf::{traits::Consumer, HeapCons};
use rodio::DeviceTrait;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tracing::error;

pub fn build_playback_stream<T>(
    device: Device,
    stream_config: StreamConfig,
    mut consumer: HeapCons<T>,
    underrun_samples: Arc<AtomicU64>,
) -> Result<Stream, BuildStreamError>
where
    T: SizedSample + Send + 'static + Default,
{
    device.build_output_stream(
        &stream_config,
        move |data: &mut [T], _: &OutputCallbackInfo| {
            let popped = consumer.pop_slice(data);
            if popped < data.len() {
                for sample in &mut data[popped..] {
                    *sample = T::default(); // тишина при нехватке данных
                }
                underrun_samples.fetch_add((data.len() - popped) as u64, Ordering::Relaxed);
            }
        },
        move |e| error!("Ошибка выходного потока: {}", e),
        None,
    )
}
