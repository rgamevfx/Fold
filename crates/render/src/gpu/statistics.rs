/// Transfer and adapter costs for one evaluation, excluding decoder-owned caches.
#[derive(Clone, Copy, Debug, Default)]
pub struct Statistics {
    pub upload_bytes: u64,
    pub upload_allocations: u32,
    pub upload_reuses: u32,
    pub lut_upload_bytes: u64,
    pub readback_bytes: u64,
    pub status_readback_bytes: u64,
    /// External immutable-image ingress. This is GPU-local copy traffic.
    pub shared_gpu_copy_bytes: u64,
    /// Full-frame CPU RGB/RGBA adapters. Native decode and geometry preparation are
    /// measured separately below; zero here does not mean zero CPU work. EXR
    /// timing includes demand wait; source workers may already have decoded it.
    pub cpu_adapter_nodes: u32,
    pub resident_still_nodes: u32,
    pub cpu_adapter_nanoseconds: u128,
    pub compute_passes: u32,
    pub resident_coverage_nodes: u32,
    pub coverage_pixels: u64,
    pub shader_compilations: u32,
    pub fused_opacity_passes: u32,
    pub fused_pointwise_passes: u32,
    pub graph_prepare_nanoseconds: u128,
    pub evaluate_cpu_nanoseconds: u128,
    pub output_cpu_nanoseconds: u128,
    pub queue_submit_nanoseconds: u128,
    pub graphics_queue_nanoseconds: u128,
    pub native_video_nodes: u32,
    pub resident_video_nodes: u32,
    pub vector_prepare_nanoseconds: u128,
    pub vector_upload_bytes: u64,
    pub vector_geometry_upload_bytes: u64,
    pub vector_instance_upload_bytes: u64,
    pub vector_instances: u64,
    pub vector_unique_triangles: u64,
    pub vector_execution_triangles: u64,
    pub native_decode_nanoseconds: u128,
    /// Inclusive helper call intervals, not isolated GPU codec/copy timestamps.
    pub native_codec_call_nanoseconds: u64,
    pub native_allocate_nanoseconds: u128,
    pub native_submit_nanoseconds: u128,
    pub native_seeks: u64,
    pub native_forward_reuses: u64,
    pub native_copy_ready_nanoseconds: u64,
    pub native_gpu_local_copy_bytes: u64,
    pub native_pipe_bytes: u64,
    pub native_upload_encode_nanoseconds: u128,
}
impl Statistics {
    pub(super) fn accumulate(&mut self, other: &Self) {
        self.upload_bytes += other.upload_bytes;
        self.upload_allocations += other.upload_allocations;
        self.upload_reuses += other.upload_reuses;
        self.lut_upload_bytes += other.lut_upload_bytes;
        self.readback_bytes += other.readback_bytes;
        self.status_readback_bytes += other.status_readback_bytes;
        self.shared_gpu_copy_bytes += other.shared_gpu_copy_bytes;
        self.cpu_adapter_nodes += other.cpu_adapter_nodes;
        self.resident_still_nodes += other.resident_still_nodes;
        self.cpu_adapter_nanoseconds += other.cpu_adapter_nanoseconds;
        self.compute_passes += other.compute_passes;
        self.resident_coverage_nodes += other.resident_coverage_nodes;
        self.coverage_pixels += other.coverage_pixels;
        self.shader_compilations += other.shader_compilations;
        self.fused_opacity_passes += other.fused_opacity_passes;
        self.fused_pointwise_passes += other.fused_pointwise_passes;
        self.graph_prepare_nanoseconds += other.graph_prepare_nanoseconds;
        self.evaluate_cpu_nanoseconds += other.evaluate_cpu_nanoseconds;
        self.output_cpu_nanoseconds += other.output_cpu_nanoseconds;
        self.queue_submit_nanoseconds += other.queue_submit_nanoseconds;
        self.graphics_queue_nanoseconds += other.graphics_queue_nanoseconds;
        self.native_video_nodes += other.native_video_nodes;
        self.resident_video_nodes += other.resident_video_nodes;
        self.vector_prepare_nanoseconds += other.vector_prepare_nanoseconds;
        self.vector_upload_bytes += other.vector_upload_bytes;
        self.vector_geometry_upload_bytes += other.vector_geometry_upload_bytes;
        self.vector_instance_upload_bytes += other.vector_instance_upload_bytes;
        self.vector_instances += other.vector_instances;
        self.vector_unique_triangles += other.vector_unique_triangles;
        self.vector_execution_triangles += other.vector_execution_triangles;
        self.native_decode_nanoseconds += other.native_decode_nanoseconds;
        self.native_codec_call_nanoseconds += other.native_codec_call_nanoseconds;
        self.native_allocate_nanoseconds += other.native_allocate_nanoseconds;
        self.native_submit_nanoseconds += other.native_submit_nanoseconds;
        self.native_seeks += other.native_seeks;
        self.native_forward_reuses += other.native_forward_reuses;
        self.native_copy_ready_nanoseconds += other.native_copy_ready_nanoseconds;
        self.native_gpu_local_copy_bytes += other.native_gpu_local_copy_bytes;
        self.native_pipe_bytes += other.native_pipe_bytes;
        self.native_upload_encode_nanoseconds += other.native_upload_encode_nanoseconds;
    }
}
