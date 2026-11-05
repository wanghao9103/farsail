use crate::Host;
use farsail_client::{Error, NativeClient, SecureStore};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JByteArray, JByteBuffer, JObject, JString, JValue},
    sys::{JNI_FALSE, JNI_TRUE, jboolean, jbyteArray, jint, jlong, jstring},
};
use serde_json::{Value, json};
use std::sync::{Arc, OnceLock};
use tokio::runtime::Runtime;

#[cfg(target_os = "android")]
static CONTEXT: OnceLock<GlobalRef> = OnceLock::new();
static HOST: OnceLock<Arc<Host>> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
struct AndroidStore {
    vm: JavaVM,
    object: GlobalRef,
}
impl AndroidStore {
    fn access<T>(
        &self,
        f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<T>,
    ) -> farsail_client::Result<T> {
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|_| Error::Store("Android VM unavailable".into()))?;
        let result = f(&mut env, self.object.as_obj());
        if env.exception_check().unwrap_or(true) {
            let _ = env.exception_clear();
            return Err(Error::Store("Android Keystore read/write failed".into()));
        }
        result.map_err(|_| Error::Store("Android Keystore read/write failed".into()))
    }
}
impl SecureStore for AndroidStore {
    fn read(&self, key: &str) -> farsail_client::Result<Option<Vec<u8>>> {
        self.access(|env, obj| {
            let key = env.new_string(key)?;
            let bytes = env
                .call_method(
                    obj,
                    "read",
                    "(Ljava/lang/String;)[B",
                    &[JValue::Object(key.as_ref())],
                )?
                .l()?;
            if bytes.is_null() {
                Ok(None)
            } else {
                Ok(Some(env.convert_byte_array(JByteArray::from(bytes))?))
            }
        })
    }
    fn write(&self, key: &str, value: &[u8]) -> farsail_client::Result<()> {
        self.access(|env, obj| {
            let key = env.new_string(key)?;
            let bytes = env.byte_array_from_slice(value)?;
            env.call_method(
                obj,
                "write",
                "(Ljava/lang/String;[B)V",
                &[JValue::Object(key.as_ref()), JValue::Object(bytes.as_ref())],
            )?;
            Ok(())
        })
    }
    fn delete(&self, key: &str) -> farsail_client::Result<()> {
        self.access(|env, obj| {
            let key = env.new_string(key)?;
            env.call_method(
                obj,
                "delete",
                "(Ljava/lang/String;)V",
                &[JValue::Object(key.as_ref())],
            )?;
            Ok(())
        })
    }
}
fn response(env: &mut JNIEnv, value: Result<Value, String>) -> jstring {
    let result = match value {
        Ok(v) => json!({"ok":true,"value":v}),
        Err(e) => json!({"ok":false,"error":e}),
    };
    env.new_string(result.to_string())
        .map(JString::into_raw)
        .unwrap_or(std::ptr::null_mut())
}
fn host() -> Result<&'static Arc<Host>, String> {
    HOST.get()
        .ok_or_else(|| "native host not initialized".into())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_viewerBegin(
    _: JNIEnv,
    _: JObject,
) -> jlong {
    host().map_or(-1, |h| h.viewer.begin() as jlong)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_viewerClose(
    _: JNIEnv,
    _: JObject,
    ticket: jlong,
) {
    if let Ok(h) = host() {
        h.viewer.close(ticket as u64);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_viewerFrame(
    env: JNIEnv,
    _: JObject,
    ticket: jlong,
) -> jbyteArray {
    host()
        .ok()
        .and_then(|h| h.viewer.frame(ticket as u64))
        .and_then(|bytes| env.byte_array_from_slice(&bytes).ok())
        .map_or(std::ptr::null_mut(), |bytes| bytes.into_raw())
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_initialize(
    mut env: JNIEnv,
    _: JObject,
    store: JObject,
    _context: JObject,
) -> jstring {
    let result = (|| {
        if HOST.get().is_some() {
            return Ok(Value::Null);
        }
        let vm = env.get_java_vm().map_err(|e| e.to_string())?;
        let object = env.new_global_ref(store).map_err(|e| e.to_string())?;
        #[cfg(target_os = "android")]
        if CONTEXT.get().is_none() {
            let context = env.new_global_ref(_context).map_err(|e| e.to_string())?;
            // SAFETY: process-lifetime VM and global Application reference; initialized
            // once on the application's single native operation executor.
            unsafe {
                iroh::dns::install_android_jni_context(
                    vm.get_java_vm_pointer().cast(),
                    context.as_obj().as_raw().cast(),
                );
            }
            let _ = CONTEXT.set(context);
        }

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(3)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let _ = RUNTIME.set(runtime);
        let _enter = RUNTIME.get().unwrap().enter();
        let client = Arc::new(
            NativeClient::new(Arc::new(AndroidStore { vm, object })).map_err(|e| e.to_string())?,
        );
        let host = Host::new(client);
        host.listen();
        HOST.set(host)
            .map_err(|_| "already initialized".to_string())?;
        Ok(Value::Null)
    })();
    response(&mut env, result)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_call(
    mut env: JNIEnv,
    _: JObject,
    op: JString,
    args: JString,
) -> jstring {
    let result = (|| {
        let op: String = env.get_string(&op).map_err(|e| e.to_string())?.into();
        let args: String = env.get_string(&args).map_err(|e| e.to_string())?.into();
        let args = serde_json::from_str(&args).map_err(|_| "invalid arguments".to_string())?;
        RUNTIME
            .get()
            .ok_or("runtime unavailable")?
            .block_on(host()?.call(&op, args))
    })();
    response(&mut env, result)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_begin(_: JNIEnv, _: JObject) -> jlong {
    host().map_or(-1, |h| h.begin() as jlong)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_stopLocal(_: JNIEnv, _: JObject) {
    if let Ok(h) = host() {
        h.stop_local();
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_setControl(
    _: JNIEnv,
    _: JObject,
    enabled: jboolean,
) {
    if let Ok(h) = host() {
        h.control(enabled != 0);
    }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_geometry(
    _: JNIEnv,
    _: JObject,
    w: jint,
    h: jint,
) -> jlong {
    if w <= 0 || h <= 0 || (w as u64) * (h as u64) > farsail_media::MAX_PIXELS as u64 {
        return -1;
    }
    host().map_or(-1, |host| host.geometry(w as u32, h as u32) as jlong)
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_publish(
    env: JNIEnv,
    _: JObject,
    bytes: JByteArray,
    w: jint,
    h: jint,
    layout: jlong,
) -> jboolean {
    let accepted = (|| {
        let hst = host().ok()?;
        if env.get_array_length(&bytes).ok()? as usize > farsail_media::MAX_BYTES {
            return None;
        }
        Some(hst.publish(
            env.convert_byte_array(bytes).ok()?,
            w as u32,
            h as u32,
            layout as u64,
        ))
    })()
    .unwrap_or(false);
    if accepted { JNI_TRUE } else { JNI_FALSE }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_publishRgba(
    env: JNIEnv,
    _: JObject,
    buffer: JByteBuffer,
    w: jint,
    h: jint,
    row_stride: jint,
    layout: jlong,
) -> jboolean {
    let accepted = (|| {
        let hst = host().ok()?;
        let (width, height) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
        let stride = usize::try_from(row_stride).ok()?;
        let span = crate::capture::rgba_span(width, height, stride, 4)?;
        if layout < 0 || span > env.get_direct_buffer_capacity(&buffer).ok()? {
            return None;
        }
        // Keep the authorized connection alive while an invisible tab pauses
        // video. Do not spend phone CPU encoding frames that nobody will show.
        if !hst.video_requested() {
            return Some(true);
        }
        let pointer = env.get_direct_buffer_address(&buffer).ok()?;
        // The direct buffer is borrowed only for this synchronous JNI call.
        // Kotlin keeps the acquired Image open until this function returns;
        // the checked span excludes final-row padding not mapped by ImageReader.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, span) };
        let image = crate::capture::encode_rgba(bytes, width, height, stride, layout as u64)?;
        Some(hst.publish(image.jpeg, width, height, layout as u64))
    })()
    .unwrap_or(false);
    if accepted { JNI_TRUE } else { JNI_FALSE }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_publishAccessibilityRgba(
    env: JNIEnv,
    _: JObject,
    buffer: JByteBuffer,
    w: jint,
    h: jint,
    row_stride: jint,
    epoch: jlong,
) -> jboolean {
    let accepted = (|| {
        let hst = host().ok()?;
        let (width, height) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
        let stride = usize::try_from(row_stride).ok()?;
        let span = crate::capture::rgba_span(width, height, stride, 4)?;
        if epoch < 0 || span > env.get_direct_buffer_capacity(&buffer).ok()? {
            return None;
        }
        let pointer = env.get_direct_buffer_address(&buffer).ok()?;
        // Kotlin owns this direct buffer for the duration of the synchronous call.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, span) };
        let image = hst.encode_screenshot(bytes, width, height, stride, epoch as u64)?;
        Some(hst.publish_accessibility(image.jpeg, width, height, epoch as u64))
    })()
    .unwrap_or(false);
    if accepted { JNI_TRUE } else { JNI_FALSE }
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_drain(
    mut env: JNIEnv,
    _: JObject,
) -> jstring {
    response(&mut env, host().map(|h| h.drain()))
}
#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_inputValid(
    mut env: JNIEnv,
    _: JObject,
    id: JString,
    epoch: jlong,
    layout: jlong,
) -> jboolean {
    let valid = env
        .get_string(&id)
        .ok()
        .and_then(|id| {
            host()
                .ok()
                .map(|h| h.input_valid(&id.to_string_lossy(), epoch as u64, layout as u64))
        })
        .unwrap_or(false);
    if valid { JNI_TRUE } else { JNI_FALSE }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_app_farsail_android_NativeBridge_verifyUpdate(
    mut env: JNIEnv,
    _: JObject,
    envelope: JString,
    installed: jlong,
) -> jstring {
    let result = (|| {
        let envelope: String = env
            .get_string(&envelope)
            .map_err(|_| "更新清单无效。".to_string())?
            .into();
        crate::updates::verify(&envelope, installed.max(0) as u64)
    })();
    response(&mut env, result)
}
