//! JNI entry points for `digital.kuduy.kudownloader.core.Native`.
//! Strings in, strings out: every structured value travels as JSON.

use jni::objects::{JObject, JString};
use jni::sys::{jboolean, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;

fn text(env: &mut JNIEnv, s: &JString) -> String {
    if s.is_null() {
        return String::new();
    }
    env.get_string(s).map(Into::into).unwrap_or_default()
}

fn out(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}

/// A panic must never cross into the JVM (it would abort the app).
fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(fallback)
}

/// Returns "" when started, otherwise the reason it could not start.
#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_init<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, config: JString<'l>) -> jstring {
    let config = text(&mut env, &config);
    let r = guard(Err(anyhow::anyhow!("KuCore crashed while starting")), || crate::init(&config));
    let msg = match r {
        Ok(()) => String::new(),
        Err(e) => format!("{e:#}"),
    };
    out(&mut env, &msg)
}

#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_call<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, method: JString<'l>, args: JString<'l>) -> jstring {
    let method = text(&mut env, &method);
    let args = text(&mut env, &args);
    let r = guard(r#"{"error":"KuCore stopped unexpectedly"}"#.to_string(), || crate::call(&method, &args));
    out(&mut env, &r)
}

#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_nextEvents<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, timeout_ms: jlong) -> jstring {
    let r = guard("[]".to_string(), || crate::next_events(timeout_ms.max(0) as u64));
    out(&mut env, &r)
}

#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_te<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, msg: JString<'l>) -> jstring {
    let m = text(&mut env, &msg);
    let r = guard(m.clone(), || crate::te(&m));
    out(&mut env, &r)
}

#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_shouldBlock<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, url: JString<'l>, source: JString<'l>, kind: JString<'l>) -> jboolean {
    let (u, s, k) = (text(&mut env, &url), text(&mut env, &source), text(&mut env, &kind));
    if guard(false, || crate::adblock_engine::should_block(&u, &s, &k)) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_cosmetic<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, url: JString<'l>) -> jstring {
    let u = text(&mut env, &url);
    let r = guard(serde_json::Value::Null, || crate::adblock_engine::cosmetic(&u));
    out(&mut env, &r.to_string())
}

/// `request` is `{"classes": [...], "ids": [...], "exceptions": [...]}`.
#[no_mangle]
pub extern "system" fn Java_digital_kuduy_kudownloader_core_Native_hidden<'l>(mut env: JNIEnv<'l>, _this: JObject<'l>, request: JString<'l>) -> jstring {
    let req = text(&mut env, &request);
    let r = guard(Vec::new(), || {
        let v: serde_json::Value = serde_json::from_str(&req).unwrap_or_default();
        let list = |k: &str| -> Vec<String> { v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).take(4000).collect()).unwrap_or_default() };
        crate::adblock_engine::hidden(&list("classes"), &list("ids"), &list("exceptions"))
    });
    out(&mut env, &serde_json::Value::from(r).to_string())
}
