use std::os::fd::AsRawFd;

use bytemuck::{Pod, Zeroable};

use crate::{
    ec_command::{ec_command_bytemuck, ec_command_with_dynamic_output_size},
    EcCmdResult,
};

use super::{get_cmd_versions::ec_cmd_get_cmd_versions, CrosEcCmd};

#[repr(C, align(4))]
#[derive(Pod, Zeroable, Clone, Copy, Debug, Default)]
pub struct EcResponseFpInfo {
    pub vendor_id: u32,
    pub product_id: u32,
    pub model_id: u32,
    pub version: u32,
    /// The size of the PGM image, in bytes
    pub frame_size: u32,
    pub pixel_format: u32,
    pub width: u16,
    pub height: u16,
    pub bpp: u16,
    pub errors: u16,
    /// The template size, in bytes
    pub template_size: u32,
    /// The maximum number of templates the FP can store and match at once
    pub template_max: u16,
    /// The number of templates loaded into the FP
    pub template_valid: u16,
    /// The first bit (the rightmost) represents template 0, the 2nd bit form the right represents template 1, etc.
    /// If the bit is 1, that means that the template has been updated by the FP and the updated version has not been downloaded yet.
    pub template_dirty: u32,
    /// This version could increase after an update to the FP firmware
    pub template_version: u32,
}

impl EcResponseFpInfo {
    pub(crate) fn get_simple_image_size(&self) -> usize {
        (self.width as usize) * (self.height as usize) * (self.bpp as usize) / 8
    }
}

pub fn fp_info<File: AsRawFd>(file: &mut File) -> EcCmdResult<EcResponseFpInfo> {
    let fd = file.as_raw_fd();
    let versions = ec_cmd_get_cmd_versions(file, CrosEcCmd::FpInfo).unwrap_or(0);

    // Support Version 2 (bit 2 = 1, mask 0x4)
    if versions & (1 << 2) != 0 {
        let raw = ec_command_with_dynamic_output_size(CrosEcCmd::FpInfo, 2, &[], 196, fd)?;
        if raw.len() >= 36 {
            let mut info = EcResponseFpInfo {
                vendor_id: u32::from_le_bytes(raw[0..4].try_into().unwrap()),
                product_id: u32::from_le_bytes(raw[4..8].try_into().unwrap()),
                model_id: u32::from_le_bytes(raw[8..12].try_into().unwrap()),
                version: u32::from_le_bytes(raw[12..16].try_into().unwrap()),
                // raw[16..18] is num_capture_types
                errors: u16::from_le_bytes(raw[18..20].try_into().unwrap()),
                template_size: u32::from_le_bytes(raw[20..24].try_into().unwrap()),
                template_max: u16::from_le_bytes(raw[24..26].try_into().unwrap()),
                template_valid: u16::from_le_bytes(raw[26..28].try_into().unwrap()),
                template_dirty: u32::from_le_bytes(raw[28..32].try_into().unwrap()),
                template_version: u32::from_le_bytes(raw[32..36].try_into().unwrap()),
                ..Default::default()
            };
            if raw.len() >= 52 {
                info.frame_size = u32::from_le_bytes(raw[36..40].try_into().unwrap());
                info.pixel_format = u32::from_le_bytes(raw[40..44].try_into().unwrap());
                info.width = u16::from_le_bytes(raw[44..46].try_into().unwrap());
                info.height = u16::from_le_bytes(raw[46..48].try_into().unwrap());
                info.bpp = u16::from_le_bytes(raw[48..50].try_into().unwrap());
            }
            return Ok(info);
        }
    }

    // Fallback: Version 1
    let info: EcResponseFpInfo = ec_command_bytemuck(CrosEcCmd::FpInfo, 1, &(), fd)?;
    Ok(info)
}
