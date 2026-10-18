use cpal::{BuildStreamError, Device, InputCallbackInfo, SizedSample, Stream, StreamConfig};
use ringbuf::{
    HeapProd,
    traits::{Observer, Producer},
};
use rodio::DeviceTrait;
use std::sync::{Arc, atomic::Ordering};
use tracing::error;

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

            // Здесь нельзя логировать: колбэк работает в realtime-потоке аудиосистемы,
            // и запись в лог (блокировки, ввод-вывод) может сорвать его временное окно
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
