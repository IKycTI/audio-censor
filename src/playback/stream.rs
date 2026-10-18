use cpal::{BuildStreamError, Device, OutputCallbackInfo, SizedSample, Stream, StreamConfig};
use ringbuf::{
    HeapCons,
    traits::{Consumer, Observer},
};
use rodio::DeviceTrait;
use std::sync::{Arc, atomic::Ordering};
use tracing::error;

use crate::{metrics::Metrics, whole_frames};

pub fn build_playback_stream<T>(
    device: Device,
    stream_config: StreamConfig,
    mut consumer: HeapCons<T>,
    metrics: Arc<Metrics>,
) -> Result<Stream, BuildStreamError>
where
    T: SizedSample + Send + 'static,
{
    let channels = stream_config.channels;
    device.build_output_stream(
        &stream_config,
        move |data: &mut [T], _: &OutputCallbackInfo| {
            // При нехватке данных забираем только целые кадры, иначе каналы сдвинутся
            let available = whole_frames(consumer.occupied_len().min(data.len()), channels);
            let popped = consumer.pop_slice(&mut data[..available]);
            if popped < data.len() {
                for sample in &mut data[popped..] {
                    // Тишина при нехватке данных: для U8/U16 это середина диапазона, а не 0
                    *sample = T::EQUILIBRIUM;
                }
                metrics
                    .underrun_samples
                    .fetch_add((data.len() - popped) as u64, Ordering::Relaxed);
            }
        },
        move |e| error!("Ошибка выходного потока: {}", e),
        None,
    )
}
