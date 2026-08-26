use std::fmt::{Display, Formatter};
use std::io;
use std::io::ErrorKind;

use crate::fp_file;
use rmp_serde::decode;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

use crate::fp_file::{get_fp_dir, get_fp_file};
use crate::template::Templates;

#[derive(Debug)]
pub enum Error {
    Open(io::Error),
    FpFile(fp_file::Error),
    Read(io::Error),
    Decode(decode::Error),
}

impl std::error::Error for Error {}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open(e) => {
                write!(f, "Error opening file: {:#?}", e)
            }
            Self::FpFile(e) => {
                write!(f, "Error getting fp file: {:#?}", e)
            }
            Self::Read(e) => {
                write!(f, "Error reading file: {:#?}", e)
            }
            Self::Decode(e) => {
                write!(f, "Error decoding file: {:#?}", e)
            }
        }
    }
}

/// The template directory, rejected if it is a symlink.
///
/// The file lives in a user-writable directory but is opened by the PAM module
/// running as root. O_NOFOLLOW only guards the final path component, so the
/// directory has to be checked separately; otherwise replacing `~/.var` with a
/// symlink is enough to have root read - or overwrite - an arbitrary file.
pub(crate) fn checked_fp_dir() -> Result<String, io::Error> {
    let dir = get_fp_dir().map_err(|e| io::Error::new(ErrorKind::Other, format!("{e}")))?;
    match std::fs::symlink_metadata(&dir) {
        Ok(m) if m.is_dir() => Ok(dir),
        Ok(_) => Err(io::Error::new(
            ErrorKind::InvalidData,
            "fp dir is a symlink or not a directory",
        )),
        Err(e) => Err(e),
    }
}

pub async fn get_templates() -> Result<Templates, Error> {
    match checked_fp_dir() {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Default::default()),
        Err(e) => return Err(Error::Open(e)),
    }
    match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(get_fp_file().map_err(Error::FpFile)?)
    {
        Ok(mut file) => {
            let meta = file.metadata().map_err(Error::Read)?;
            if !meta.is_file() || meta.nlink() != 1 {
                return Err(Error::Open(io::Error::new(
                    ErrorKind::InvalidData,
                    "template file is not a plain unshared file",
                )));
            }
            let mut buf = Vec::new();
            file.read_to_end(&mut buf).map_err(Error::Read)?;
            rmp_serde::from_slice::<Templates>(&buf).map_err(Error::Decode)
        }
        Err(e) => match e.kind() {
            ErrorKind::NotFound => Ok(Default::default()),
            _ => Err(Error::Open(e)),
        },
    }
}
