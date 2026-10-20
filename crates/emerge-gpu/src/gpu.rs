//! Device setup and small helpers shared by the spikes.

use wgpu::util::DeviceExt;

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub name: String,
    pub timestamps: bool,
    pub period_ns: f32,
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
        let mut features = if timestamps { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        assert!(adapter.features().contains(wgpu::Features::SHADER_INT64), "the adapter has no 64-bit integers");
        features |= wgpu::Features::SHADER_INT64;
        // The adapter's own limits: the defaults cap a storage binding at 128 MB.
        let desc = wgpu::DeviceDescriptor { label: None, required_features: features, required_limits: adapter.limits(), ..Default::default() };
        let (device, queue) = pollster::block_on(adapter.request_device(&desc)).expect("request_device");
        let period_ns = queue.get_timestamp_period();
        Gpu { device, queue, name: format!("{} ({:?})", info.name, info.backend), timestamps, period_ns }
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

    /// `reps` dispatches in one compute pass: (GPU ms per dispatch from timestamps, if the
    /// adapter has them; wall ms per dispatch).
    pub fn bench(&self, pipeline: &wgpu::ComputePipeline, buffers: &[&wgpu::Buffer], workgroups: (u32, u32, u32), reps: u32) -> (Option<f64>, f64) {
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
        let queries = self.timestamps.then(|| {
            let set = self.device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 2 });
            let resolve = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            (set, resolve)
        });
        let t = std::time::Instant::now();
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let writes = queries.as_ref().map(|(set, _)| wgpu::ComputePassTimestampWrites {
                query_set: set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: writes });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            for _ in 0..reps {
                pass.dispatch_workgroups(workgroups.0, workgroups.1, workgroups.2);
            }
        }
        if let Some((set, resolve)) = &queries {
            enc.resolve_query_set(set, 0..2, resolve, 0);
        }
        self.queue.submit([enc.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        let wall = t.elapsed().as_secs_f64() * 1e3 / reps as f64;
        let gpu = queries.map(|(_, resolve)| {
            let ts: Vec<u64> = self.read(&resolve, 2);
            (ts[1].saturating_sub(ts[0])) as f64 * self.period_ns as f64 * 1e-6 / reps as f64
        });
        (gpu, wall)
    }

    /// A bind group layout of `n` read-write storage buffers at bindings `0..n`, shared by every
    /// pipeline of a module so that one bind group serves all its entry points.
    pub fn storage_layout(&self, n: u32) -> (wgpu::BindGroupLayout, wgpu::PipelineLayout) {
        let entries: Vec<_> = (0..n)
            .map(|i| wgpu::BindGroupLayoutEntry {
                binding: i,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: false }, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            })
            .collect();
        let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&bgl)], ..Default::default() });
        (bgl, layout)
    }

    pub fn module(&self, wgsl: &str) -> wgpu::ShaderModule {
        self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: None, source: wgpu::ShaderSource::Wgsl(wgsl.into()) })
    }

    pub fn pipeline_in(&self, module: &wgpu::ShaderModule, entry: &str, layout: &wgpu::PipelineLayout) -> wgpu::ComputePipeline {
        self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(layout), module, entry_point: Some(entry), compilation_options: Default::default(), cache: None })
    }

    pub fn group(&self, bgl: &wgpu::BindGroupLayout, buffers: &[&wgpu::Buffer]) -> wgpu::BindGroup {
        let entries: Vec<_> = buffers.iter().enumerate().map(|(i, b)| wgpu::BindGroupEntry { binding: i as u32, resource: b.as_entire_binding() }).collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: bgl, entries: &entries })
    }

    /// Runs the dispatches in order, in one submission, and waits.
    pub fn dispatch(&self, steps: &[(&wgpu::ComputePipeline, &wgpu::BindGroup, u32)]) {
        let mut enc = self.device.create_command_encoder(&Default::default());
        for (pipeline, group, groups) in steps {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, *group, &[]);
            pass.dispatch_workgroups(*groups, 1, 1);
        }
        self.queue.submit([enc.finish()]);
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    }
}
