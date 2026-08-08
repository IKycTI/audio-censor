use cpal::{BuildStreamError, Device, InputCallbackInfo, Stream, StreamConfig};
use ringbuf::{HeapProd, traits::Producer};
use rodio::DeviceTrait;
use tracing::error;

pub fn get_stream<T>(
    device: Device,
    stream_config: StreamConfig,
    mut producer: HeapProd<T>,
) -> Result<Stream, BuildStreamError>
where
    T: cpal::SizedSample + Send + 'static,
{
    device.build_input_stream(
        &stream_config,
        move |data: &[T], _: &InputCallbackInfo| {
            for d in data.iter() {
                producer.try_push(*d).ok();
            }
        },
        move |e| error!("Ошибка входного потока: {}", e),
        None,
    )
}
