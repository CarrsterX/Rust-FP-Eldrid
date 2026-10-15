use std::{mem::offset_of, os::fd::AsRawFd};

use bytemuck::{bytes_of, Pod, Zeroable};

use crate::{ec_command::ec_command_with_dynamic_output_size, EcCmdResult};

use super::{
    fp_download::FpTemplate, fp_info::EcResponseFpInfo,
    get_protocol_info::EcResponseGetProtocolInfo, CrosEcCmd,
};

#[derive(Pod, Zeroable, Clone, Copy)]
#[repr(C)]
struct EcParamsFpTemplateWithoutData {
    offset: u32,
    size: u32,
    data: [u8; 0],
}

/// Flag in the 'size' field indicating that the full template has been sent
const FP_TEMPLATE_COMMIT: u32 = 0x80000000;

pub fn fp_upload_template<File: AsRawFd>(
    file: &mut File,
    protocol_info: &EcResponseGetProtocolInfo,
    fp_info: &EcResponseFpInfo,
    template: &FpTemplate,
) -> EcCmdResult<()> {
    assert_eq!(
        template.buffer().len(),
        fp_info.template_size as usize,
        "The given template must match the fp sensor's template size"
    );
    let max_chunk_size =
        protocol_info.max_ec_input_size() - offset_of!(EcParamsFpTemplateWithoutData, data) - 4;
    let total_len = template.buffer().len();
    let mut offset = 0;
    while offset < total_len {
        let remaining = total_len - offset;
        let transfer_len = remaining.min(max_chunk_size);
        let is_commit = (remaining == transfer_len);
        let mut size = transfer_len as u32;
        if is_commit {
            size |= FP_TEMPLATE_COMMIT;
        }
        let mut vec = bytes_of(&EcParamsFpTemplateWithoutData {
            offset: offset as u32,
            size,
            data: [],
        })
        .to_vec();
        vec.extend_from_slice(&template.buffer()[offset..offset + transfer_len]);
        ec_command_with_dynamic_output_size(
            CrosEcCmd::FpTemplate,
            0,
            &vec,
            0,
            file.as_raw_fd(),
        )?;
        offset += transfer_len;
    }
    Ok(())
}
