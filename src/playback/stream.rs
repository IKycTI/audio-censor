use cpal::{BuildStreamError, Device, OutputCallbackInfo, Stream, StreamConfig};
use ringbuf::{
    HeapCons,
    traits::{Consumer, Observer},
};
use rodio::DeviceTrait;
use tracing::error;

pub fn get_stream<T>(
    device: Device,
    stream_config: StreamConfig,
    mut consumer: HeapCons<T>,
) -> Result<Stream, BuildStreamError>
where
    T: cpal::SizedSample + Send + 'static + Default,
{
    device.build_output_stream(
        &stream_config,
        move |data: &mut [T], _: &OutputCallbackInfo| {
            for frame in data.chunks_mut(stream_config.channels as usize) {
                if !consumer.is_empty() {
                    for sample in frame.iter_mut() {
                        // Берём первый сэмпл из буфера (FIFO)
                        *sample = consumer.try_pop().expect("Буффер пустой");
                    }
                } else {
                    // Если данных нет — тишина
                    for sample in frame.iter_mut() {
                        *sample = T::default();
                    }
                }
            }
        },
        move |e| error!("Ошибка выходного потока: {}", e),
        None,
    )
}
