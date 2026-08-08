use ringbuf::{HeapCons, HeapProd, HeapRb, traits::*};

pub fn create_scsp<T>(size: usize) -> (HeapProd<T>, HeapCons<T>) {
    let rb = HeapRb::<T>::new(size);
    let (prod, cons) = rb.split();
    return (prod, cons);
}
