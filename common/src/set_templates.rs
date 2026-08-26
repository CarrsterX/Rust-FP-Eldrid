use std::fmt::{Display, Formatter};
use std::io;

use rmp_serde::encode;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

use crate::fp_file;
use crate::fp_file::{get_fp_dir, get_fp_file};
use crate::get_templates::checked_fp_dir;
use crate::template::Templates;

#[derive(Debug)]
pub enum Error {
    Encode(encode::Error),
    FpDir(fp_file::Error),
    CreateDir(io::Error),
    FpFile(fp_file::Error),
    Open(io::Error),
    Write(io::Error),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(e) => {
                write!(f, "Error encoding file: {:#?}", e)
            }
            Self::FpDir(e) => {
                write!(f, "Error getting fp file: {:#?}", e)
            }
            Self::CreateDir(e) => {
                write!(f, "Error creating dir: {:#?}", e)
            }
            Self::FpFile(e) => {
                write!(f, "Error getting fp file: {:#?}", e)
            }
            Self::Open(e) => {
                write!(f, "Error opening file: {:#?}", e)
            }
            Self::Write(e) => {
                write!(f, "Error reading file: {:#?}", e)
            }
        }
    }
}

pub async fn set_templates(templates: &Templates) -> Result<(), Error> {
    let vec = encode::to_vec(templates).map_err(Error::Encode)?;
    std::fs::create_dir_all(get_fp_dir().map_err(Error::FpDir)?).map_err(Error::CreateDir)?;
    // Re-check after creating it: root is about to write here.
    checked_fp_dir().map_err(Error::CreateDir)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        // Without this a template that encodes smaller than its predecessor
        // leaves stale trailing bytes behind.
        .truncate(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(get_fp_file().map_err(Error::FpFile)?)
        .map_err(Error::Open)?;
    // write() may be partial; the remainder was silently dropped.
    file.write_all(&vec).map_err(Error::Write)?;
    Ok(())
}
