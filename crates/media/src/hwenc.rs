//! Hardware screen encoder.
//!
//! libwebrtc in webrtc-sys 0.3 has no hardware video encoder on Windows (NVENC is Linux
//! only), so on its own the screen would be encoded on the CPU. On Windows this module
//! encodes on the GPU through a Media Foundation encoder MFT (AMD AMF, NVIDIA NVENC or
//! Intel QSV, whichever the driver registers) and hands the access units to LiveKit's
//! pre-encoded pass-through (`VideoEncoderBackend::PreEncoded`). Keyframe (PLI/FIR) and
//! bitrate requests from WebRTC are forwarded to the MFT.

use livekit::options::VideoCodec;
use livekit::webrtc::native::yuv_helper;
use livekit::webrtc::prelude::{I420Buffer, VideoBuffer};

pub(crate) use imp::{probe, HwScreenEncoder};

/// A hardware encoder that opened and produced a keyframe on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HwProbe {
    pub codec: VideoCodec,
    pub encoder: String,
    pub vendor: GpuVendor,
}

/// Encoder settings shared by every (re)opened MFT of one share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HwConfig {
    pub fps: u32,
    pub max_bitrate_bps: u32,
}

/// Seconds between periodic keyframes (PLI requests are answered immediately anyway).
const GOP_SECONDS: u32 = 4;
/// Floor for the bitrate WebRTC's bandwidth estimate can push the encoder down to.
const MIN_BITRATE_BPS: u64 = 500_000;

/// GPU maker of a hardware encoder MFT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl GpuVendor {
    /// From the MFT's `MFT_ENUM_HARDWARE_VENDOR_ID_Attribute` (e.g. `VEN_10DE`).
    pub(crate) fn from_vendor_id(id: &str) -> Self {
        let id = id.to_ascii_uppercase();
        if id.starts_with("VEN_10DE") {
            Self::Nvidia
        } else if id.starts_with("VEN_1002") {
            Self::Amd
        } else if id.starts_with("VEN_8086") {
            Self::Intel
        } else {
            Self::Other
        }
    }

    /// Lower is preferred: discrete GPUs before an integrated Intel GPU on hybrid laptops.
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Nvidia | Self::Amd => 0,
            Self::Intel => 1,
            Self::Other => 2,
        }
    }
}

/// Codecs the hardware path can produce, in the caller's order of preference.
pub(crate) fn hardware_codecs(preferred: &[VideoCodec]) -> Vec<VideoCodec> {
    let mut out = Vec::new();
    for &codec in preferred {
        if matches!(codec, VideoCodec::AV1 | VideoCodec::H264) && !out.contains(&codec) {
            out.push(codec);
        }
    }
    out
}

/// One frame ready for the encoder: NV12 (Y plane, then interleaved UV), tightly packed.
pub(crate) struct Nv12Frame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: i64,
}

impl Nv12Frame {
    /// Converts to NV12, cropping odd dimensions to even (NVENC/AMF reject odd NV12 sizes).
    pub(crate) fn from_i420(buffer: &I420Buffer, timestamp_us: i64) -> Self {
        let (width, height) = (buffer.width() & !1, buffer.height() & !1);
        let luma = (width * height) as usize;
        let mut data = vec![0u8; luma + luma / 2];
        let (dst_y, dst_uv) = data.split_at_mut(luma);
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data();
        yuv_helper::i420_to_nv12(
            y,
            sy,
            u,
            su,
            v,
            sv,
            dst_y,
            width,
            dst_uv,
            width,
            width as i32,
            height as i32,
        );
        Self {
            data,
            width,
            height,
            timestamp_us,
        }
    }
}

/// H.264 Annex-B NAL unit types present in `payload`.
fn h264_nal_types(payload: &[u8]) -> impl Iterator<Item = u8> + '_ {
    let mut i = 0;
    std::iter::from_fn(move || {
        while i + 3 < payload.len() {
            let start = if payload[i..].starts_with(&[0, 0, 1]) {
                3
            } else if payload[i..].starts_with(&[0, 0, 0, 1]) {
                4
            } else {
                i += 1;
                continue;
            };
            i += start;
            if let Some(&header) = payload.get(i) {
                return Some(header & 0x1f);
            }
        }
        None
    })
}

const H264_NAL_IDR: u8 = 5;
const H264_NAL_SPS: u8 = 7;

/// Makes sure an H.264 keyframe carries SPS/PPS: some MFTs only expose them in the output
/// media type (`MF_MT_MPEG_SEQUENCE_HEADER`), and a late viewer cannot decode without them.
fn with_h264_parameter_sets(payload: Vec<u8>, sequence_header: &[u8]) -> Vec<u8> {
    if sequence_header.is_empty() || h264_nal_types(&payload).any(|t| t == H264_NAL_SPS) {
        return payload;
    }
    let mut out = Vec::with_capacity(sequence_header.len() + payload.len());
    out.extend_from_slice(sequence_header);
    out.extend_from_slice(&payload);
    out
}

#[cfg(windows)]
mod imp {
    use std::mem::ManuallyDrop;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use livekit::options::VideoCodec;
    use livekit::webrtc::video_frame::{EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame};
    use livekit::webrtc::video_source::native::NativeVideoSource;
    use livekit::webrtc::video_source::VideoResolution;
    use windows::core::{Interface, Result as WinResult, GUID, PWSTR};
    use windows::Win32::Foundation::VARIANT_TRUE;
    use windows::Win32::Media::MediaFoundation::*;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
    };
    use windows::Win32::System::Variant::{VARIANT, VT_BOOL, VT_UI4};

    use super::{
        h264_nal_types, with_h264_parameter_sets, GpuVendor, HwConfig, HwProbe, Nv12Frame,
        GOP_SECONDS, H264_NAL_IDR, H264_NAL_SPS, MIN_BITRATE_BPS,
    };
    use crate::hub::ScreenFeed;

    /// Frames queued for the encoder thread; older ones are dropped when it falls behind.
    const QUEUE_FRAMES: usize = 2;
    /// How long the probe waits for the first encoded frame.
    const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

    /// COM (MTA) + Media Foundation for the current thread.
    struct MfRuntime {
        com: bool,
    }

    impl MfRuntime {
        fn start() -> WinResult<Self> {
            // SAFETY: plain FFI initialization, balanced in `Drop`.
            unsafe {
                let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
                if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_LITE) {
                    if com {
                        CoUninitialize();
                    }
                    return Err(e);
                }
                Ok(Self { com })
            }
        }
    }

    impl Drop for MfRuntime {
        fn drop(&mut self) {
            // SAFETY: balances `start`.
            unsafe {
                let _ = MFShutdown();
                if self.com {
                    CoUninitialize();
                }
            }
        }
    }

    fn subtype(codec: VideoCodec) -> Option<GUID> {
        match codec {
            VideoCodec::AV1 => Some(MFVideoFormat_AV1),
            VideoCodec::H264 => Some(MFVideoFormat_H264),
            _ => None,
        }
    }

    fn encoded_codec(codec: VideoCodec) -> EncodedVideoCodec {
        match codec {
            VideoCodec::AV1 => EncodedVideoCodec::AV1,
            _ => EncodedVideoCodec::H264,
        }
    }

    fn pack(high: u32, low: u32) -> u64 {
        (u64::from(high) << 32) | u64::from(low)
    }

    fn variant_u32(value: u32) -> VARIANT {
        let mut v = VARIANT::default();
        // SAFETY: writes the tag and the matching union member of a zeroed VARIANT.
        unsafe {
            let inner = &mut *v.Anonymous.Anonymous;
            inner.vt = VT_UI4;
            inner.Anonymous.ulVal = value;
        }
        v
    }

    fn variant_true() -> VARIANT {
        let mut v = VARIANT::default();
        // SAFETY: as in `variant_u32`.
        unsafe {
            let inner = &mut *v.Anonymous.Anonymous;
            inner.vt = VT_BOOL;
            inner.Anonymous.boolVal = VARIANT_TRUE;
        }
        v
    }

    /// Hardware encoder MFTs for `codec` taking NV12, discrete GPUs first, then by merit.
    fn encoders(codec: VideoCodec) -> Vec<(IMFActivate, GpuVendor)> {
        let Some(subtype) = subtype(codec) else {
            return Vec::new();
        };
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };
        let output = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: subtype,
        };
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        let flags = MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0);
        // SAFETY: MFTEnumEx allocates `count` activates with CoTaskMemAlloc; each is taken
        // out of the array before it is freed.
        unsafe {
            if MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                flags,
                Some(&input),
                Some(&output),
                &mut list,
                &mut count,
            )
            .is_err()
                || list.is_null()
            {
                return Vec::new();
            }
            let mut found: Vec<(IMFActivate, GpuVendor)> =
                std::slice::from_raw_parts_mut(list, count as usize)
                    .iter_mut()
                    .filter_map(Option::take)
                    .map(|activate| {
                        let vendor = string_attr(&activate, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute)
                            .map_or(GpuVendor::Other, |id| GpuVendor::from_vendor_id(&id));
                        (activate, vendor)
                    })
                    .collect();
            CoTaskMemFree(Some(list as *const _));
            // Stable: keeps the SORTANDFILTER merit order within each vendor rank.
            found.sort_by_key(|(_, vendor)| vendor.rank());
            found
        }
    }

    fn friendly_name(activate: &IMFActivate) -> String {
        string_attr(activate, &MFT_FRIENDLY_NAME_Attribute)
            .unwrap_or_else(|| "hardware MFT".to_owned())
    }

    fn string_attr(activate: &IMFActivate, key: &GUID) -> Option<String> {
        let mut value = PWSTR::null();
        let mut len = 0u32;
        // SAFETY: the string is allocated by MF and freed right after copying.
        unsafe {
            activate
                .GetAllocatedString(key, &mut value, &mut len)
                .ok()?;
            let out = value.to_string().ok();
            CoTaskMemFree(Some(value.0 as *const _));
            out
        }
    }

    struct EncodedOut {
        payload: Vec<u8>,
        key: bool,
        timestamp_us: i64,
        width: u32,
        height: u32,
    }

    /// One asynchronous hardware encoder MFT at a fixed frame size.
    struct Mft {
        activate: IMFActivate,
        transform: IMFTransform,
        events: IMFMediaEventGenerator,
        codec_api: Option<ICodecAPI>,
        name: String,
        vendor: GpuVendor,
        codec: VideoCodec,
        width: u32,
        height: u32,
        fps: u32,
        in_id: u32,
        out_id: u32,
        provides_samples: bool,
        out_size: u32,
        need_input: u32,
        in_flight: u32,
        bitrate: u32,
        sequence_header: Vec<u8>,
    }

    impl Mft {
        fn open_first(
            codec: VideoCodec,
            width: u32,
            height: u32,
            cfg: HwConfig,
        ) -> WinResult<Self> {
            let mut last = windows::core::Error::from(MF_E_TOPO_CODEC_NOT_FOUND);
            for (activate, vendor) in encoders(codec) {
                let name = friendly_name(&activate);
                match Self::open(activate, name.clone(), vendor, codec, width, height, cfg) {
                    Ok(mft) => return Ok(mft),
                    Err(e) => {
                        tracing::debug!(encoder = %name, ?vendor, codec = codec.as_str(), error = %e, "hardware encoder rejected the format");
                        last = e;
                    }
                }
            }
            Err(last)
        }

        fn open(
            activate: IMFActivate,
            name: String,
            vendor: GpuVendor,
            codec: VideoCodec,
            width: u32,
            height: u32,
            cfg: HwConfig,
        ) -> WinResult<Self> {
            let subtype =
                subtype(codec).ok_or_else(|| windows::core::Error::from(MF_E_INVALIDMEDIATYPE))?;
            let fps = cfg.fps.max(1);
            // SAFETY: COM calls on objects owned by this struct, on its creating thread.
            unsafe {
                let transform: IMFTransform = activate.ActivateObject()?;
                let attributes = transform.GetAttributes()?;
                attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
                let _ = attributes.SetUINT32(&MF_LOW_LATENCY, 1);

                let (mut in_id, mut out_id) = (0u32, 0u32);
                {
                    let (mut ins, mut outs) = ([0u32], [0u32]);
                    if transform.GetStreamIDs(&mut ins, &mut outs).is_ok() {
                        (in_id, out_id) = (ins[0], outs[0]);
                    }
                }

                let mut mft = Self {
                    events: transform.cast()?,
                    codec_api: transform.cast().ok(),
                    activate,
                    transform,
                    name,
                    vendor,
                    codec,
                    width,
                    height,
                    fps,
                    in_id,
                    out_id,
                    provides_samples: false,
                    out_size: 0,
                    need_input: 0,
                    in_flight: 0,
                    bitrate: cfg.max_bitrate_bps,
                    sequence_header: Vec::new(),
                };

                // Real-time settings; must precede the media types on some encoders, while
                // some NVIDIA drivers only accept them once the types are set (retried below).
                let rate_set = mft.set_rate_control();
                mft.set(&CODECAPI_AVLowLatencyMode, variant_true());
                mft.set(&CODECAPI_AVEncMPVDefaultBPictureCount, variant_u32(0));
                let gop_set = mft.set(&CODECAPI_AVEncMPVGOPSize, variant_u32(fps * GOP_SECONDS));
                mft.set(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(mft.bitrate));

                let output = MFCreateMediaType()?;
                output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                output.SetGUID(&MF_MT_SUBTYPE, &subtype)?;
                output.SetUINT32(&MF_MT_AVG_BITRATE, mft.bitrate)?;
                output.SetUINT64(&MF_MT_FRAME_SIZE, pack(width, height))?;
                output.SetUINT64(&MF_MT_FRAME_RATE, pack(fps, 1))?;
                output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
                output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
                if codec == VideoCodec::H264 {
                    // The pass-through negotiates Constrained Baseline (42e01f).
                    let mut result = Err(windows::core::Error::from(MF_E_INVALIDMEDIATYPE));
                    for profile in [
                        eAVEncH264VProfile_ConstrainedBase.0,
                        eAVEncH264VProfile_Base.0,
                        eAVEncH264VProfile_Main.0,
                    ] {
                        output.SetUINT32(&MF_MT_MPEG2_PROFILE, profile as u32)?;
                        result = mft.transform.SetOutputType(out_id, &output, 0);
                        if result.is_ok() {
                            break;
                        }
                    }
                    result?;
                } else {
                    mft.transform.SetOutputType(out_id, &output, 0)?;
                }
                mft.set_input_type()?;
                if !rate_set {
                    mft.set_rate_control();
                }
                if !gop_set {
                    mft.set(&CODECAPI_AVEncMPVGOPSize, variant_u32(fps * GOP_SECONDS));
                }
                // Some encoders only honor the bitrate once the types are set.
                mft.set(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(mft.bitrate));

                let info = mft.transform.GetOutputStreamInfo(out_id)?;
                let provides =
                    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0;
                mft.provides_samples = info.dwFlags & provides as u32 != 0;
                mft.out_size = info.cbSize.max(width * height * 3 / 2);
                mft.refresh_sequence_header();

                mft.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
                mft.transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)?;
                tracing::info!(
                    encoder = %mft.name,
                    vendor = ?mft.vendor,
                    codec = codec.as_str(),
                    width,
                    height,
                    fps,
                    bitrate = mft.bitrate,
                    "hardware screen encoder opened"
                );
                Ok(mft)
            }
        }

        unsafe fn set_input_type(&self) -> WinResult<()> {
            let input = MFCreateMediaType()?;
            input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
            input.SetUINT64(&MF_MT_FRAME_SIZE, pack(self.width, self.height))?;
            input.SetUINT64(&MF_MT_FRAME_RATE, pack(self.fps, 1))?;
            input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
            input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            let direct = self.transform.SetInputType(self.in_id, &input, 0);
            if direct.is_ok() {
                return direct;
            }
            // Fall back to the encoder's own NV12 type with our size and rate.
            let mut index = 0;
            while let Ok(available) = self.transform.GetInputAvailableType(self.in_id, index) {
                index += 1;
                if available.GetGUID(&MF_MT_SUBTYPE).ok() != Some(MFVideoFormat_NV12) {
                    continue;
                }
                available.SetUINT64(&MF_MT_FRAME_SIZE, pack(self.width, self.height))?;
                available.SetUINT64(&MF_MT_FRAME_RATE, pack(self.fps, 1))?;
                return self.transform.SetInputType(self.in_id, &available, 0);
            }
            direct
        }

        /// Low-delay VBR, else CBR. Returns whether the encoder accepted either.
        fn set_rate_control(&self) -> bool {
            self.set(
                &CODECAPI_AVEncCommonRateControlMode,
                variant_u32(eAVEncCommonRateControlMode_LowDelayVBR.0 as u32),
            ) || self.set(
                &CODECAPI_AVEncCommonRateControlMode,
                variant_u32(eAVEncCommonRateControlMode_CBR.0 as u32),
            )
        }

        fn set(&self, api: &GUID, value: VARIANT) -> bool {
            let Some(codec_api) = &self.codec_api else {
                return false;
            };
            // SAFETY: `value` outlives the call; plain-data VARIANTs need no cleanup.
            unsafe { codec_api.SetValue(api, &value).is_ok() }
        }

        fn refresh_sequence_header(&mut self) {
            if self.codec != VideoCodec::H264 {
                return;
            }
            // SAFETY: reads a blob attribute into a buffer of the reported size.
            unsafe {
                let Ok(current) = self.transform.GetOutputCurrentType(self.out_id) else {
                    return;
                };
                let Ok(size) = current.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) else {
                    return;
                };
                let mut blob = vec![0u8; size as usize];
                if current
                    .GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None)
                    .is_ok()
                {
                    self.sequence_header = blob;
                }
            }
        }

        /// Applies a bitrate target from WebRTC, capped by `max_bps`.
        fn apply_rate(&mut self, target_bps: u64, max_bps: u32) {
            let bps = target_bps.clamp(MIN_BITRATE_BPS.min(u64::from(max_bps)), u64::from(max_bps))
                as u32;
            // Ignore jitter below 5% to avoid reconfiguring the encoder on every estimate.
            if bps.abs_diff(self.bitrate) * 20 < self.bitrate {
                return;
            }
            if self.set(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(bps)) {
                self.bitrate = bps;
            }
        }

        fn encode(&mut self, frame: &Nv12Frame, keyframe: bool) -> WinResult<()> {
            if keyframe {
                self.set(&CODECAPI_AVEncVideoForceKeyFrame, variant_u32(1));
            }
            let len = frame.data.len() as u32;
            // SAFETY: the locked buffer holds `len` bytes; it is unlocked before use.
            unsafe {
                let buffer = MFCreateMemoryBuffer(len)?;
                let mut ptr = std::ptr::null_mut();
                buffer.Lock(&mut ptr, None, None)?;
                std::ptr::copy_nonoverlapping(frame.data.as_ptr(), ptr, frame.data.len());
                buffer.Unlock()?;
                buffer.SetCurrentLength(len)?;
                let sample = MFCreateSample()?;
                sample.AddBuffer(&buffer)?;
                sample.SetSampleTime(frame.timestamp_us * 10)?;
                sample.SetSampleDuration(10_000_000 / i64::from(self.fps))?;
                self.transform.ProcessInput(self.in_id, &sample, 0)?;
            }
            self.need_input -= 1;
            self.in_flight += 1;
            Ok(())
        }

        /// Handles every queued MFT event without blocking.
        fn pump(&mut self, deliver: &mut dyn FnMut(EncodedOut)) -> WinResult<()> {
            loop {
                // SAFETY: COM calls on owned objects.
                let event = match unsafe { self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                    Ok(event) => event,
                    Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(()),
                    Err(e) => return Err(e),
                };
                // SAFETY: as above.
                let (kind, status) = unsafe { (event.GetType()?, event.GetStatus()?) };
                status.ok()?;
                if kind == METransformNeedInput.0 as u32 {
                    self.need_input += 1;
                } else if kind == METransformHaveOutput.0 as u32 {
                    if let Some(out) = self.take_output()? {
                        self.in_flight = self.in_flight.saturating_sub(1);
                        deliver(out);
                    }
                }
            }
        }

        fn take_output(&mut self) -> WinResult<Option<EncodedOut>> {
            // SAFETY: the output sample and events are taken back out of the ManuallyDrop
            // fields so they are released exactly once.
            unsafe {
                let sample = if self.provides_samples {
                    None
                } else {
                    let sample = MFCreateSample()?;
                    sample.AddBuffer(&MFCreateMemoryBuffer(self.out_size)?)?;
                    Some(sample)
                };
                let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: self.out_id,
                    pSample: ManuallyDrop::new(sample),
                    dwStatus: 0,
                    pEvents: ManuallyDrop::new(None),
                }];
                let mut status = 0u32;
                let result = self.transform.ProcessOutput(0, &mut buffers, &mut status);
                let sample = ManuallyDrop::take(&mut buffers[0].pSample);
                drop(ManuallyDrop::take(&mut buffers[0].pEvents));
                match result {
                    Ok(()) => {}
                    Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                        let available = self.transform.GetOutputAvailableType(self.out_id, 0)?;
                        self.transform.SetOutputType(self.out_id, &available, 0)?;
                        self.refresh_sequence_header();
                        return Ok(None);
                    }
                    Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
                    Err(e) => return Err(e),
                }
                let Some(sample) = sample else {
                    return Ok(None);
                };
                let buffer = sample.ConvertToContiguousBuffer()?;
                let mut ptr = std::ptr::null_mut();
                let mut len = 0u32;
                buffer.Lock(&mut ptr, None, Some(&mut len))?;
                let mut payload = std::slice::from_raw_parts(ptr, len as usize).to_vec();
                buffer.Unlock()?;
                if payload.is_empty() {
                    return Ok(None);
                }
                let clean_point = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
                let key = match self.codec {
                    VideoCodec::H264 => {
                        clean_point || h264_nal_types(&payload).any(|t| t == H264_NAL_IDR)
                    }
                    _ => clean_point,
                };
                if key && self.codec == VideoCodec::H264 {
                    if self.sequence_header.is_empty() {
                        self.refresh_sequence_header();
                    }
                    payload = with_h264_parameter_sets(payload, &self.sequence_header);
                }
                let timestamp_us = sample.GetSampleTime().map(|t| t / 10).unwrap_or_default();
                Ok(Some(EncodedOut {
                    payload,
                    key,
                    timestamp_us,
                    width: self.width,
                    height: self.height,
                }))
            }
        }
    }

    impl Drop for Mft {
        fn drop(&mut self) {
            // SAFETY: COM calls on owned objects; errors on teardown are irrelevant.
            unsafe {
                let _ = self
                    .transform
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
                let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
                let _ = self.activate.ShutdownObject();
            }
        }
    }

    fn black_frame(width: u32, height: u32, timestamp_us: i64) -> Nv12Frame {
        let luma = (width * height) as usize;
        let mut data = vec![16u8; luma + luma / 2];
        data[luma..].fill(128);
        Nv12Frame {
            data,
            width,
            height,
            timestamp_us,
        }
    }

    /// Encodes a few frames to check the MFT really works (opening alone can succeed on
    /// encoders that later reject system-memory input) and that its first output is a
    /// keyframe a late viewer can decode. The MFT is released before returning.
    fn probe_codec(
        codec: VideoCodec,
        width: u32,
        height: u32,
        cfg: HwConfig,
    ) -> WinResult<(String, GpuVendor, Vec<EncodedOut>)> {
        let mut mft = Mft::open_first(codec, width, height, cfg)?;
        let deadline = Instant::now() + PROBE_TIMEOUT;
        let mut out = Vec::new();
        let mut sent = 0i64;
        while out.is_empty() && Instant::now() < deadline {
            mft.pump(&mut |frame| out.push(frame))?;
            if mft.need_input > 0 && sent < 4 {
                let frame =
                    black_frame(width, height, sent * 1_000_000 / i64::from(cfg.fps.max(1)));
                mft.encode(&frame, sent == 0)?;
                sent += 1;
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let Some(first) = out.first() else {
            return Err(windows::core::Error::from(MF_E_TRANSFORM_NEED_MORE_INPUT));
        };
        let decodable = first.key
            && (codec != VideoCodec::H264
                || h264_nal_types(&first.payload).any(|t| t == H264_NAL_SPS));
        if !decodable {
            return Err(windows::core::Error::from(MF_E_INVALIDMEDIATYPE));
        }
        Ok((mft.name.clone(), mft.vendor, out))
    }

    /// The first codec of `codecs` with a working hardware encoder on this machine.
    /// Blocking (opens and test-encodes on the GPU).
    pub(crate) fn probe(
        codecs: &[VideoCodec],
        width: u32,
        height: u32,
        cfg: HwConfig,
    ) -> Option<HwProbe> {
        let _mf = MfRuntime::start().ok()?;
        for &codec in codecs {
            match probe_codec(codec, width, height, cfg) {
                Ok((encoder, vendor, _)) => {
                    return Some(HwProbe {
                        codec,
                        encoder,
                        vendor,
                    })
                }
                Err(e) => {
                    tracing::info!(codec = codec.as_str(), error = %e, "no usable hardware encoder")
                }
            }
        }
        None
    }

    /// Hardware encoder thread feeding a pre-encoded [`NativeVideoSource`]. Stops when
    /// this handle and every [`ScreenFeed`] from it are dropped.
    pub(crate) struct HwScreenEncoder {
        frames: SyncSender<Nv12Frame>,
        failed: Arc<AtomicBool>,
    }

    impl HwScreenEncoder {
        pub(crate) fn start(codec: VideoCodec, cfg: HwConfig, source: NativeVideoSource) -> Self {
            let (frames, rx) = mpsc::sync_channel(QUEUE_FRAMES);
            let failed = Arc::new(AtomicBool::new(false));
            let flag = failed.clone();
            let spawned = std::thread::Builder::new()
                .name("screen-hwenc".into())
                .spawn(move || {
                    if let Err(e) = run(codec, cfg, &source, &rx) {
                        tracing::error!(codec = codec.as_str(), error = %e, "hardware screen encoder failed");
                        flag.store(true, Ordering::Relaxed);
                    }
                });
            if let Err(e) = spawned {
                tracing::error!(error = %e, "failed to start the hardware encoder thread");
                failed.store(true, Ordering::Relaxed);
            }
            Self { frames, failed }
        }

        /// Hub feed: converts frames to NV12 and queues them (dropping when the queue is full).
        pub(crate) fn feed(&self) -> ScreenFeed {
            let frames = self.frames.clone();
            Arc::new(move |buffer, timestamp_us| {
                let _ = frames.try_send(Nv12Frame::from_i420(buffer, timestamp_us));
            })
        }

        /// Whether the encoder stopped with an error (the share should fall back to software).
        pub(crate) fn failed(&self) -> bool {
            self.failed.load(Ordering::Relaxed)
        }
    }

    fn run(
        codec: VideoCodec,
        cfg: HwConfig,
        source: &NativeVideoSource,
        frames: &mpsc::Receiver<Nv12Frame>,
    ) -> WinResult<()> {
        let _mf = MfRuntime::start()?;
        let mut mft: Option<Mft> = None;
        let mut pending: Option<Nv12Frame> = None;
        let mut deliver = |out: EncodedOut| {
            source.capture_encoded_frame(&EncodedVideoFrame {
                codec: encoded_codec(codec),
                payload: &out.payload,
                timestamp_us: out.timestamp_us,
                frame_type: if out.key {
                    EncodedFrameType::Key
                } else {
                    EncodedFrameType::Delta
                },
                resolution: VideoResolution {
                    width: out.width,
                    height: out.height,
                },
                frame_metadata: None,
            });
        };
        loop {
            if let Some(m) = mft.as_mut() {
                m.pump(&mut deliver)?;
            }
            if let Some(frame) = pending.take() {
                let size = (frame.width, frame.height);
                if mft.as_ref().is_none_or(|m| (m.width, m.height) != size) {
                    drop(mft.take());
                    mft = Some(Mft::open_first(codec, size.0, size.1, cfg)?);
                }
                let m = mft.as_mut().expect("encoder opened above");
                if m.need_input > 0 {
                    if let Some(rate) = source.take_rate_control_request() {
                        m.apply_rate(rate.target_bitrate_bps, cfg.max_bitrate_bps);
                    }
                    m.encode(&frame, source.take_keyframe_request())?;
                } else {
                    pending = Some(frame);
                }
            }
            let busy = pending.is_some() || mft.as_ref().is_some_and(|m| m.in_flight > 0);
            let wait = Duration::from_millis(if busy { 1 } else { 50 });
            if pending.is_some() {
                std::thread::sleep(wait);
                continue;
            }
            match frames.recv_timeout(wait) {
                Ok(frame) => pending = Some(frame),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// Runs on the real GPU; skipped when the machine has no hardware encoder MFT.
        #[test]
        fn hardware_encoders_produce_decodable_keyframes() {
            let cfg = HwConfig {
                fps: 30,
                max_bitrate_bps: 2_000_000,
            };
            let Ok(_mf) = MfRuntime::start() else {
                eprintln!("SKIPPED: Media Foundation unavailable");
                return;
            };
            for codec in [VideoCodec::H264, VideoCodec::AV1] {
                match probe_codec(codec, 640, 360, cfg) {
                    Ok((name, vendor, out)) => {
                        let first = &out[0];
                        assert!(
                            first.key,
                            "{name}: first {} frame must be a keyframe",
                            codec.as_str()
                        );
                        assert_eq!((first.width, first.height), (640, 360));
                        if codec == VideoCodec::H264 {
                            let types: Vec<u8> = h264_nal_types(&first.payload).collect();
                            assert!(
                                types.contains(&7) && types.contains(&5),
                                "{name}: SPS+IDR expected, got {types:?}"
                            );
                        }
                        eprintln!(
                            "{} via {name} ({vendor:?}): {} bytes",
                            codec.as_str(),
                            first.payload.len()
                        );
                    }
                    Err(e) => eprintln!("SKIPPED {}: {e}", codec.as_str()),
                }
            }
        }

        /// Repeated probes must not leak encoder sessions (GeForce cards cap NVENC sessions).
        #[test]
        fn probe_releases_the_encoder_before_returning() {
            let cfg = HwConfig {
                fps: 30,
                max_bitrate_bps: 2_000_000,
            };
            let Some(first) = probe(&[VideoCodec::H264], 640, 360, cfg) else {
                eprintln!("SKIPPED: no hardware H.264 encoder");
                return;
            };
            eprintln!("{} ({:?})", first.encoder, first.vendor);
            for _ in 0..12 {
                assert_eq!(
                    probe(&[VideoCodec::H264], 640, 360, cfg).map(|p| p.encoder),
                    Some(first.encoder.clone())
                );
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use livekit::options::VideoCodec;
    use livekit::webrtc::video_source::native::NativeVideoSource;

    use super::{HwConfig, HwProbe};
    use crate::hub::ScreenFeed;

    pub(crate) fn probe(_: &[VideoCodec], _: u32, _: u32, _: HwConfig) -> Option<HwProbe> {
        None
    }

    pub(crate) struct HwScreenEncoder;

    impl HwScreenEncoder {
        pub(crate) fn start(_: VideoCodec, _: HwConfig, _: NativeVideoSource) -> Self {
            Self
        }

        pub(crate) fn feed(&self) -> ScreenFeed {
            std::sync::Arc::new(|_, _| {})
        }

        pub(crate) fn failed(&self) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_codecs_keep_order_and_skip_unsupported() {
        assert_eq!(
            hardware_codecs(&[VideoCodec::AV1, VideoCodec::H264, VideoCodec::VP8]),
            vec![VideoCodec::AV1, VideoCodec::H264]
        );
        assert_eq!(
            hardware_codecs(&[VideoCodec::VP8, VideoCodec::H264, VideoCodec::H264]),
            vec![VideoCodec::H264]
        );
    }

    #[test]
    fn i420_converts_to_packed_nv12() {
        let mut i420 = I420Buffer::new(4, 2);
        {
            let (y, u, v) = i420.data_mut();
            y.fill(10);
            u.fill(20);
            v.fill(30);
        }
        let frame = Nv12Frame::from_i420(&i420, 7);
        assert_eq!((frame.width, frame.height, frame.timestamp_us), (4, 2, 7));
        assert_eq!(&frame.data[..8], &[10; 8]);
        assert_eq!(&frame.data[8..], &[20, 30, 20, 30]);
    }

    #[test]
    fn vendor_ids_map_to_gpu_vendors() {
        assert_eq!(GpuVendor::from_vendor_id("VEN_10DE"), GpuVendor::Nvidia);
        assert_eq!(
            GpuVendor::from_vendor_id("ven_1002&DEV_7590"),
            GpuVendor::Amd
        );
        assert_eq!(GpuVendor::from_vendor_id("VEN_8086"), GpuVendor::Intel);
        assert_eq!(GpuVendor::from_vendor_id(""), GpuVendor::Other);
    }

    #[test]
    fn encoders_prefer_discrete_vendors() {
        let mut list = [
            (GpuVendor::Intel, "qsv"),
            (GpuVendor::Nvidia, "nvenc"),
            (GpuVendor::Other, "x"),
        ];
        list.sort_by_key(|(v, _)| v.rank());
        assert_eq!(
            list.iter().map(|(_, n)| *n).collect::<Vec<_>>(),
            ["nvenc", "qsv", "x"]
        );
    }

    #[test]
    fn odd_frames_are_cropped_to_even() {
        let buffer = I420Buffer::new(1365, 767);
        let frame = Nv12Frame::from_i420(&buffer, 0);
        assert_eq!((frame.width, frame.height), (1364, 766));
        assert_eq!(frame.data.len(), 1364 * 766 * 3 / 2);
    }

    #[test]
    fn keyframes_get_parameter_sets_only_when_missing() {
        let header = [0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2];
        let idr = vec![0, 0, 0, 1, 0x65, 9];
        let with = with_h264_parameter_sets(idr.clone(), &header);
        assert_eq!(h264_nal_types(&with).collect::<Vec<_>>(), vec![7, 8, 5]);
        assert_eq!(with_h264_parameter_sets(with.clone(), &header), with);
        assert_eq!(with_h264_parameter_sets(idr.clone(), &[]), idr);
        assert_eq!(
            h264_nal_types(&[0, 0, 1, 0x41, 0, 0, 1]).collect::<Vec<_>>(),
            vec![1]
        );
    }
}
