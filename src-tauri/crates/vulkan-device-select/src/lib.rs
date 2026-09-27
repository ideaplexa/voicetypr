//! Pure, platform-agnostic Vulkan device ranking for the Whisper GPU sidecar.
//!
//! The actual Vulkan enumeration (via `ash`) lives in the Windows-only sidecar
//! binary; this crate holds only the vendor/type ranking so it compiles and is
//! unit-tested on any host (macOS CI included), keeping the crash-prone Vulkan
//! calls out of the main process and out of the test build.

/// Vulkan physical device class, collapsed to what ranking cares about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulkanDeviceType {
    Other,
    Integrated,
    Discrete,
}

/// One enumerated Vulkan device, reduced to the fields ranking needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulkanDeviceDescriptor {
    /// Original Vulkan physical-device index (what `GGML_VK_VISIBLE_DEVICES` expects).
    pub index: usize,
    pub vendor_id: u32,
    pub device_type: VulkanDeviceType,
    pub device_local_heap_bytes: u64,
    pub device_name: String,
}

pub const VENDOR_ID_NVIDIA: u32 = 0x10DE;
pub const VENDOR_ID_AMD: u32 = 0x1002;

/// Lower rank = more preferred. Discrete NVIDIA/AMD beat other discrete, then
/// integrated, then anything else.
pub fn device_type_rank(vendor_id: u32, device_type: VulkanDeviceType) -> u8 {
    match device_type {
        VulkanDeviceType::Discrete
            if vendor_id == VENDOR_ID_NVIDIA || vendor_id == VENDOR_ID_AMD =>
        {
            0
        }
        VulkanDeviceType::Discrete => 1,
        VulkanDeviceType::Integrated => 2,
        VulkanDeviceType::Other => 3,
    }
}

/// Pick the preferred device's original Vulkan index: best rank first, then the
/// largest device-local heap, then the lowest index for determinism.
pub fn select_preferred_device_index(devices: &[VulkanDeviceDescriptor]) -> Option<usize> {
    devices
        .iter()
        .min_by(|left, right| {
            let left_rank = device_type_rank(left.vendor_id, left.device_type);
            let right_rank = device_type_rank(right.vendor_id, right.device_type);
            left_rank
                .cmp(&right_rank)
                .then_with(|| {
                    right
                        .device_local_heap_bytes
                        .cmp(&left.device_local_heap_bytes)
                })
                .then_with(|| left.index.cmp(&right.index))
        })
        .map(|device| device.index)
}

/// Stable human label for logs.
pub fn device_type_label(device_type: VulkanDeviceType) -> &'static str {
    match device_type {
        VulkanDeviceType::Discrete => "discrete",
        VulkanDeviceType::Integrated => "integrated",
        VulkanDeviceType::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(
        index: usize,
        vendor_id: u32,
        device_type: VulkanDeviceType,
        heap: u64,
    ) -> VulkanDeviceDescriptor {
        VulkanDeviceDescriptor {
            index,
            vendor_id,
            device_type,
            device_local_heap_bytes: heap,
            device_name: format!("dev{index}"),
        }
    }

    #[test]
    fn prefers_nvidia_discrete_over_intel_integrated() {
        // Vulkan0 = Intel iGPU, Vulkan1 = NVIDIA dGPU (Steve's laptop layout).
        let devices = vec![
            dev(0, 0x8086, VulkanDeviceType::Integrated, 2 << 30),
            dev(1, VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete, 8 << 30),
        ];
        assert_eq!(select_preferred_device_index(&devices), Some(1));
    }

    #[test]
    fn prefers_amd_discrete_over_intel_integrated() {
        let devices = vec![
            dev(0, 0x8086, VulkanDeviceType::Integrated, 2 << 30),
            dev(1, VENDOR_ID_AMD, VulkanDeviceType::Discrete, 8 << 30),
        ];
        assert_eq!(select_preferred_device_index(&devices), Some(1));
    }

    #[test]
    fn falls_back_to_single_integrated_device() {
        let devices = vec![dev(0, 0x8086, VulkanDeviceType::Integrated, 2 << 30)];
        assert_eq!(select_preferred_device_index(&devices), Some(0));
    }

    #[test]
    fn empty_device_list_selects_nothing() {
        assert_eq!(select_preferred_device_index(&[]), None);
    }

    #[test]
    fn largest_heap_breaks_tie_between_same_rank_discrete() {
        // Two NVIDIA discrete GPUs (equal rank) -> larger VRAM wins.
        let devices = vec![
            dev(0, VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete, 6 << 30),
            dev(1, VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete, 12 << 30),
        ];
        assert_eq!(select_preferred_device_index(&devices), Some(1));
    }

    #[test]
    fn lowest_index_breaks_tie_when_rank_and_heap_equal() {
        let devices = vec![
            dev(0, VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete, 8 << 30),
            dev(1, VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete, 8 << 30),
        ];
        assert_eq!(select_preferred_device_index(&devices), Some(0));
    }

    #[test]
    fn discrete_beats_integrated_even_with_smaller_heap() {
        // A dGPU with less reported device-local heap still beats an iGPU.
        let devices = vec![
            dev(0, 0x8086, VulkanDeviceType::Integrated, 16 << 30),
            dev(1, VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete, 4 << 30),
        ];
        assert_eq!(select_preferred_device_index(&devices), Some(1));
    }

    #[test]
    fn non_nvidia_amd_discrete_beats_integrated_but_ranks_below_known_vendors() {
        // e.g. an Intel Arc discrete (vendor 0x8086, DISCRETE) is rank 1.
        assert_eq!(device_type_rank(0x8086, VulkanDeviceType::Discrete), 1);
        assert_eq!(
            device_type_rank(VENDOR_ID_NVIDIA, VulkanDeviceType::Discrete),
            0
        );
        let devices = vec![
            dev(0, 0x8086, VulkanDeviceType::Integrated, 2 << 30),
            dev(1, 0x8086, VulkanDeviceType::Discrete, 8 << 30),
        ];
        assert_eq!(select_preferred_device_index(&devices), Some(1));
    }

    #[test]
    fn labels_are_stable() {
        assert_eq!(device_type_label(VulkanDeviceType::Discrete), "discrete");
        assert_eq!(
            device_type_label(VulkanDeviceType::Integrated),
            "integrated"
        );
        assert_eq!(device_type_label(VulkanDeviceType::Other), "other");
    }
}
