// How long does one tiny readback take (submit + wait + map)?
use emerge_gpu::gpu::Gpu;
fn main() {
    let gpu = Gpu::new();
    let b = gpu.storage_with(&[1u32, 2, 3, 4]);
    let _: Vec<u32> = gpu.read(&b, 4);
    let t = std::time::Instant::now();
    for _ in 0..1000 {
        let _: Vec<u32> = gpu.read(&b, 4);
    }
    println!("tiny readback: {:.0} us each", t.elapsed().as_secs_f64() * 1e3);
}
