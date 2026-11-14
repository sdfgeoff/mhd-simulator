//! Full-precision immutable RAM recordings, a selected-frame GPU cache, and bounded async downloads.
use crate::{
    gpu::{GpuContext, read_bytes},
    simulation::GridSi,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Instant,
};
#[derive(Clone)]
pub(crate) struct FieldStorage {
    gpu: Option<wgpu::Buffer>,
    host: Option<Arc<Vec<u8>>>,
}
impl FieldStorage {
    pub fn gpu(buffer: wgpu::Buffer) -> Self {
        Self {
            gpu: Some(buffer),
            host: None,
        }
    }
    pub fn host(bytes: Vec<u8>) -> Self {
        Self {
            gpu: None,
            host: Some(Arc::new(bytes)),
        }
    }
    pub fn buffer(&self) -> &wgpu::Buffer {
        self.gpu
            .as_ref()
            .expect("Only the selected resident recording has a GPU buffer")
    }
    pub fn is_resident(&self) -> bool {
        self.gpu.is_some()
    }
    pub fn gpu_bytes(&self) -> u64 {
        self.gpu.as_ref().map_or(0, wgpu::Buffer::size)
    }
    pub fn host_bytes(&self) -> u64 {
        self.host.as_ref().map_or(0, |v| v.len() as u64)
    }
    pub fn make_resident(&mut self, context: &GpuContext) {
        if self.gpu.is_some() {
            return;
        }
        let data = self.host.as_ref().expect("Recorded field data");
        let buffer = context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Selected RAM recording GPU view"),
            size: data.len() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        context.queue.write_buffer(&buffer, 0, data);
        self.gpu = Some(buffer);
    }
    fn read(&self, context: &GpuContext, offset: u64, size: u64) -> Result<Vec<u8>, String> {
        if let Some(data) = &self.host {
            return data
                .get(offset as usize..(offset + size) as usize)
                .map(|v| v.to_vec())
                .ok_or("Recorded field index is out of bounds".into());
        }
        read_bytes(context, self.buffer(), offset, size)
    }
}
pub(crate) fn probe<T: bytemuck::Pod>(
    context: &GpuContext,
    grid: &GridSi,
    data: &FieldStorage,
    point_mm: [f32; 3],
) -> Result<Option<T>, String> {
    let mut q = [0u32; 3];
    for a in 0..3 {
        let x = ((point_mm[a] * 1e-3 - grid.origin_m[a]) / grid.cell_m).floor();
        if !x.is_finite() || x < 0. || x >= grid.dims[a] as f32 {
            return Ok(None);
        }
        q[a] = x as u32;
    }
    let i = q[0] as u64 + grid.dims[0] as u64 * (q[1] as u64 + grid.dims[1] as u64 * q[2] as u64);
    let size = std::mem::size_of::<T>() as u64;
    Ok(Some(bytemuck::pod_read_unaligned(&data.read(
        context,
        i * size,
        size,
    )?)))
}
#[derive(Default)]
pub struct RecordingMemory {
    pub staging_bytes: AtomicU64,
    pub pending_bytes: AtomicU64,
    pub queued_ram_bytes: AtomicU64,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct RecordingMemorySnapshot {
    pub staging_bytes: u64,
    pub pending_bytes: u64,
    pub queued_ram_bytes: u64,
}
impl RecordingMemory {
    pub fn snapshot(&self) -> RecordingMemorySnapshot {
        RecordingMemorySnapshot {
            staging_bytes: self.staging_bytes.load(Ordering::Relaxed),
            pending_bytes: self.pending_bytes.load(Ordering::Relaxed),
            queued_ram_bytes: self.queued_ram_bytes.load(Ordering::Relaxed),
        }
    }
}
struct Staging {
    buffer: wgpu::Buffer,
    memory: Arc<RecordingMemory>,
}
impl Drop for Staging {
    fn drop(&mut self) {
        self.memory
            .staging_bytes
            .fetch_sub(self.buffer.size(), Ordering::Relaxed);
    }
}
pub struct ReadbackPool {
    context: GpuContext,
    free: Vec<Staging>,
    pub memory: Arc<RecordingMemory>,
}
struct Download {
    slot: Staging,
    ready: mpsc::Receiver<Result<Instant, String>>,
    status: Option<Result<Instant, String>>,
}
pub struct PendingFields {
    context: GpuContext,
    submission: wgpu::SubmissionIndex,
    downloads: Vec<Download>,
    bytes: u64,
    memory: Arc<RecordingMemory>,
    started: Instant,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct CaptureTiming {
    pub bytes: u64,
    pub completion_latency_ms: f64,
    pub cpu_copy_ms: f64,
}
impl ReadbackPool {
    pub fn new(context: GpuContext, memory: Arc<RecordingMemory>) -> Self {
        Self {
            context,
            free: Vec::new(),
            memory,
        }
    }
    fn acquire(&mut self, size: u64) -> Result<Staging, String> {
        if let Some(i) = self.free.iter().position(|s| s.buffer.size() == size) {
            return Ok(self.free.swap_remove(i));
        }
        if size > self.context.device.limits().max_buffer_size {
            return Err("Recorded field exceeds device transfer-buffer limit".into());
        }
        let buffer = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Reusable asynchronous recording download"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        self.memory.staging_bytes.fetch_add(size, Ordering::Relaxed);
        Ok(Staging {
            buffer,
            memory: self.memory.clone(),
        })
    }
    pub fn begin(&mut self, sources: &[&wgpu::Buffer]) -> Result<PendingFields, String> {
        let started = Instant::now();
        let mut enc = self
            .context
            .device
            .create_command_encoder(&Default::default());
        let mut slots = Vec::new();
        let mut bytes = 0;
        for source in sources {
            let slot = self.acquire(source.size())?;
            enc.copy_buffer_to_buffer(source, 0, &slot.buffer, 0, source.size());
            bytes += source.size();
            slots.push(slot);
        }
        let submission = self.context.queue.submit(Some(enc.finish()));
        let mut downloads = Vec::new();
        for slot in slots {
            let (tx, ready) = mpsc::channel();
            slot.buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| {
                    let _ = tx.send(r.map(|_| Instant::now()).map_err(|e| e.to_string()));
                });
            downloads.push(Download {
                slot,
                ready,
                status: None,
            });
        }
        self.memory
            .pending_bytes
            .fetch_add(bytes, Ordering::Relaxed);
        Ok(PendingFields {
            context: self.context.clone(),
            submission,
            downloads,
            bytes,
            memory: self.memory.clone(),
            started,
        })
    }
}
impl PendingFields {
    pub fn ready(&mut self) -> bool {
        self.context.device.poll(wgpu::Maintain::Poll);
        for d in &mut self.downloads {
            if d.status.is_none() {
                d.status = match d.ready.try_recv() {
                    Ok(v) => Some(v),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(e) => Some(Err(e.to_string())),
                };
            }
        }
        self.downloads.iter().all(|d| d.status.is_some())
    }
    pub fn finish(
        mut self,
        pool: &mut ReadbackPool,
    ) -> Result<(Vec<Vec<u8>>, CaptureTiming), String> {
        self.context
            .device
            .poll(wgpu::Maintain::WaitForSubmissionIndex(
                self.submission.clone(),
            ));
        let mut latency = 0f64;
        for d in &mut self.downloads {
            let status = d
                .status
                .take()
                .unwrap_or_else(|| d.ready.recv().unwrap_or_else(|e| Err(e.to_string())));
            latency = latency.max(status?.duration_since(self.started).as_secs_f64() * 1000.);
        }
        let start = Instant::now();
        let mut data = Vec::new();
        for d in self.downloads.drain(..) {
            let mapped = d.slot.buffer.slice(..).get_mapped_range();
            data.push(mapped.to_vec());
            drop(mapped);
            d.slot.buffer.unmap();
            pool.free.push(d.slot);
        }
        Ok((
            data,
            CaptureTiming {
                bytes: self.bytes,
                completion_latency_ms: latency,
                cpu_copy_ms: start.elapsed().as_secs_f64() * 1000.,
            },
        ))
    }
}
impl Drop for PendingFields {
    fn drop(&mut self) {
        self.memory
            .pending_bytes
            .fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::util::DeviceExt;
    #[test]
    fn downloads_capture_submission_order_reuse_staging_and_upload_exact_bytes() {
        let _lock = crate::gpu_test_lock();
        let context = crate::domain::GpuDomain::new().unwrap().context;
        let memory = Arc::new(RecordingMemory::default());
        let mut pool = ReadbackPool::new(context.clone(), memory.clone());
        let original = vec![7u32; 1024];
        let changed = vec![19u32; 1024];
        let source = context
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Recording order fixture"),
                contents: bytemuck::cast_slice(&original),
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
            });
        let pending = pool.begin(&[&source]).unwrap();
        context
            .queue
            .write_buffer(&source, 0, bytemuck::cast_slice(&changed));
        let (data, timing) = pending.finish(&mut pool).unwrap();
        assert_eq!(data[0], bytemuck::cast_slice::<u32, u8>(&original));
        assert_eq!(timing.bytes, 4096);
        assert_eq!(memory.snapshot().pending_bytes, 0);
        assert_eq!(memory.snapshot().staging_bytes, 4096);
        let mut field = FieldStorage::host(data.into_iter().next().unwrap());
        assert_eq!(field.gpu_bytes(), 0);
        let clone = field.clone();
        assert!(Arc::ptr_eq(
            field.host.as_ref().unwrap(),
            clone.host.as_ref().unwrap()
        ));
        field.make_resident(&context);
        assert_eq!(
            read_bytes(&context, field.buffer(), 0, 4096).unwrap(),
            bytemuck::cast_slice::<u32, u8>(&original)
        );
        let (next, _) = pool.begin(&[&source]).unwrap().finish(&mut pool).unwrap();
        assert_eq!(next[0], bytemuck::cast_slice::<u32, u8>(&changed));
        assert_eq!(memory.snapshot().staging_bytes, 4096);
        drop(pool);
        assert_eq!(memory.snapshot().staging_bytes, 0);
    }
}
