#![warn(unused_crate_dependencies)]

extern crate pam;
extern crate rand;

use std::ffi::CStr;
use std::sync::mpsc::channel;
use std::thread;

use pam::constants::PamResultCode::{PAM_ABORT, PAM_AUTH_ERR, PAM_SUCCESS};
use pam::constants::{PamFlag, PamResultCode, PAM_ERROR_MSG};
use pam::conv::Conv;
use pam::module::{PamHandle, PamHooks};
use pollster::block_on;
use postcard::from_bytes;
use zbus::blocking::Connection;

use rust_fp::fingerprint_driver::{MatchOutput, MatchedOutput};
use rust_fp_common::get_templates::get_templates;
use rust_fp_common::rust_fp_dbus::RustFpProxyBlocking;
use rust_fp_common::set_templates::set_templates;

use crate::wait_until_unlock::wait_until_unlock;

mod wait_until_unlock;

struct RustFpPam;
pam::pam_hooks!(RustFpPam);

impl PamHooks for RustFpPam {
    // This function performs the task of authenticating the user.
    fn sm_authenticate(pamh: &mut PamHandle, _args: Vec<&CStr>, _flags: PamFlag) -> PamResultCode {
        enum Message {
            Error(String),
            Result(PamResultCode),
        }

        let (tx, rx) = channel();
        // Exit on Ctrl+C
        // Sometimes can fail with the `MultipleHandlers` error
        let _ = ctrlc::set_handler({
            let tx = tx.clone();
            move || {
                // Useful for debugging
                // Command::new("play").arg("https://www.myinstants.com/media/sounds/sudden-suspense-sound-effect.mp3").output().unwrap();
                let _ = tx.send(Message::Result(PAM_ABORT));
            }
        });
        // Exit if the screen was unlocked by typing the password
        thread::spawn({
            let tx = tx.clone();
            move || {
                if wait_until_unlock().is_ok() {
                    // Useful for debugging
                    // Command::new("play").arg("https://www.myinstants.com/media/sounds/sudden-suspense-sound-effect.mp3").output().unwrap();
                    let _ = tx.send(Message::Result(PAM_ABORT));
                }
            }
        });
        // Actual fingerprint matching
        thread::spawn({
            let tx = tx.clone();
            move || {
                let authenticate = {
                    let tx = tx.clone();
                    move || -> PamResultCode {
                        let mut templates = match block_on(get_templates()) {
                            Ok(t) => t,
                            Err(e) => {
                                eprintln!("rust-fp: cannot read templates: {e}");
                                return PAM_AUTH_ERR;
                            }
                        };
                        if !templates.is_empty() {
                            let connection = match Connection::system() {
                                Ok(c) => c,
                                Err(e) => {
                                    eprintln!("rust-fp: no system bus: {e}");
                                    return PAM_AUTH_ERR;
                                }
                            };
                            let proxy = match RustFpProxyBlocking::new(&connection) {
                                Ok(p) => p,
                                Err(e) => {
                                    eprintln!("rust-fp: rust-fp service unavailable: {e}");
                                    return PAM_AUTH_ERR;
                                }
                            };
                            let templates_vec = templates.iter().collect::<Vec<_>>();
                            let max_attempts = 5;
                            for attempt in 0..max_attempts {
                                let raw = match proxy.match_templates(
                                    templates_vec
                                        .iter()
                                        .map::<Vec<u8>, _>(|(_k, v)| v.to_vec())
                                        .collect(),
                                ) {
                                    Ok(r) => r,
                                    Err(e) => {
                                        // The daemon can die or be restarted while a
                                        // match is outstanding.
                                        eprintln!("rust-fp: match call failed: {e}");
                                        return PAM_AUTH_ERR;
                                    }
                                };
                                let output: MatchOutput = match from_bytes(&raw) {
                                    Ok(o) => o,
                                    Err(e) => {
                                        eprintln!("rust-fp: undecodable match output: {e:?}");
                                        return PAM_AUTH_ERR;
                                    }
                                };
                                match output {
                                    MatchOutput::Match(MatchedOutput {
                                        index,
                                        updated_template,
                                    }) => {
                                        let matched_label = templates_vec[index].0;
                                        eprintln!("Matched: {matched_label}.");
                                        if let Some(updated_template) = updated_template {
                                            eprintln!(
                                                "Template was updated. Saving updated template..."
                                            );
                                            templates
                                                .insert(matched_label.to_owned(), updated_template);
                                            // Authentication already succeeded; failing to persist the
                                            // adapted template must not revoke it.
                                            if let Err(e) = block_on(set_templates(&templates)) {
                                                eprintln!("rust-fp: could not persist template: {e}");
                                            }
                                            eprintln!("Saved updated template");
                                        }
                                        return PAM_SUCCESS;
                                    }
                                    MatchOutput::NoMatch(error) => {
                                        let remaining_attempts = max_attempts - attempt - 1;
                                        let _ = tx.send(Message::Error(format!(
                                            "No match. {remaining_attempts} attempts remaining."
                                        )));
                                        if let Some(error) = error {
                                            let _ = tx.send(Message::Error(format!(
                                                "Error matching: {error:?}"
                                            )));
                                        }
                                    }
                                }
                            }
                            PAM_AUTH_ERR
                        } else {
                            let _ = tx.send(Message::Error("No templates saved. Not matching.".into()));
                            PAM_AUTH_ERR
                        }
                    }
                };
                let _ = tx.send(Message::Result(authenticate()));
            }
        });
        let conv = match pamh.get_item::<Conv>() {
            Ok(Some(conv)) => conv,
            Ok(None) => todo!(),
            Err(err) => {
                eprintln!("Couldn't get pam_conv");
                return err;
            }
        };
        for message in rx {
            match message {
                Message::Error(message) => {
                    // A conversation that cannot render our message must not fail the
                    // whole authentication: the stack still has to fall through
                    // to the password module.
                    let _ = conv.send(PAM_ERROR_MSG, &message);
                }
                Message::Result(result) => {
                    return result;
                }
            }
        }
        PAM_AUTH_ERR
    }
}
