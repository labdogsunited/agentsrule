//! Device setup and small helpers shared by the spikes.

use wgpu::util::DeviceExt;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub name: String,
    pub timestamps: bool,
}

impl Gpu {
    /// The Vulkan discrete GPU; fails if there is none (llvmpipe and the OpenGL entry do not
    /// count). `SPIKE_ADAPTER=llvmpipe` picks the Vulkan adapter whose name contains that text,
    /// to run a spike on a second implementation.
    pub fn new() -> Gpu {
        let want = std::env::var("SPIKE_ADAPTER").ok();
        let instance = wgpu::Instance::default();
        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::VULKAN));
        let adapter = adapters
            .into_iter()
            .find(|a| match &want {
                Some(w) => a.get_info().name.contains(w.as_str()),
                None => a.get_info().device_type == wgpu::DeviceType::DiscreteGpu,
            })
            .expect("no matching Vulkan adapter found");
        let info = adapter.get_info();
        let timestamps = adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let features = if timestamps { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        let desc = wgpu::DeviceDescriptor { label: None, required_features: features, ..Default::default() };
        let (device, queue) = pollster::block_on(adapter.request_device(&desc)).expect("request_device");
        Gpu { device, queue, name: format!("{} ({:?})", info.name, info.backend), timestamps }
    }

    pub fn pipeline(&self, wgsl: &str, entry: &str) -> wgpu::ComputePipeline {
        let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(wgsl.into()),
        });
        self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    }

    pub fn storage(&self, bytes: usize) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn storage_with<T: bytemuck::Pod>(&self, data: &[T]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
        })
    }

    pub fn uniform_with<T: bytemuck::Pod>(&self, data: &[T]) -> wgpu::Buffer {
        self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Copies a buffer to the host.
    pub fn read<T: bytemuck::Pod>(&self, buf: &wgpu::Buffer, count: usize) -> Vec<T> {
        let bytes = (count * std::mem::size_of::<T>()) as u64;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, 0, &staging, 0, bytes);
        self.queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        rx.recv().unwrap().expect("map");
        let out = bytemuck::cast_slice::<u8, T>(&staging.slice(..).get_mapped_range().expect("range")).to_vec();
        staging.unmap();
        out
    }

    /// Runs one compute pass with the given buffers bound in order at group 0, and waits.
    /// Returns the wall time.
    pub fn run(&self, pipeline: &wgpu::ComputePipeline, buffers: &[&wgpu::Buffer], workgroups: (u32, u32, u32)) -> std::time::Duration {
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry { binding: i as u32, resource: b.as_entire_binding() })
            .collect();
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let t = std::time::Instant::now();
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(workgroups.0, workgroups.1, workgroups.2);
        }
        self.queue.submit([enc.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        t.elapsed()
    }
}
