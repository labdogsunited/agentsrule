//! Device setup and small helpers shared by the spikes.

use wgpu::util::DeviceExt;

/// How many workgroups a step runs.
#[derive(Clone, Copy)]
pub enum Size<'a> {
    Groups(u32),
    /// A grid of x by y groups.
    Grid(u32, u32),
    /// Three words at the byte offset of the buffer, written by an earlier step.
    Indirect(&'a wgpu::Buffer, u64),
}

impl Size<'static> {
    /// A grid for `threads` threads of 64 per group, in rows of 32768 groups (a dimension holds at most 65535).
    pub fn threads(threads: usize) -> Size<'static> {
        let groups = threads.max(1).div_ceil(64) as u32;
        Size::Grid(groups.min(32768), groups.div_ceil(32768))
    }
}

pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub name: String,
    pub timestamps: bool,
    pub period_ns: f32,
    /// Kernel timing (`EMERGE_GPU_KERNELS=1`): pipeline names, and total microseconds and count by name.
    names: std::cell::RefCell<std::collections::HashMap<wgpu::ComputePipeline, String>>,
    /// While set, `dispatch`, `clear` and `copy_word` are recorded into one command buffer that is
    /// submitted by `flush` (and before any read), instead of one submission each.
    pub batching: std::cell::Cell<bool>,
    pending: std::cell::RefCell<Option<wgpu::CommandEncoder>>,
    kernel_time: Option<std::cell::RefCell<std::collections::HashMap<String, (f64, u32)>>>,
}

impl Gpu {
    /// The Vulkan discrete GPU; fails if there is none (llvmpipe and the OpenGL entry do not
    /// count). `SPIKE_ADAPTER=llvmpipe` picks the Vulkan adapter whose name contains that text,
    /// to run a spike on a second implementation.
    #[allow(clippy::new_without_default)] // finding a device is not a default value
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
        let kernel_time = (timestamps && std::env::var_os("EMERGE_GPU_KERNELS").is_some()).then(Default::default);
        Gpu { device, queue, name: format!("{} ({:?})", info.name, info.backend), timestamps, period_ns, names: Default::default(), batching: Default::default(), pending: Default::default(), kernel_time }
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
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::INDIRECT,
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
        self.read_at(buf, 0, count)
    }

    /// Copies `count` elements starting at element `first` to the host.
    pub fn read_at<T: bytemuck::Pod>(&self, buf: &wgpu::Buffer, first: usize, count: usize) -> Vec<T> {
        if count == 0 {
            return Vec::new(); // an empty mapping cannot be cast, and a world can be empty
        }
        self.flush();
        let size = std::mem::size_of::<T>();
        let (offset, bytes) = ((first * size) as u64, (count * size) as u64);
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(buf, offset, &staging, 0, bytes);
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
        let p = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: None, layout: Some(layout), module, entry_point: Some(entry), compilation_options: Default::default(), cache: None });
        if self.kernel_time.is_some() {
            self.names.borrow_mut().insert(p.clone(), entry.to_string());
        }
        p
    }

    /// Total GPU microseconds and calls by kernel, when `EMERGE_GPU_KERNELS` is set.
    pub fn kernel_report(&self) -> Vec<(String, f64, u32)> {
        let mut v: Vec<_> = self.kernel_time.iter().flat_map(|t| t.borrow().iter().map(|(k, (us, n))| (k.clone(), *us, *n)).collect::<Vec<_>>()).collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }

    pub fn group(&self, bgl: &wgpu::BindGroupLayout, buffers: &[&wgpu::Buffer]) -> wgpu::BindGroup {
        let entries: Vec<_> = buffers.iter().enumerate().map(|(i, b)| wgpu::BindGroupEntry { binding: i as u32, resource: b.as_entire_binding() }).collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: bgl, entries: &entries })
    }

    /// Runs the dispatches in order, in one submission. It does not wait: a `read` after it does.
    pub fn dispatch(&self, steps: &[(&wgpu::ComputePipeline, &wgpu::BindGroup, u32)]) {
        let steps: Vec<_> = steps.iter().map(|(p, g, n)| (*p, *g, Size::Groups(*n))).collect();
        self.dispatch_sized(&steps);
    }

    /// Like `dispatch`, with the size of a step either given or read by the GPU from a buffer (three words:
    /// the group counts), which an earlier step may have written.
    pub fn dispatch_sized(&self, steps: &[(&wgpu::ComputePipeline, &wgpu::BindGroup, Size)]) {
        if self.kernel_time.is_some() {
            return self.dispatch_timed(steps);
        }
        self.record(|enc| {
            // One pass for all steps: wgpu puts the barriers between dispatches itself.
            let mut pass = enc.begin_compute_pass(&Default::default());
            for (pipeline, group, size) in steps {
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, *group, &[]);
                match size {
                    Size::Groups(n) => pass.dispatch_workgroups(*n, 1, 1),
                    Size::Grid(x, y) => pass.dispatch_workgroups(*x, *y, 1),
                    Size::Indirect(buf, at) => pass.dispatch_workgroups_indirect(buf, *at),
                }
            }
        });
    }

    /// Runs `f` on the pending command buffer when batching, or on a fresh one that is submitted at once.
    fn record(&self, f: impl FnOnce(&mut wgpu::CommandEncoder)) {
        if self.batching.get() {
            let mut p = self.pending.borrow_mut();
            let enc = p.get_or_insert_with(|| self.device.create_command_encoder(&Default::default()));
            f(enc);
        } else {
            let mut enc = self.device.create_command_encoder(&Default::default());
            f(&mut enc);
            self.queue.submit([enc.finish()]);
        }
    }

    /// Submits what was recorded while batching.
    pub fn flush(&self) {
        if let Some(enc) = self.pending.borrow_mut().take() {
            self.queue.submit([enc.finish()]);
        }
    }

    /// Copies one 32-bit word between buffers, in order with the recorded dispatches.
    pub fn copy_buffer(&self, src: &wgpu::Buffer, dst: &wgpu::Buffer, bytes: u64) {
        self.record(|enc| enc.copy_buffer_to_buffer(src, 0, dst, 0, bytes));
    }

    pub fn copy_word(&self, src: &wgpu::Buffer, src_offset: u64, dst: &wgpu::Buffer, dst_offset: u64) {
        self.record(|enc| enc.copy_buffer_to_buffer(src, src_offset, dst, dst_offset, 4));
    }

    /// Each step in its own pass with timestamps, waited for; for the kernel report only.
    fn dispatch_timed(&self, steps: &[(&wgpu::ComputePipeline, &wgpu::BindGroup, Size)]) {
        self.flush();
        let n = steps.len() as u32;
        let set = self.device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: 2 * n });
        let resolve = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16 * n as u64, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        for (i, (pipeline, group, size)) in steps.iter().enumerate() {
            let writes = wgpu::ComputePassTimestampWrites { query_set: &set, beginning_of_pass_write_index: Some(2 * i as u32), end_of_pass_write_index: Some(2 * i as u32 + 1) };
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None, timestamp_writes: Some(writes) });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, *group, &[]);
            match size {
                Size::Groups(n) => pass.dispatch_workgroups(*n, 1, 1),
                Size::Grid(x, y) => pass.dispatch_workgroups(*x, *y, 1),
                Size::Indirect(buf, at) => pass.dispatch_workgroups_indirect(buf, *at),
            }
        }
        enc.resolve_query_set(&set, 0..2 * n, &resolve, 0);
        self.queue.submit([enc.finish()]);
        let ts: Vec<u64> = self.read(&resolve, 2 * n as usize);
        let names = self.names.borrow();
        let mut table = self.kernel_time.as_ref().unwrap().borrow_mut();
        for (i, (pipeline, _, _)) in steps.iter().enumerate() {
            let us = ts[2 * i + 1].saturating_sub(ts[2 * i]) as f64 * self.period_ns as f64 * 1e-3;
            let e = table.entry(names.get(*pipeline).cloned().unwrap_or_else(|| "?".into())).or_insert((0.0, 0));
            e.0 += us;
            e.1 += 1;
        }
    }

    /// Sets a buffer to zero on the GPU (no upload).
    pub fn clear(&self, buf: &wgpu::Buffer) {
        self.record(|enc| enc.clear_buffer(buf, 0, None));
    }

    /// Zeroes `bytes` bytes of a buffer from byte `offset`, in order with the recorded work.
    pub fn clear_range(&self, buf: &wgpu::Buffer, offset: u64, bytes: u64) {
        self.record(|enc| enc.clear_buffer(buf, offset, Some(bytes)));
    }

    /// Waits for all submitted work.
    pub fn sync(&self) {
        self.flush();
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    }
}
