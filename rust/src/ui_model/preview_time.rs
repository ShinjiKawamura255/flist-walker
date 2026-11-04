//! Preview display conversion. Call on the preview worker: OS timezone lookup
//! may read timezone data. Stored timestamps and UTC comparisons stay unchanged.
use std::time::{Duration, SystemTime};

use super::preview::{format_system_time, unix_seconds};

pub(super) fn format_local_system_time(time: SystemTime) -> Option<String> {
    format_with_offset(time, local_offset_seconds)
}

fn format_with_offset(
    time: SystemTime,
    offset: impl FnOnce(SystemTime) -> Option<i64>,
) -> Option<String> {
    let Some(offset) = offset(time).filter(|offset| (-86_400..=86_400).contains(offset)) else {
        // Explicit UTC is also the fallback for unsupported platform/date ranges.
        return format_system_time(time);
    };
    let magnitude = Duration::from_secs(offset.unsigned_abs());
    let local = if offset < 0 {
        time.checked_sub(magnitude)
    } else {
        time.checked_add(magnitude)
    };
    let Some(local) = local else {
        return format_system_time(time);
    };
    let mut text = format_system_time(local)?;
    let magnitude = offset.unsigned_abs();
    text.push_str(&format!(
        "{}{:02}:{:02}",
        if offset < 0 { '-' } else { '+' },
        magnitude / 3_600,
        magnitude % 3_600 / 60,
    ));
    // Historical local mean time can have a sub-minute offset.
    if magnitude % 60 != 0 {
        text.push_str(&format!(":{:02}", magnitude % 60));
    }
    Some(text)
}

#[cfg(unix)]
fn local_offset_seconds(time: SystemTime) -> Option<i64> {
    // Check the platform's time_t range (including 32-bit Unix targets).
    let seconds = libc::time_t::try_from(i128::from(unix_seconds(time)?)).ok()?;
    let mut local = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are valid, separate caller-owned values. localtime_r
    // initializes local on success; neither a static tm nor tzname is accessed.
    if unsafe { libc::localtime_r(&seconds, local.as_mut_ptr()) }.is_null() {
        return None;
    }
    // SAFETY: localtime_r returned non-null and initialized the output above.
    i64::try_from(i128::from(unsafe { local.assume_init() }.tm_gmtoff)).ok()
}

#[cfg(windows)]
fn local_offset_seconds(time: SystemTime) -> Option<i64> {
    windows::local_offset_seconds(time)
}

#[cfg(not(any(unix, windows)))]
fn local_offset_seconds(_time: SystemTime) -> Option<i64> {
    None
}

#[cfg(windows)]
mod windows {
    use super::*;

    #[repr(C)]
    #[derive(Default)]
    struct WindowsSystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    #[repr(C)]
    struct DynamicTimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: WindowsSystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: WindowsSystemTime,
        daylight_bias: i32,
        time_zone_key_name: [u16; 128],
        dynamic_daylight_time_disabled: u8,
    }

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDynamicTimeZoneInformation(zone: *mut DynamicTimeZoneInformation) -> u32;
        fn FileTimeToSystemTime(file: *const FileTime, system: *mut WindowsSystemTime) -> i32;
        fn SystemTimeToFileTime(system: *const WindowsSystemTime, file: *mut FileTime) -> i32;
        fn SystemTimeToTzSpecificLocalTimeEx(
            zone: *const DynamicTimeZoneInformation,
            utc: *const WindowsSystemTime,
            local: *mut WindowsSystemTime,
        ) -> i32;
    }

    pub(super) fn local_offset_seconds(time: SystemTime) -> Option<i64> {
        let mut zone = std::mem::MaybeUninit::<DynamicTimeZoneInformation>::uninit();
        // SAFETY: zone has the Windows SDK layout and is writable for the call.
        if unsafe { GetDynamicTimeZoneInformation(zone.as_mut_ptr()) } == u32::MAX {
            return None;
        }
        // SAFETY: the successful API call fully initialized this structure.
        offset_in_zone(time, &unsafe { zone.assume_init() })
    }

    fn offset_in_zone(time: SystemTime, zone: &DynamicTimeZoneInformation) -> Option<i64> {
        // FILETIME uses unsigned 100ns ticks since 1601. Reject out-of-range
        // timestamps rather than wrap; the caller then displays explicit UTC.
        let ticks =
            u64::try_from((i128::from(unix_seconds(time)?) + 11_644_473_600) * 10_000_000).ok()?;
        let file = FileTime {
            low: ticks as u32,
            high: (ticks >> 32) as u32,
        };
        let mut utc = WindowsSystemTime::default();
        let mut local = WindowsSystemTime::default();
        let mut local_file = FileTime::default();
        // SAFETY: all SDK-layout inputs/outputs are valid for the synchronous
        // calls. Outputs are consumed only if every API reports success. Ex
        // uses historical dynamic DST rules for this instant, not today's bias.
        unsafe {
            if FileTimeToSystemTime(&file, &mut utc) == 0
                || SystemTimeToTzSpecificLocalTimeEx(zone, &utc, &mut local) == 0
                || SystemTimeToFileTime(&local, &mut local_file) == 0
            {
                return None;
            }
        }
        let local_ticks = (u64::from(local_file.high) << 32) | u64::from(local_file.low);
        i64::try_from((i128::from(local_ticks) - i128::from(ticks)) / 10_000_000).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::time::UNIX_EPOCH;

        #[link(name = "advapi32")]
        extern "system" {
            fn EnumDynamicTimeZoneInformation(
                index: u32,
                zone: *mut DynamicTimeZoneInformation,
            ) -> u32;
        }

        fn registered_zone(key: &str) -> DynamicTimeZoneInformation {
            let key: Vec<u16> = key.encode_utf16().collect();
            // Read real OS entries without changing the machine's timezone.
            // SystemTimeToTzSpecificLocalTimeEx needs a registered key even
            // for fixed zones; a hand-built bias-only fixture is not valid.
            for index in 0..1_024 {
                let mut zone = std::mem::MaybeUninit::<DynamicTimeZoneInformation>::uninit();
                // SAFETY: the SDK-layout output is valid for the synchronous call.
                let status = unsafe { EnumDynamicTimeZoneInformation(index, zone.as_mut_ptr()) };
                if status == 259 {
                    // ERROR_NO_MORE_ITEMS
                    break;
                }
                assert_eq!(status, 0, "enumerate timezone index={index}");
                // SAFETY: ERROR_SUCCESS means the API initialized the output.
                let zone = unsafe { zone.assume_init() };
                let len = zone
                    .time_zone_key_name
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(zone.time_zone_key_name.len());
                if zone.time_zone_key_name[..len] == key {
                    return zone;
                }
            }
            panic!(
                "required Windows timezone not registered: {}",
                String::from_utf16_lossy(&key)
            );
        }

        #[test]
        fn windows_local_time_fixed_offsets_and_pre_epoch() {
            assert_eq!(std::mem::size_of::<DynamicTimeZoneInformation>(), 432);
            for (key, offset) in [
                ("Tokyo Standard Time", 32_400),
                ("Nepal Standard Time", 20_700),
                ("Pacific Standard Time", -28_800),
                ("UTC", 0),
            ] {
                let zone = registered_zone(key);
                for time in [UNIX_EPOCH, UNIX_EPOCH - Duration::from_nanos(100)] {
                    assert_eq!(
                        offset_in_zone(time, &zone),
                        Some(offset),
                        "zone={key}, time={time:?}, OS error={}",
                        std::io::Error::last_os_error()
                    );
                }
            }
            let before_windows_epoch = UNIX_EPOCH - Duration::from_secs(11_644_473_601);
            assert_eq!(
                offset_in_zone(before_windows_epoch, &registered_zone("UTC")),
                None
            );
        }

        #[test]
        fn windows_unregistered_timezone_conversion_falls_back_to_explicit_utc() {
            let mut zone = registered_zone("UTC");
            zone.time_zone_key_name = [0; 128];
            for (slot, value) in zone
                .time_zone_key_name
                .iter_mut()
                .zip("FlistWalkerMissingTimezoneForTest".encode_utf16())
            {
                *slot = value;
            }
            zone.dynamic_daylight_time_disabled = 0;
            assert_eq!(offset_in_zone(UNIX_EPOCH, &zone), None);
            assert_eq!(
                format_with_offset(UNIX_EPOCH, |time| offset_in_zone(time, &zone)).as_deref(),
                Some("1970-01-01 00:00 UTC")
            );
        }

        #[test]
        fn windows_local_time_uses_dynamic_dst_for_the_timestamp_year() {
            let mut pacific = registered_zone("Pacific Standard Time");
            pacific.dynamic_daylight_time_disabled = 0;
            for (seconds, expected) in [
                (1_772_963_940, "2026-03-08 01:59 UTC-08:00"),
                (1_772_964_000, "2026-03-08 03:00 UTC-07:00"),
                (1_793_521_800, "2026-11-01 01:30 UTC-07:00"),
                (1_793_525_400, "2026-11-01 01:30 UTC-08:00"),
                (1_142_856_000, "2006-03-20 04:00 UTC-08:00"),
            ] {
                let time = UNIX_EPOCH + Duration::from_secs(seconds);
                assert_eq!(
                    format_with_offset(time, |time| offset_in_zone(time, &pacific)).as_deref(),
                    Some(expected)
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn offsets_cross_date_and_year_boundaries() {
        for (seconds, offset, expected) in [
            (1_783_020_900, 32_400, "2026-07-03 04:35 UTC+09:00"),
            (1_783_020_900, -25_200, "2026-07-02 12:35 UTC-07:00"),
            (1_783_020_900, 20_700, "2026-07-03 01:20 UTC+05:45"),
            (0, -28_800, "1969-12-31 16:00 UTC-08:00"),
            (0, 0, "1970-01-01 00:00 UTC+00:00"),
            (0, 20_476, "1970-01-01 05:41 UTC+05:41:16"),
        ] {
            let time = UNIX_EPOCH + Duration::from_secs(seconds);
            assert_eq!(
                format_with_offset(time, |_| Some(offset)).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            // Windows SystemTime has 100ns resolution.
            format_with_offset(UNIX_EPOCH - Duration::from_nanos(100), |_| Some(32_400)).as_deref(),
            Some("1970-01-01 08:59 UTC+09:00")
        );
    }

    #[test]
    fn failed_or_invalid_timezone_conversion_is_explicitly_utc() {
        for offset in [None, Some(i64::MIN), Some(86_401)] {
            assert_eq!(
                format_with_offset(UNIX_EPOCH, |_| offset).as_deref(),
                Some("1970-01-01 00:00 UTC")
            );
        }
    }

    #[test]
    fn system_timezone_conversion_succeeds_for_a_modern_instant() {
        let time = UNIX_EPOCH + Duration::from_secs(1_783_020_900);
        let offset = local_offset_seconds(time).expect("system timezone available");
        assert_eq!(
            format_local_system_time(time),
            format_with_offset(time, |_| Some(offset))
        );
        assert!(format_local_system_time(time).unwrap().contains(" UTC"));
    }
}
