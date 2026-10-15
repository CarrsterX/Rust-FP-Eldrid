#![warn(unused_crate_dependencies)]

extern crate pam;
extern crate rand;

use std::ffi::CStr;
use std::sync::mpsc::channel;
use std::thread;

use pam::constants::PamResultCode::{PAM_ABORT, PAM_AUTH_ERR, PAM_SUCCESS};
use pam::constants::{PamFlag, PamResultCode, PAM_ERROR_MSG, PAM_TEXT_INFO};
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

/// `pam_get_user`, bound correctly.
///
/// pam-bindings 0.1.1 declares the out-parameter as `&*mut c_char` and hands
/// it a *shared* reference to an immutable local, then reads that local back
/// after the call. That is undefined behaviour: with optimisations enabled the
/// compiler is entitled to assume the pointer never changed, so the null check
/// always fires and the function returns `Err(PAM_SUCCESS)` even on success.
///
/// Callers who forward that code as their own result - the obvious way to
/// write the error path - end up returning PAM_SUCCESS, authenticating with no
/// check whatsoever. Bind it ourselves rather than rely on the fix landing
/// upstream.
fn pam_user(pamh: &PamHandle) -> Option<String> {
    use std::ffi::CStr;
    extern "C" {
        fn pam_get_user(
            pamh: *const libc::c_void,
            user: *mut *const libc::c_char,
            prompt: *const libc::c_char,
        ) -> libc::c_int;
    }
    let mut ptr: *const libc::c_char = std::ptr::null();
    let rc = unsafe {
        pam_get_user(
            pamh as *const PamHandle as *const libc::c_void,
            &mut ptr,
            std::ptr::null(),
        )
    };
    if rc != 0 || ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
}

fn pam_service(pamh: &PamHandle) -> Option<String> {
    const PAM_SERVICE: libc::c_int = 1;
    use std::ffi::CStr;
    extern "C" {
        fn pam_get_item(
            pamh: *const libc::c_void,
            item_type: libc::c_int,
            item: *mut *const libc::c_char,
        ) -> libc::c_int;
    }
    let mut ptr: *const libc::c_char = std::ptr::null();
    let rc = unsafe {
        pam_get_item(
            pamh as *const PamHandle as *const libc::c_void,
            PAM_SERVICE,
            &mut ptr,
        )
    };
    if rc != 0 || ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
}

/// Home directory of `user` according to the passwd database.
///
/// `home::home_dir()` reports the home of the *running process*, which for a
/// system authentication helper is root's. Resolving the PAM user explicitly
/// is what makes "authenticate user X" mean "match X's own templates".
fn home_of(user: &str) -> Option<String> {
    use std::ffi::{CStr, CString};
    let c_user = CString::new(user).ok()?;
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe {
        libc::getpwnam_r(
            c_user.as_ptr(),
            &mut pwd,
            buf.as_mut_ptr(),
            buf.len(),
            &mut result,
        )
    };
    if rc != 0 || result.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pwd.pw_dir) }.to_str().ok().map(str::to_owned)
}

struct RustFpPam;
pam::pam_hooks!(RustFpPam);

impl PamHooks for RustFpPam {
    // This function performs the task of authenticating the user.
    fn sm_authenticate(pamh: &mut PamHandle, _args: Vec<&CStr>, _flags: PamFlag) -> PamResultCode {
        enum Message {
            Error(String),
            Result(PamResultCode),
        }

        // Point $HOME at the user being authenticated before any thread is
        // spawned (set_var is not thread-safe). get_templates()/set_templates()
        // derive their path from home_dir(), which consults $HOME first.
        let username = match pam_user(pamh) {
            Some(u) => u,
            None => {
                eprintln!("rust-fp: could not determine the PAM user");
                return PAM_AUTH_ERR;
            }
        };
        match home_of(&username) {
            Some(dir) => std::env::set_var("HOME", dir),
            None => {
                eprintln!("rust-fp: no home directory for user {username}");
                return PAM_AUTH_ERR;
            }
        }

        let (tx, rx) = channel();
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
                            // Each attempt blocks on the sensor, and nothing is shown on screen while
                            // it does. Retrying in here just delays the password prompt the
                            // user is waiting for; let the PAM stack move on instead.
                            let max_attempts = 3;
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
                                        if remaining_attempts == 0 {
                                            let _ = tx.send(Message::Error("No match.".into()));
                                            if let Some(error) = error {
                                                let _ = tx.send(Message::Error(format!(
                                                    "Error matching: {error:?}"
                                                )));
                                            }
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
        let is_gdm = pam_service(pamh).as_deref() == Some("gdm-fingerprint");
        // Tell the user what is being waited for in interactive terminal sessions.
        // Under GDM, GNOME Shell already shows the biometric indicator, and sending
        // PAM_TEXT_INFO triggers GNOME Shell 2-second message hold timer.
        if !is_gdm {
            if let Err(e) = conv.send(PAM_TEXT_INFO, "Touch the fingerprint sensor") {
                eprintln!("rust-fp: could not show the fingerprint prompt: {e:?}");
            }
        }

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

    fn sm_setcred(_pamh: &mut PamHandle, _args: Vec<&CStr>, _flags: PamFlag) -> PamResultCode {
        PAM_SUCCESS
    }
}
