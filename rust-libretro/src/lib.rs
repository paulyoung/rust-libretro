#![allow(clippy::missing_safety_doc)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/max-m/rust-libretro/master/media/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/max-m/rust-libretro/master/media/favicon.png"
)]

mod core_wrapper;
#[cfg(feature = "log")]
mod logger;

pub mod contexts;
pub mod core;
pub mod environment;
pub mod types;
pub mod util;

pub use rust_libretro_proc as proc;
pub use rust_libretro_sys as sys;

use crate::{contexts::*, core::Core, core_wrapper::CoreWrapper, sys::*, types::*, util::*};
use std::{
    ffi::*,
    os::raw::c_char,
    path::{Path, PathBuf},
};

#[doc(hidden)]
static mut RETRO_INSTANCE: Option<CoreWrapper> = None;

/// This macro must be used to initialize your [`Core`].
///
/// # Examples
/// ```rust
/// # use rust_libretro::{contexts::*, core::{Core, CoreOptions}, sys::*, types::*, retro_core};
/// # use std::ffi::CString;
/// struct ExampleCore {
///     option_1: bool,
///     option_2: bool,
///
///     pixels: [u8; 800 * 600 * 4],
///     timer: i64,
///     even: bool,
/// }
/// retro_core!(ExampleCore {
///     option_1: false,
///     option_2: true,
///
///     pixels: [0; 800 * 600 * 4],
///     timer: 5_000_001,
///     even: true,
/// });
///
/// /// Dummy implementation
/// impl CoreOptions for ExampleCore {}
/// impl Core for ExampleCore {
///     fn get_info(&self) -> SystemInfo {
///         SystemInfo {
///             library_name: CString::new("ExampleCore").unwrap(),
///             library_version: CString::new("1.0.0").unwrap(),
///             valid_extensions: CString::new("").unwrap(),
///             need_fullpath: false,
///             block_extract: false,
///         }
///     }
///     fn on_get_av_info(&mut self, _ctx: &mut GetAvInfoContext) -> retro_system_av_info {
///         retro_system_av_info {
///             geometry: retro_game_geometry {
///                 base_width: 800,
///                 base_height: 600,
///                 max_width: 800,
///                 max_height: 600,
///                 aspect_ratio: 0.0,
///             },
///             timing: retro_system_timing {
///                 fps: 60.0,
///                 sample_rate: 0.0,
///             },
///         }
///     }
///     fn on_init(&mut self, ctx: &mut InitContext) { }
/// }
/// ```
#[macro_export]
macro_rules! retro_core {
    ( $( $definition:tt )+ ) => {
        #[doc(hidden)]
        #[inline(never)]
        #[no_mangle]
        pub unsafe extern "Rust" fn __retro_init_core() {
            $crate::set_core($($definition)+);
        }
    }
}

#[doc(hidden)]
macro_rules! forward {
    ($(#[doc = $doc:tt ], )* $wrapper:ident, $name:ident, $handler:ident $(-> $return_type:ty)?, $($context:tt)+) => {
        #[no_mangle]
        $(#[doc = $doc])*
        pub unsafe extern "C" fn $name() $(-> $return_type)? {
            // Check that the instance has been created
            if let Some($wrapper) = RETRO_INSTANCE.as_mut() {
                // Forward to the Core implementation
                let mut ctx = $($context)+;
                return $wrapper.core.$handler(&mut ctx);
            }

            panic!(concat!(stringify!($name), ": Core has not been initialized yet!"));
        }
    };
}

#[doc(hidden)]
macro_rules! callback {
    ($(#[doc = $doc:tt ], )* $name:ident, $arg:ident, $handler:ident) => {
        #[no_mangle]
        $(#[doc = $doc])*
        pub unsafe extern "C" fn $name(arg1: $arg) {
            // Check that the instance has been created
            if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
                if arg1.is_some() {
                    // We were given a callback, make sure that it’s not a NULL pointer
                    if (arg1.unwrap() as *const c_void).is_null() {
                        panic!(concat!(
                            "Expected ",
                            stringify!($arg),
                            " got NULL pointer instead!"
                        ));
                    }
                }

                // The callback is safe to set. Either it’s None or not a NULL pointer
                return wrapper.$handler(arg1);
            }

            panic!(concat!(
                stringify!($name),
                ": Core has not been initialized yet!"
            ));
        }
    };
}

#[doc(hidden)]
pub fn set_core<C: 'static + Core>(core: C) {
    unsafe {
        if RETRO_INSTANCE.is_some() {
            let core = &RETRO_INSTANCE.as_ref().unwrap().core;
            let info = core.get_info();
            let name = info.library_name.into_string().unwrap();
            let version = info.library_version.into_string().unwrap();

            panic!("Attempted to set a core after the system was already initialized.\nAlready registered core: {} {}", name, version)
        }

        RETRO_INSTANCE.replace(CoreWrapper::new(core));
    }
}

#[cfg(feature = "log")]
#[doc(hidden)]
fn init_log(env_callback: retro_environment_t) {
    let retro_logger = unsafe { environment::get_log_callback(env_callback) };

    let retro_logger = if let Ok(Some(log_callback)) = retro_logger {
        logger::RetroLogger::new(log_callback)
    } else {
        logger::RetroLogger::new(retro_log_callback { log: None })
    };

    log::set_max_level(log::LevelFilter::Trace);
    log::set_boxed_logger(Box::new(retro_logger)).expect("could not set logger");
}

/*****************************************************************************\
|                              CORE API FUNCTIONS                             |
\*****************************************************************************/

forward!(
    #[doc = "Notifies the [`Core`] when all cheats should be unapplied."],
    wrapper,
    retro_cheat_reset,
    on_cheat_reset,
    GenericContext::new(&wrapper.environment_callback)
);
forward!(
    #[doc = "Notifies the [`Core`] when it is being closed and its resources should be freed."],
    wrapper,
    retro_deinit,
    on_deinit,
    GenericContext::new(&wrapper.environment_callback)
);
forward!(
    #[doc = "Called when the frontend needs region information from the [`Core`]."],
    #[doc = ""],
    #[doc = "## Note about RetroArch:"],
    #[doc = "RetroArch doesn’t use this interface anymore, because [`retro_get_system_av_info`] provides similar information."],
    wrapper,
    retro_get_region,
    on_get_region -> std::os::raw::c_uint,
    GenericContext::new(&wrapper.environment_callback)
);
forward!(
    #[doc = "Notifies the [`Core`] when the current game should be reset."],
    wrapper,
    retro_reset,
    on_reset,
    GenericContext::new(&wrapper.environment_callback)
);
forward!(
    #[doc = "Called when the frontend needs to know how large a buffer to allocate for save states."],
    #[doc = ""],
    #[doc = "See also [`rust_libretro_sys::retro_serialize_size`]."],
    wrapper,
    retro_serialize_size,
    get_serialize_size -> size_t,
    GenericContext::new(&wrapper.environment_callback)
);
forward!(
    #[doc = "Notifies the [`Core`] when the currently loaded game should be unloaded. Called before [`retro_deinit`]."],
    wrapper,
    retro_unload_game,
    on_unload_game,
    GenericContext::new(&wrapper.environment_callback)
);

callback!(
    #[doc = "Provides the audio sample callback to the [`Core`]."],
    #[doc = ""],
    #[doc = "Guaranteed to have been called before the first call to [`retro_run`] is made."],
    retro_set_audio_sample,
    retro_audio_sample_t,
    on_set_audio_sample
);
callback!(
    #[doc = "Provides the batched audio sample callback to the [`Core`]."],
    #[doc = ""],
    #[doc = "Guaranteed to have been called before the first call to [`retro_run`] is made."],
    retro_set_audio_sample_batch,
    retro_audio_sample_batch_t,
    on_set_audio_sample_batch
);
callback!(
    #[doc = "Provides the input polling callback to the [`Core`]."],
    #[doc = ""],
    #[doc = "Guaranteed to have been called before the first call to [`retro_run`] is made."],
    retro_set_input_poll,
    retro_input_poll_t,
    on_set_input_poll
);
callback!(
    #[doc = "Provides the input state request callback to the [`Core`]."],
    #[doc = ""],
    #[doc = "Guaranteed to have been called before the first call to [`retro_run`] is made."],
    retro_set_input_state,
    retro_input_state_t,
    on_set_input_state
);
callback!(
    #[doc = "Provides the frame drawing callback to the [`Core`]."],
    #[doc = ""],
    #[doc = "Guaranteed to have been called before the first call to [`retro_run`] is made."],
    retro_set_video_refresh,
    retro_video_refresh_t,
    on_set_video_refresh
);

/// Tells the frontend which API version this [`Core`] implements.
#[no_mangle]
pub unsafe extern "C" fn retro_api_version() -> std::os::raw::c_uint {
    RETRO_API_VERSION
}

/// Initializes the [`Core`].
///
/// Called after the environment callbacks have been set.
#[no_mangle]
pub unsafe extern "C" fn retro_init() {
    if let Some(mut wrapper) = RETRO_INSTANCE.as_mut() {
        wrapper.can_dupe = environment::can_dupe(wrapper.environment_callback);

        let mut ctx = InitContext::new(&wrapper.environment_callback);

        wrapper.core.on_init(&mut ctx)
    } else {
        panic!("retro_init: Core has not been initialized yet!");
    }
}

/// Provides _statically known_ system info to the frontend.
///
/// See also [`rust_libretro_sys::retro_get_system_info`].
#[no_mangle]
pub unsafe extern "C" fn retro_get_system_info(info: *mut retro_system_info) {
    // Make sure that the pointer we got is plausible
    if info.is_null() {
        panic!("Expected retro_system_info, got NULL pointer instead!");
    }

    // We didn’t get a NULL pointer, so this should be safe
    let info = &mut *info;

    // retro_get_system_info requires statically allocated data
    // This is unsafe because we mutate a static value.
    //
    // TODO: Should this be put behind an Arc<Mutex> or Arc<RwLock>?
    static mut SYS_INFO: Option<*const SystemInfo> = None;

    let sys_info = {
        if SYS_INFO.is_none() {
            extern "Rust" {
                fn __retro_init_core();
            }
            __retro_init_core();

            if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
                SYS_INFO = Some(Box::into_raw(Box::new(wrapper.core.get_info())));
            } else {
                panic!("No core instance found!");
            }
        }

        &*SYS_INFO.unwrap()
    };

    info.library_name = sys_info.library_name.as_ptr();
    info.library_version = sys_info.library_version.as_ptr();
    info.valid_extensions = sys_info.valid_extensions.as_ptr();
    info.need_fullpath = sys_info.need_fullpath;
    info.block_extract = sys_info.block_extract;
}

/// Provides audio/video timings and geometry info to the frontend.
///
/// Guaranteed to be called only after successful invocation of [`retro_load_game`].
///
/// See also [`rust_libretro_sys::retro_get_system_av_info`].
#[no_mangle]
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut retro_system_av_info) {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        // Make sure that the pointer we got is plausible
        if info.is_null() {
            panic!("Expected retro_system_av_info, got NULL pointer instead!");
        }

        // We didn’t get a NULL pointer, so this should be safe
        let info = &mut *info;

        let mut ctx = GetAvInfoContext::new(&wrapper.environment_callback);

        let av_info = wrapper.core.on_get_av_info(&mut ctx);

        info.geometry = av_info.geometry;
        info.timing = av_info.timing;
    } else {
        panic!("retro_get_system_av_info: Core has not been initialized yet!");
    }
}

/// Provides the environment callback to the [`Core`].
///
/// Guaranteed to have been called before [`retro_init`].
///
/// **TODO:** This method seems to get called multiple times by RetroArch
#[no_mangle]
pub unsafe extern "C" fn retro_set_environment(environment: retro_environment_t) {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut initial = false;

        if let Some(callback) = environment {
            if !wrapper.environment_set {
                initial = true;
                wrapper.environment_set = true;

                #[cfg(feature = "log")]
                init_log(Some(callback));

                #[cfg(feature = "unstable-env-commands")]
                {
                    wrapper.supports_bitmasks = environment::get_input_bitmasks(Some(callback));
                }
            }

            wrapper.environment_callback.replace(callback);
        } else {
            wrapper.environment_callback.take();
        }

        let mut ctx = SetEnvironmentContext::new(&wrapper.environment_callback);

        // Our default implementation of `set_core_options` uses `RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION`,
        // which seems to only work on the first call to `retro_set_environment`.
        if initial && !wrapper.core.set_core_options(&ctx) {
            #[cfg(feature = "log")]
            log::warn!("Failed to set core options");
        }

        wrapper.core.on_set_environment(initial, &mut ctx);
    }
}

/// Sets the device type to be used for player `port`.
///
/// See also [`rust_libretro_sys::retro_set_controller_port_device`].
#[no_mangle]
pub unsafe extern "C" fn retro_set_controller_port_device(
    port: std::os::raw::c_uint,
    device: std::os::raw::c_uint,
) {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        return wrapper.core.on_set_controller_port_device(port, device);
    }

    panic!("retro_set_controller_port_device: Core has not been initialized yet!");
}

/// Runs the game for one frame.
///
/// See also [`rust_libretro_sys::retro_run`].
#[no_mangle]
pub unsafe extern "C" fn retro_run() {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        if environment::get_variable_update(wrapper.environment_callback) {
            let mut ctx = OptionsChangedContext::new(&wrapper.environment_callback);

            wrapper.core.on_options_changed(&mut ctx);
        }

        if let Some(callback) = wrapper.input_poll_callback {
            (callback)();
        }

        let mut ctx = RunContext {
            environment_callback: &wrapper.environment_callback,

            video_refresh_callback: &wrapper.video_refresh_callback,
            audio_sample_callback: &wrapper.audio_sample_callback,
            audio_sample_batch_callback: &wrapper.audio_sample_batch_callback,
            input_poll_callback: &wrapper.input_poll_callback,
            input_state_callback: &wrapper.input_state_callback,

            can_dupe: wrapper.can_dupe,
            had_frame: &mut wrapper.had_frame,
            last_width: &mut wrapper.last_width,
            last_height: &mut wrapper.last_height,
            last_pitch: &mut wrapper.last_pitch,

            supports_bitmasks: wrapper.supports_bitmasks,
        };

        return wrapper.core.on_run(&mut ctx, wrapper.frame_delta.take());
    }

    panic!("retro_run: Core has not been initialized yet!");
}

/// Called by the frontend when the [`Core`]s state should be serialized (“save state”).
/// This function should return [`false`] on error.
///
/// This could also be used by a frontend to implement rewind.
#[no_mangle]
pub unsafe extern "C" fn retro_serialize(data: *mut std::os::raw::c_void, size: size_t) -> bool {
    if data.is_null() {
        #[cfg(feature = "log")]
        log::warn!("retro_serialize: data is null");

        return false;
    }

    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = GenericContext::new(&wrapper.environment_callback);

        // Convert the given buffer into a proper slice
        let slice = std::slice::from_raw_parts_mut(data as *mut u8, size as usize);

        return wrapper.core.on_serialize(slice, &mut ctx);
    }

    panic!("retro_serialize: Core has not been initialized yet!");
}

/// Called by the frontend when a “save state” should be loaded.
/// This function should return [`false`] on error.
///
/// This could also be used by a frontend to implement rewind.
#[no_mangle]
pub unsafe extern "C" fn retro_unserialize(
    data: *const std::os::raw::c_void,
    size: size_t,
) -> bool {
    if data.is_null() {
        #[cfg(feature = "log")]
        log::warn!("retro_unserialize: data is null");

        return false;
    }

    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = GenericContext::new(&wrapper.environment_callback);

        // Convert the given buffer into a proper slice
        let slice = std::slice::from_raw_parts_mut(data as *mut u8, size as usize);

        return wrapper.core.on_unserialize(slice, &mut ctx);
    }

    panic!("retro_unserialize: Core has not been initialized yet!");
}

/// Called by the frontend whenever a cheat should be applied.
///
/// The format is core-specific but this function lacks a return value,
/// so a [`Core`] can’t tell the frontend if it failed to parse a code.
#[no_mangle]
pub unsafe extern "C" fn retro_cheat_set(
    index: std::os::raw::c_uint,
    enabled: bool,
    code: *const std::os::raw::c_char,
) {
    if code.is_null() {
        #[cfg(feature = "log")]
        log::warn!("retro_cheat_set: code is null");

        return;
    }

    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = GenericContext::new(&wrapper.environment_callback);

        // Wrap the pointer into a `CStr`.
        // This assumes the pointer is valid and ends on a null byte.
        //
        // For now we’ll let the core handle conversion to Rust `str` or `String`,
        // as the lack of documentation doesn’t make it clear if the returned string
        // is encoded as valid UTF-8.
        let code = CStr::from_ptr(code);

        return wrapper.core.on_cheat_set(index, enabled, code, &mut ctx);
    }

    panic!("retro_cheat_set: Core has not been initialized yet!");
}

/// Called by the frontend when a game should be loaded.
///
/// A return value of [`true`] indicates success.
#[no_mangle]
pub unsafe extern "C" fn retro_load_game(game: *const retro_game_info) -> bool {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = OptionsChangedContext::new(&wrapper.environment_callback);

        wrapper.core.on_options_changed(&mut ctx);

        let mut ctx = LoadGameContext::new(
            &wrapper.environment_callback,
            &mut wrapper.camera_interface,
            &mut wrapper.perf_interface,
            &mut wrapper.location_interface,
            &mut wrapper.rumble_interface,
            #[cfg(feature = "unstable-env-commands")]
            &mut wrapper.sensor_interface,
        );

        let status = if game.is_null() {
            wrapper.core.on_load_game(None, &mut ctx)
        } else {
            wrapper.core.on_load_game(Some(*game), &mut ctx)
        };

        return status;
    }

    panic!("retro_load_game: Core has not been initialized yet!");
}

/// See [`rust_libretro_sys::retro_load_game_special`].
#[no_mangle]
pub unsafe extern "C" fn retro_load_game_special(
    game_type: std::os::raw::c_uint,
    info: *const retro_game_info,
    num_info: size_t,
) -> bool {
    if info.is_null() {
        #[cfg(feature = "log")]
        log::warn!("retro_load_game_special: info is null");

        return false;
    }

    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = OptionsChangedContext::new(&wrapper.environment_callback);

        wrapper.core.on_options_changed(&mut ctx);

        let mut ctx = LoadGameSpecialContext::new(&wrapper.environment_callback);

        let status = wrapper
            .core
            .on_load_game_special(game_type, info, num_info, &mut ctx);

        return status;
    }

    panic!("retro_load_game_special: Core has not been initialized yet!");
}

/// Returns a mutable pointer to queried memory type.
/// Return [`std::ptr::null()`] in case this doesn’t apply to your [`Core`].
///
/// `id` is one of the `RETRO_MEMORY_*` constants.
#[no_mangle]
pub unsafe extern "C" fn retro_get_memory_data(
    id: std::os::raw::c_uint,
) -> *mut std::os::raw::c_void {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = GenericContext::new(&wrapper.environment_callback);

        return wrapper.core.get_memory_data(id, &mut ctx);
    }

    panic!("retro_get_memory_data: Core has not been initialized yet!");
}

/// Returns the size (in bytes) of the queried memory type.
/// Return `0` in case this doesn’t apply to your [`Core`].
///
/// `id` is one of the `RETRO_MEMORY_*` constants.
#[no_mangle]
pub unsafe extern "C" fn retro_get_memory_size(id: std::os::raw::c_uint) -> size_t {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = GenericContext::new(&wrapper.environment_callback);

        return wrapper.core.get_memory_size(id, &mut ctx);
    }

    panic!("retro_get_memory_size: Core has not been initialized yet!");
}

/*****************************************************************************\
|                            NON CORE API FUNCTIONS                           |
\*****************************************************************************/

#[no_mangle]
pub unsafe extern "C" fn retro_keyboard_callback_fn(
    down: bool,
    keycode: ::std::os::raw::c_uint,
    character: u32,
    key_modifiers: u16,
) {
    cfg_if::cfg_if! {
        if #[cfg(target_family = "windows")] {
            let keycode = keycode as i32;
        }
    };

    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        wrapper.core.on_keyboard_event(
            down,
            retro_key(keycode),
            character,
            retro_mod(key_modifiers.into()),
        )
    }
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_hw_context_reset_callback() {
    println!("TODO: retro_hw_context_reset_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_hw_context_destroyed_callback() {
    println!("TODO: retro_hw_context_destroyed_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_set_eject_state_callback(ejected: bool) -> bool {
    dbg!(ejected);
    println!("TODO: retro_set_eject_state_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_get_eject_state_callback() -> bool {
    println!("TODO: retro_get_eject_state_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_get_image_index_callback() -> ::std::os::raw::c_uint {
    println!("TODO: retro_get_image_index_callback");
    0
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_set_image_index_callback(index: ::std::os::raw::c_uint) -> bool {
    dbg!(index);
    println!("TODO: retro_set_image_index_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_get_num_images_callback() -> ::std::os::raw::c_uint {
    println!("TODO: retro_get_num_images_callback");
    0
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_replace_image_index_callback(
    index: ::std::os::raw::c_uint,
    info: *const retro_game_info,
) -> bool {
    dbg!(index);
    dbg!(info);
    println!("TODO: retro_replace_image_index_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_add_image_index_callback() -> bool {
    println!("TODO: retro_add_image_index_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_set_initial_image_callback(
    index: ::std::os::raw::c_uint,
    path: *const ::std::os::raw::c_char,
) -> bool {
    dbg!(index);
    dbg!(path);
    println!("TODO: retro_set_initial_image_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_get_image_path_callback(
    index: ::std::os::raw::c_uint,
    path: *mut ::std::os::raw::c_char,
    len: size_t,
) -> bool {
    dbg!(index);
    dbg!(path);
    dbg!(len);
    println!("TODO: retro_get_image_path_callback");
    false
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_get_image_label_callback(
    index: ::std::os::raw::c_uint,
    label: *mut ::std::os::raw::c_char,
    len: size_t,
) -> bool {
    dbg!(index);
    dbg!(label);
    dbg!(len);
    println!("TODO: retro_get_image_label_callback");
    false
}

#[no_mangle]
pub unsafe extern "C" fn retro_frame_time_callback_fn(usec: retro_usec_t) {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        wrapper.frame_delta = Some(usec)
    }
}

/// Notifies the [`Core`] when audio data should be written.
#[no_mangle]
pub unsafe extern "C" fn retro_audio_callback_fn() {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        let mut ctx = AudioContext {
            environment_callback: &wrapper.environment_callback,

            audio_sample_callback: &wrapper.audio_sample_callback,
            audio_sample_batch_callback: &wrapper.audio_sample_batch_callback,
        };

        wrapper.core.on_write_audio(&mut ctx);
    }
}

/// Notifies the [`Core`] about the state of the frontend’s audio system.
///
/// [`true`]: Audio driver in frontend is active, and callback is
/// expected to be called regularily.
///
/// [`false`]: Audio driver in frontend is paused or inactive.
///
/// Audio callback will not be called until set_state has been
/// called with [`true`].
///
/// Initial state is [`false`] (inactive).
#[no_mangle]
pub unsafe extern "C" fn retro_audio_set_state_callback_fn(enabled: bool) {
    if let Some(wrapper) = RETRO_INSTANCE.as_mut() {
        wrapper.core.on_audio_set_state(enabled);
    }
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_camera_frame_raw_framebuffer_callback(
    buffer: *const u32,
    width: ::std::os::raw::c_uint,
    height: ::std::os::raw::c_uint,
    pitch: size_t,
) {
    dbg!(buffer);
    dbg!(width);
    dbg!(height);
    dbg!(pitch);
    println!("TODO: retro_camera_frame_raw_framebuffer_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_camera_frame_opengl_texture_callback(
    texture_id: ::std::os::raw::c_uint,
    texture_target: ::std::os::raw::c_uint,
    affine: *const f32,
) {
    dbg!(texture_id);
    dbg!(texture_target);
    dbg!(affine);
    println!("TODO: retro_camera_frame_opengl_texture_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_camera_initialized_callback() {
    println!("TODO: retro_camera_initialized_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_camera_deinitialized_callback() {
    println!("TODO: retro_camera_deinitialized_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_location_lifetime_status_initialized_callback() {
    println!("TODO: retro_location_lifetime_status_initialized_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_location_lifetime_status_deinitialized_callback() {
    println!("TODO: retro_location_lifetime_status_deinitialized_callback")
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_get_proc_address_callback(
    sym: *const ::std::os::raw::c_char,
) -> retro_proc_address_t {
    dbg!(sym);
    println!("TODO: retro_get_proc_address_callback");
    None
}

/// **TODO:** Not exposed to [`Core`] yet.
#[no_mangle]
pub unsafe extern "C" fn retro_audio_buffer_status_callback_fn(
    active: bool,
    occupancy: ::std::os::raw::c_uint,
    underrun_likely: bool,
) {
    dbg!(active);
    dbg!(occupancy);
    dbg!(underrun_likely);
    println!("TODO: retro_audio_buffer_status_callback_fn")
}
