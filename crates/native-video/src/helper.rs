//! Supervised process boundary for potentially blocking FFmpeg/CUDA calls.
//! No CPU frame transfer API is called. Errors terminate the worker, never fallback.
mod ffi {
    #![allow(
        non_upper_case_globals,
        non_camel_case_types,
        non_snake_case,
        dead_code,
        improper_ctypes,
        unnecessary_transmutes,
        unused_imports,
        clippy::ptr_offset_with_cast,
        clippy::upper_case_acronyms,
        clippy::type_complexity
    )]
    include!(concat!(env!("OUT_DIR"), "/ffi.rs"));
}
use ffi::*;
use std::{
    ffi::{CStr, CString},
    os::{
        fd::{AsRawFd, IntoRawFd},
        unix::net::UnixDatagram,
    },
    ptr,
};
fn av(code: i32) -> Result<(), String> {
    if code < 0 {
        Err(format!("FFmpeg error {code}"))
    } else {
        Ok(())
    }
}
fn cu(code: CUresult) -> Result<(), String> {
    if code != 0 {
        Err(format!("CUDA error {code}"))
    } else {
        Ok(())
    }
}
unsafe extern "C" fn hardware_format(
    _: *mut AVCodecContext,
    formats: *const AVPixelFormat,
) -> AVPixelFormat {
    unsafe {
        let mut p = formats;
        while *p != AVPixelFormat_AV_PIX_FMT_NONE {
            if *p == AVPixelFormat_AV_PIX_FMT_CUDA {
                return *p;
            }
            p = p.add(1);
        }
    }
    AVPixelFormat_AV_PIX_FMT_NONE
}
struct Decoder {
    format: *mut AVFormatContext,
    codec: *mut AVCodecContext,
    device: *mut AVBufferRef,
    packet: *mut AVPacket,
    frame: *mut AVFrame,
    stream: i32,
    width: u32,
    height: u32,
    rate: [u32; 2],
    frames: u32,
    next: Option<u32>,
    eof: bool,
}
impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            av_frame_free(&mut self.frame);
            av_packet_free(&mut self.packet);
            avcodec_free_context(&mut self.codec);
            avformat_close_input(&mut self.format);
            av_buffer_unref(&mut self.device);
        }
    }
}
impl Decoder {
    unsafe fn open(
        path: &str,
        uuid: [u8; 16],
        width: u32,
        height: u32,
        rate: [u32; 2],
        frames: u32,
    ) -> Result<Self, String> {
        unsafe {
            if CStr::from_ptr(av_version_info()).to_bytes() != b"6.1.1" {
                return Err("unapproved FFmpeg version".into());
            }
            for (version, major, config, license) in [
                (
                    avcodec_version(),
                    60,
                    avcodec_configuration(),
                    avcodec_license(),
                ),
                (
                    avformat_version(),
                    60,
                    avformat_configuration(),
                    avformat_license(),
                ),
                (
                    avutil_version(),
                    58,
                    avutil_configuration(),
                    avutil_license(),
                ),
            ] {
                let config = CStr::from_ptr(config).to_string_lossy();
                let license = CStr::from_ptr(license).to_string_lossy();
                if version >> 16 != major
                    || config.contains("--enable-gpl")
                    || config.contains("--enable-nonfree")
                    || config.contains("--enable-version3")
                    || !config.contains("--disable-everything")
                    || license != "LGPL version 2.1 or later"
                {
                    return Err("unapproved FFmpeg ABI/configuration/license".into());
                }
            }
            if !(48..=4096).contains(&width)
                || !(16..=4096).contains(&height)
                || !width.is_multiple_of(2)
                || !height.is_multiple_of(2)
                || u64::from(width) * u64::from(height) > 4_194_304
                || rate.contains(&0)
                || frames == 0
                || frames > 18000
            {
                return Err("unsupported native H264 geometry/rate/duration".into());
            }
            cu(cuInit(0))?;
            let mut count = 0;
            cu(cuDeviceGetCount(&mut count))?;
            let mut ordinal = None;
            for n in 0..count {
                let mut id: CUuuid = std::mem::zeroed();
                cu(cuDeviceGetUuid_v2(&mut id, n))?;
                if id.bytes.map(|b| b as u8) == uuid {
                    ordinal = Some(n);
                    break;
                }
            }
            let ordinal = ordinal.ok_or("no CUDA device matches Vulkan UUID")?;
            let mut d = Self {
                format: ptr::null_mut(),
                codec: ptr::null_mut(),
                device: ptr::null_mut(),
                packet: ptr::null_mut(),
                frame: ptr::null_mut(),
                stream: 0,
                width,
                height,
                rate,
                frames,
                next: None,
                eof: false,
            };
            let name = CString::new(ordinal.to_string()).unwrap();
            av(av_hwdevice_ctx_create(
                &mut d.device,
                AVHWDeviceType_AV_HWDEVICE_TYPE_CUDA,
                name.as_ptr(),
                ptr::null_mut(),
                0,
            ))?;
            let path = CString::new(path).map_err(|_| "source path contains NUL")?;
            av(avformat_open_input(
                &mut d.format,
                path.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
            ))?;
            av(avformat_find_stream_info(d.format, ptr::null_mut()))?;
            d.stream = av_find_best_stream(
                d.format,
                AVMediaType_AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                ptr::null_mut(),
                0,
            );
            av(d.stream)?;
            let stream = *(*d.format).streams.add(d.stream as usize);
            if (*(*stream).codecpar).codec_id != AVCodecID_AV_CODEC_ID_H264 {
                return Err("native helper requires H264".into());
            }
            let codec = avcodec_find_decoder(AVCodecID_AV_CODEC_ID_H264);
            if codec.is_null() {
                return Err("H264 decoder unavailable".into());
            }
            d.codec = avcodec_alloc_context3(codec);
            if d.codec.is_null() {
                return Err("codec allocation failed".into());
            }
            av(avcodec_parameters_to_context(d.codec, (*stream).codecpar))?;
            (*d.codec).hw_device_ctx = av_buffer_ref(d.device);
            (*d.codec).get_format = Some(hardware_format);
            (*d.codec).thread_count = 1;
            (*d.codec).extra_hw_frames = 2;
            av(avcodec_open2(d.codec, codec, ptr::null_mut()))?;
            d.packet = av_packet_alloc();
            d.frame = av_frame_alloc();
            if d.packet.is_null() || d.frame.is_null() {
                return Err("decode allocation failed".into());
            }
            Ok(d)
        }
    }
    unsafe fn decode(&mut self, target: u32) -> Result<(), String> {
        unsafe {
            if target >= self.frames {
                return Err("native request outside source".into());
            }
            let stream = *(*self.format).streams.add(self.stream as usize);
            let tb = (*stream).time_base;
            if tb.num <= 0 || tb.den <= 0 {
                return Err("invalid stream time base".into());
            }
            if !self.next.is_some_and(|n| n <= target && target - n <= 32) {
                let numerator = i128::from(target) * i128::from(self.rate[1]) * i128::from(tb.den);
                let denominator = i128::from(self.rate[0]) * i128::from(tb.num);
                if numerator % denominator != 0 {
                    return Err("nonintegral CFR timestamp".into());
                }
                av(av_seek_frame(
                    self.format,
                    self.stream,
                    (numerator / denominator)
                        .try_into()
                        .map_err(|_| "seek overflow")?,
                    AVSEEK_FLAG_BACKWARD as i32,
                ))?;
                avcodec_flush_buffers(self.codec);
                self.eof = false;
            }
            for _ in 0..(18000 * 4) {
                av_frame_unref(self.frame);
                let result = avcodec_receive_frame(self.codec, self.frame);
                if result == 0 {
                    let f = &*self.frame;
                    let n = i128::from(f.best_effort_timestamp)
                        * i128::from(tb.num)
                        * i128::from(self.rate[0]);
                    let den = i128::from(tb.den) * i128::from(self.rate[1]);
                    if n < 0 || n % den != 0 {
                        return Err("decoded frame PTS violates verified CFR".into());
                    }
                    let index =
                        u32::try_from(n / den).map_err(|_| "decoded frame index overflow")?;
                    if index < target {
                        continue;
                    }
                    if index != target {
                        return Err("decoder skipped requested PTS".into());
                    }
                    if f.format != AVPixelFormat_AV_PIX_FMT_CUDA
                        || f.width != self.width as i32
                        || f.height != self.height as i32
                        || f.crop_top != 0
                        || f.crop_bottom != 0
                        || f.crop_left != 0
                        || f.crop_right != 0
                        || f.interlaced_frame != 0
                        || f.color_range != AVColorRange_AVCOL_RANGE_MPEG
                        || f.colorspace != AVColorSpace_AVCOL_SPC_BT709
                        || f.color_primaries != AVColorPrimaries_AVCOL_PRI_BT709
                        || f.color_trc != AVColorTransferCharacteristic_AVCOL_TRC_BT709
                        || f.chroma_location != AVChromaLocation_AVCHROMA_LOC_LEFT
                        || f.hw_frames_ctx.is_null()
                    {
                        return Err(
                            "decoded surface violates native CUDA limited709 progressive profile"
                                .into(),
                        );
                    }
                    let frames = &*((*f.hw_frames_ctx).data.cast::<AVHWFramesContext>());
                    if frames.sw_format != AVPixelFormat_AV_PIX_FMT_NV12
                        || f.linesize[0] < self.width as i32
                        || f.linesize[1] < self.width as i32
                        || f.data[0].is_null()
                        || f.data[1].is_null()
                    {
                        return Err("invalid NV12 surface layout".into());
                    }
                    self.next = Some(target + 1);
                    return Ok(());
                }
                if result != -libc::EAGAIN {
                    return Err(format!("native decode ended before target: {result}"));
                }
                loop {
                    av_packet_unref(self.packet);
                    let read = av_read_frame(self.format, self.packet);
                    if read < 0 {
                        if self.eof {
                            return Err("unexpected decoder EOF".into());
                        }
                        self.eof = true;
                        av(avcodec_send_packet(self.codec, ptr::null()))?;
                        break;
                    }
                    if (*self.packet).stream_index == self.stream {
                        av(avcodec_send_packet(self.codec, self.packet))?;
                        break;
                    }
                }
            }
            Err("native decode preroll budget exceeded".into())
        }
    }
    unsafe fn copy(
        &mut self,
        fd: std::os::fd::OwnedFd,
        allocation: u64,
        bytes: u64,
    ) -> Result<(), String> {
        unsafe {
            let stride = u64::from(self.width).div_ceil(4) * 4;
            let expected = stride * u64::from(self.height) * 3 / 2;
            if bytes != expected || allocation < bytes || allocation > 64 * 1024 * 1024 {
                return Err("invalid export allocation layout".into());
            }
            let hw = &*((*self.device).data.cast::<AVHWDeviceContext>());
            let cuda = &*(hw.hwctx.cast::<AVCUDADeviceContext>());
            cu(cuCtxPushCurrent_v2(cuda.cuda_ctx))?;
            // RAII unmaps CUDA imports before acknowledgment, including errors.
            struct Import {
                memory: CUexternalMemory,
                pointer: CUdeviceptr,
            }
            impl Drop for Import {
                fn drop(&mut self) {
                    unsafe {
                        cuCtxSynchronize();
                        if self.pointer != 0 {
                            cuMemFree_v2(self.pointer);
                        }
                        if !self.memory.is_null() {
                            cuDestroyExternalMemory(self.memory);
                        }
                        let mut old = ptr::null_mut();
                        cuCtxPopCurrent_v2(&mut old);
                    }
                }
            }
            let mut import = Import {
                memory: ptr::null_mut(),
                pointer: 0,
            };
            let mut desc: CUDA_EXTERNAL_MEMORY_HANDLE_DESC = std::mem::zeroed();
            desc.type_ = CUexternalMemoryHandleType_enum_CU_EXTERNAL_MEMORY_HANDLE_TYPE_OPAQUE_FD;
            desc.handle.fd = fd.as_raw_fd();
            desc.size = allocation;
            desc.flags = CUDA_EXTERNAL_MEMORY_DEDICATED;
            cu(cuImportExternalMemory(&mut import.memory, &desc))?;
            let _ = fd.into_raw_fd();
            let mapping = CUDA_EXTERNAL_MEMORY_BUFFER_DESC {
                size: bytes,
                ..std::mem::zeroed()
            };
            cu(cuExternalMemoryGetMappedBuffer(
                &mut import.pointer,
                import.memory,
                &mapping,
            ))?;
            cu(cuMemsetD8Async(
                import.pointer,
                0,
                bytes as usize,
                cuda.stream,
            ))?;
            for plane in 0..2 {
                let mut copy: CUDA_MEMCPY2D = std::mem::zeroed();
                copy.srcMemoryType = CUmemorytype_enum_CU_MEMORYTYPE_DEVICE;
                copy.srcDevice = (*self.frame).data[plane] as CUdeviceptr;
                copy.srcPitch = (*self.frame).linesize[plane] as usize;
                copy.dstMemoryType = CUmemorytype_enum_CU_MEMORYTYPE_DEVICE;
                copy.dstDevice = import.pointer
                    + if plane == 0 {
                        0
                    } else {
                        stride * u64::from(self.height)
                    };
                copy.dstPitch = stride as usize;
                copy.WidthInBytes = self.width as usize;
                copy.Height = (self.height / if plane == 0 { 1 } else { 2 }) as usize;
                cu(cuMemcpy2DAsync_v2(&copy, cuda.stream))?;
            }
            cu(cuStreamSynchronize(cuda.stream))?;
            av_frame_unref(self.frame);
            cu(cuMemFree_v2(import.pointer))?;
            import.pointer = 0;
            cu(cuDestroyExternalMemory(import.memory))?;
            import.memory = ptr::null_mut();
            drop(import);
            Ok(())
        }
    }
}
fn run() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() != 11 {
        return Err("native helper requires private socket/source/profile/UUID arguments".into());
    }
    let socket = UnixDatagram::bind(&a[2]).map_err(|e| e.to_string())?;
    socket.connect(&a[1]).map_err(|e| e.to_string())?;
    let parse = |i: usize| {
        a[i].parse::<u32>()
            .map_err(|_| "invalid helper metadata".to_owned())
    };
    if a[9].len() != 32 || a[10] != fold_native_video::PROTOCOL_NAME {
        return Err("invalid native protocol/device".into());
    }
    let mut uuid = [0; 16];
    for (i, b) in uuid.iter_mut().enumerate() {
        *b = u8::from_str_radix(&a[9][i * 2..i * 2 + 2], 16).map_err(|_| "invalid UUID")?;
    }
    let mut decoder = unsafe {
        Decoder::open(
            &a[3],
            uuid,
            parse(4)?,
            parse(5)?,
            [parse(6)?, parse(7)?],
            parse(8)?,
        )?
    };
    socket
        .send(fold_native_video::READY_MESSAGE)
        .map_err(|e| e.to_string())?;
    loop {
        let mut bytes = [0; 24];
        let (n, fd) =
            fold_native_video::receive_fd(&socket, &mut bytes).map_err(|e| e.to_string())?;
        if n != 24 {
            return Err("invalid request size".into());
        }
        let value = |offset| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        let frame = u32::try_from(value(0)).map_err(|_| "frame overflow")?;
        // Wall-clock attribution, including blocking host/driver calls. No
        // physical NVDEC/copy-engine GPU timing is inferred from these values.
        let begin = std::time::Instant::now();
        unsafe {
            decoder.decode(frame)?;
        }
        let codec_call_nanoseconds = begin.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let begin = std::time::Instant::now();
        unsafe {
            decoder.copy(fd, value(8), value(16))?;
        }
        let copy_ready_nanoseconds = begin.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        socket
            .send(&fold_native_video::acknowledgment(
                frame,
                codec_call_nanoseconds,
                copy_ready_nanoseconds,
            ))
            .map_err(|e| e.to_string())?;
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("native video: {error}");
        std::process::exit(1);
    }
}
