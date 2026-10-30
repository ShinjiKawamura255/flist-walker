//! Metadata/filter-construction ONLY. No StartTrace/EnableTrace/ReadProcessMemory.
//! Based on Microsoft tdh.h blob7f299ee728891e35bbe3a6fdf34dc5cecae312ad.
use std::fmt::Write;
#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    a: u32,
    b: u16,
    c: u16,
    d: [u8; 8],
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Descriptor {
    id: u16,
    version: u8,
    channel: u8,
    level: u8,
    opcode: u8,
    task: u16,
    keyword: u64,
}
fn u32_at(b: &[u8], p: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(p..p.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn u16_at(b: &[u8], p: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        b.get(p..p.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn tid_fields(b: &[u8]) -> Option<Vec<&'static str>> {
    if b.len() < 112 {
        return None;
    }
    let count = u32_at(b, 100)? as usize;
    let top = u32_at(b, 104)? as usize;
    if count > 256 || top > count || 112usize.checked_add(count.checked_mul(24)?)? > b.len() {
        return None;
    }
    let allowed = [
        "TTID",
        "IssuingThreadId",
        "ThreadId",
        "OldThreadId",
        "NewThreadId",
    ];
    let mut found = Vec::new();
    for i in 0..top {
        let p = 112 + i * 24;
        let flags = u32_at(b, p)?;
        let offset = u32_at(b, p + 4)? as usize;
        if flags != 0
            || u16_at(b, p + 8)? != 8
            || u16_at(b, p + 16)? != 1
            || u32_at(b, p + 12)? != 0
        {
            continue;
        } // scalar win:UInt32 only
        if !offset.is_multiple_of(2) || offset < 112 + count * 24 || offset >= b.len() {
            return None;
        }
        let mut units = Vec::new();
        let mut closed = false;
        for n in 0..128 {
            let unit = u16_at(b, offset + n * 2)?;
            if unit == 0 {
                closed = true;
                break;
            }
            units.push(unit)
        }
        if !closed {
            return None;
        }
        for name in allowed {
            if units == name.encode_utf16().collect::<Vec<_>>() {
                found.push(name)
            }
        }
    }
    Some(found)
}

#[derive(Clone, Copy)]
pub(super) struct Fault {
    kind: &'static str,
    provider: &'static str,
    stage: &'static str,
    event: Option<(u16, u8)>,
    code: Option<u32>,
}
pub(super) fn render_report(rows: &[String], fault: Option<Fault>) -> (String, u8) {
    let fault = fault.or_else(|| {
        (rows.len() != 2).then_some(Fault {
            kind: "insufficient",
            provider: "none",
            stage: "provider-count",
            event: None,
            code: None,
        })
    });
    let complete = fault.is_none();
    let error = if let Some(f) = fault {
        let (id, version) = f
            .event
            .map(|(i, v)| (i.to_string(), v.to_string()))
            .unwrap_or(("null".into(), "null".into()));
        format!(
            r#"{{"kind":"{}","provider":"{}","stage":"{}","event_id":{},"version":{},"code":{}}}"#,
            f.kind,
            f.provider,
            f.stage,
            id,
            version,
            f.code.map(|x| x.to_string()).unwrap_or("null".into())
        )
    } else {
        "null".into()
    };
    let mut out = String::new();
    let _ = write!(
        out,
        r#"{{"mode":"metadata-only","query_status":"{}","trace_started":false,"routing_verified":false,"complete":{},"error":{},"providers":[{}]}}"#,
        if complete { "complete" } else { "incomplete" },
        complete,
        error,
        rows.join(",")
    );
    if out.len() > 65536 {
        return (r#"{"mode":"metadata-only","query_status":"incomplete","complete":false,"trace_started":false,"routing_verified":false,"error":{"kind":"limit","provider":"none","stage":"output","event_id":null,"version":null,"code":null},"providers":[]}"#.into(),78);
    }
    (out, if complete { 0 } else { 78 })
}
#[cfg(all(windows, target_env = "msvc"))]
pub(super) mod native {
    use super::*;
    use std::ffi::c_void;
    use std::time::{Duration, Instant};
    #[repr(C)]
    struct Predicate {
        name: *mut u16,
        op: u16,
        value: *mut u16,
    }
    #[link(name = "tdh")]
    extern "system" {
        fn TdhEnumerateManifestProviderEvents(g: *const Guid, b: *mut c_void, n: *mut u32) -> u32;
        fn TdhGetManifestEventInformation(
            g: *const Guid,
            e: *const Descriptor,
            b: *mut c_void,
            n: *mut u32,
        ) -> u32;
        fn TdhCreatePayloadFilter(
            g: *const Guid,
            e: *const Descriptor,
            any: u8,
            n: u32,
            p: *mut Predicate,
            out: *mut *mut c_void,
        ) -> u32;
        fn TdhDeletePayloadFilter(p: *mut *mut c_void) -> u32;
    }
    const _: unsafe extern "system" fn(*mut *mut c_void) -> u32 = TdhDeletePayloadFilter;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    fn buffer<F: FnMut(*mut c_void, &mut u32) -> u32>(mut f: F) -> Result<Vec<u8>, u32> {
        let mut size = 0;
        let status = f(std::ptr::null_mut(), &mut size);
        if status != 122 {
            return Err(status);
        }
        if size == 0 || size > 262144 {
            return Err(0xffff0001);
        }
        let mut words = vec![0u64; (size as usize).div_ceil(8)];
        let capacity = size;
        let status = f(words.as_mut_ptr().cast(), &mut size);
        if status != 0 {
            return Err(status);
        }
        if size > capacity {
            return Err(0xffff0002);
        }
        Ok(unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), size as usize) }.to_vec())
    }
    fn query(label: &'static str, guid: &Guid, start: Instant) -> Result<String, Fault> {
        let fault = |kind, stage, event, code| Fault {
            kind,
            provider: label,
            stage,
            event,
            code,
        };
        let check = || {
            if start.elapsed() > Duration::from_secs(10) {
                Err(fault("limit", "time", None, None))
            } else {
                Ok(())
            }
        };
        check()?;
        let b = buffer(|b, n| unsafe { TdhEnumerateManifestProviderEvents(guid, b, n) })
            .map_err(|c| fault("api-error", "enumerate", None, Some(c)))?;
        let n = u32_at(&b, 0)
            .ok_or_else(|| fault("insufficient", "enumeration-layout", None, None))?
            as usize;
        if n == 0 {
            return Err(fault("insufficient", "empty-events", None, None));
        }
        if n > 256 || 8 + n * 16 > b.len() {
            return Err(fault("limit", "event-count-or-layout", None, None));
        }
        let mut events = Vec::new();
        let mut bytes = 0;
        for i in 0..n {
            check()?;
            let desc = unsafe {
                std::ptr::read_unaligned(b.as_ptr().add(8 + i * 16).cast::<Descriptor>())
            };
            let event = Some((desc.id, desc.version));
            let meta = buffer(|b, n| unsafe { TdhGetManifestEventInformation(guid, &desc, b, n) })
                .map_err(|c| fault("api-error", "event-metadata", event, Some(c)))?;
            let fields = tid_fields(&meta)
                .ok_or_else(|| fault("insufficient", "metadata-layout", event, None))?;
            for name in fields {
                check()?;
                let mut field = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                let mut value = unsafe { GetCurrentThreadId() }
                    .to_string()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect::<Vec<_>>();
                let mut predicate = Predicate {
                    name: field.as_mut_ptr(),
                    op: 0,
                    value: value.as_mut_ptr(),
                };
                let mut filter = std::ptr::null_mut();
                let code = unsafe {
                    TdhCreatePayloadFilter(guid, &desc, 0, 1, &mut predicate, &mut filter)
                };
                if code != 0 {
                    return Err(fault("api-error", "filter-build", event, Some(code)));
                }
                if filter.is_null() {
                    return Err(fault("insufficient", "filter-null", event, Some(code)));
                }
                // Always attempt owned allocation cleanup before checking budgets again.
                let cleanup = unsafe { TdhDeletePayloadFilter(&mut filter) };
                if cleanup != 0 {
                    return Err(fault("api-error", "filter-cleanup", event, Some(cleanup)));
                }
                check()?;
                let row = format!(
                    r#"{{"id":{},"version":{},"field":"{}","filter_build_status":0,"filter_cleanup_status":0}}"#,
                    desc.id, desc.version, name
                );
                bytes += row.len();
                if bytes > 24576 || events.len() >= 128 {
                    return Err(fault("limit", "event-output", event, None));
                }
                events.push(row);
            }
        }
        if events.is_empty() {
            return Err(fault("insufficient", "no-scalar-tid-field", None, None));
        }
        Ok(format!(
            r#"{{"provider":"{label}","status":0,"event_count":{n},"events":[{}]}}"#,
            events.join(",")
        ))
    }
    pub fn run(start: Instant) -> (String, u8) {
        // Metadata/filter allocation only. No privilege or process changes/trace.
        let providers = [
            (
                "system-io",
                Guid {
                    a: 0x3d5c43e3,
                    b: 0x0f1c,
                    c: 0x4202,
                    d: [0xb8, 0x17, 0x17, 0x4c, 0x00, 0x70, 0xdc, 0x79],
                },
            ),
            (
                "system-scheduler",
                Guid {
                    a: 0x599a2a76,
                    b: 0x4d91,
                    c: 0x4910,
                    d: [0x9a, 0xc7, 0x7d, 0x33, 0xf2, 0xe9, 0x7a, 0x6c],
                },
            ),
        ];
        let mut rows = Vec::new();
        let mut failure = None;
        for (label, guid) in providers {
            match query(label, &guid, start) {
                Ok(row) => rows.push(row),
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        render_report(&rows, failure)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn meta(name: &str) -> Vec<u8> {
        let mut b = vec![0u8; 136];
        b[100..104].copy_from_slice(&1u32.to_le_bytes());
        b[104..108].copy_from_slice(&1u32.to_le_bytes());
        b[116..120].copy_from_slice(&136u32.to_le_bytes());
        b[120..122].copy_from_slice(&8u16.to_le_bytes());
        b[128..130].copy_from_slice(&1u16.to_le_bytes());
        for x in name.encode_utf16().chain(Some(0)) {
            b.extend_from_slice(&x.to_le_bytes())
        }
        b
    }
    #[test]
    fn abi_layouts() {
        assert_eq!(std::mem::size_of::<Guid>(), 16);
        assert_eq!(std::mem::size_of::<Descriptor>(), 16);
    }
    #[test]
    fn keeps_only_fixed_numeric_tid_names() {
        assert_eq!(tid_fields(&meta("OldThreadId")), Some(vec!["OldThreadId"]));
        assert_eq!(tid_fields(&meta("SecretPath")), Some(vec![]));
    }
    #[test]
    fn rejects_truncated_and_unterminated_metadata() {
        assert!(tid_fields(&[0; 111]).is_none());
        let mut b = meta("TTID");
        b.truncate(b.len() - 2);
        assert!(tid_fields(&b).is_none());
    }
    #[test]
    fn rejects_outside_offsets_and_property_counts() {
        let mut b = meta("TTID");
        b[116..120].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(tid_fields(&b).is_none());
        let mut b = meta("TTID");
        b[100..104].copy_from_slice(&257u32.to_le_bytes());
        assert!(tid_fields(&b).is_none());
    }
    #[test]
    fn refuses_pointer_and_struct_fields() {
        let mut b = meta("ThreadId");
        b[120..122].copy_from_slice(&16u16.to_le_bytes());
        assert_eq!(tid_fields(&b), Some(vec![]));
        let mut b = meta("ThreadId");
        b[112..116].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(tid_fields(&b), Some(vec![]));
    }
    #[test]
    fn scalar_rejects_zero_multiple_and_mapped_fields() {
        for count in [0u16, 2] {
            let mut b = meta("TTID");
            b[128..130].copy_from_slice(&count.to_le_bytes());
            assert_eq!(tid_fields(&b), Some(vec![]));
        }
        let mut b = meta("TTID");
        b[124..128].copy_from_slice(&136u32.to_le_bytes());
        assert_eq!(tid_fields(&b), Some(vec![]));
    }

    #[test]
    fn incomplete_outcomes_never_render_complete_or_zero_exit() {
        for stage in [
            "enumerate",
            "event-metadata",
            "empty-events",
            "no-scalar-tid-field",
            "filter-build",
            "filter-cleanup",
        ] {
            let fault = Fault {
                kind: "api-error",
                provider: "system-io",
                stage,
                event: Some((4, 1)),
                code: Some(5),
            };
            let (out, code) = render_report(&[], Some(fault));
            assert_eq!(code, 78);
            assert!(
                out.contains("\"complete\":false")
                    && out.contains(stage)
                    && out.contains("\"code\":5")
                    && out.contains("\"routing_verified\":false")
            );
        }
        assert_eq!(render_report(&[], None).1, 78);
    }
    #[test]
    fn successful_metadata_is_still_not_routing_or_capture_proof() {
        let (out, code) = render_report(&["{}".into(), "{}".into()], None);
        assert_eq!(code, 0);
        assert!(
            out.contains("\"trace_started\":false") && out.contains("\"routing_verified\":false")
        );
    }
    #[test]
    fn oversized_report_refuses_without_partial_payload() {
        let (out, code) = render_report(&["x".repeat(70000), "{}".into()], None);
        assert_eq!(code, 78);
        assert!(out.contains("\"complete\":false") && out.contains("\"routing_verified\":false"));
        assert!(out.len() < 300);
    }
}
