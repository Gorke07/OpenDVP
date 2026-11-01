//! Current COMMGR ES2 DTM15, recovered from 42B64C..42C037.
use super::DtmSolarParameters;
use super::installed::{Control, Ext, cos, e, inverse, sin, tan};

const FIFTEENTH: Ext = Ext([0x89, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0xfb, 0x3f]);
const TWENTY_FOURTH: Ext = Ext([0xab, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xfa, 0x3f]);
const ONE_EIGHTIETH: Ext = Ext([0x0b, 0xb6, 0x60, 0x0b, 0xb6, 0x60, 0x0b, 0xb6, 0xf7, 0x3f]);
const NINETIETH: Ext = Ext([0x0b, 0xb6, 0x60, 0x0b, 0xb6, 0x60, 0x0b, 0xb6, 0xf8, 0x3f]);
const INVERSE_PI: Ext = Ext([0x33, 0xaf, 0x43, 0x4e, 0x6e, 0x83, 0xf9, 0xa2, 0xfd, 0x3f]);

fn literal(bits: u64) -> Ext {
    e(f64::from_bits(bits))
}

fn f(value: f32) -> Ext {
    e(f64::from(value))
}

fn radians(value: f32) -> f64 {
    (literal(0x4009_21fb_5444_2d28) * f(value) * ONE_EIGHTIETH).f64()
}

/// The native helper uses signed division toward zero and never adds a century.
fn ordinal([year, month, day]: [i16; 3]) -> i16 {
    const ADJUST: [i32; 13] = [0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const OFFSET: [i32; 13] = [0, 0, -31, 306, 275, 245, 214, 184, 153, 122, 92, 61, 31];
    let serial = |month: usize, day: i32| {
        let adjusted = i32::from(year) - ADJUST[month];
        let century = adjusted / 100;
        let remainder = adjusted % 100;
        century * 146097 / 4 + remainder * 1461 / 4 - OFFSET[month] + day
    };
    (serial(month as usize, i32::from(day)) - serial(1, 1) + 1) as i16
}

/// Both endpoints are included; negative exact multiples can become `high`.
fn normalize(value: f32, low: f32, high: f32) -> f32 {
    let width = (f(high) - f(low)).f32();
    if value < low {
        let steps = (e(((f(low) - f(value)) / f(width)).f64().floor()) + e(1.0)).f32();
        (f(steps) * f(width) + f(value)).f32()
    } else if value > high {
        let steps = (e(((f(value) - f(high)) / f(width)).f64().floor()) + e(1.0)).f32();
        (f(value) - f(steps) * f(width)).f32()
    } else {
        value
    }
}

pub(super) fn event(
    parameters: DtmSolarParameters,
    date: [i16; 3],
    sunset: bool,
) -> Option<[i16; 3]> {
    let _control = Control::enter();
    let longitude_hours = (FIFTEENTH * f(parameters.longitude)).f32();
    let approximate = ((e(if sunset { 18.0 } else { 6.0 }) - f(longitude_hours)) * TWENTY_FOURTH
        + e(f64::from(ordinal(date))))
    .f32();
    let anomaly =
        (literal(0x3fef_8a09_02de_00d2) * f(approximate) - literal(0x400a_4fdf_3b64_5a1d)).f32();
    let first = sin(radians(anomaly)) * literal(0x3ffe_a7ef_9db2_2d0e) + f(anomaly);
    let twice = (literal(0x4019_21fb_5444_2d28) * f(anomaly) * ONE_EIGHTIETH).f64();
    let longitude = normalize(
        (first + sin(twice) * literal(0x3f94_7ae1_47ae_147b) + literal(0x4071_aa24_dd2f_1aa0))
            .f32(),
        0.0,
        360.0,
    );
    let tangent = (tan(radians(longitude)) * literal(0x3fed_5d4e_8fb0_0bcc)).f64();
    let mut ascension = normalize(
        (e(tangent).atan() * e(180.0) * INVERSE_PI).f32(),
        0.0,
        360.0,
    );
    let longitude_quadrant = (e((NINETIETH * f(longitude)).f64().floor()) * e(90.0)).f32();
    let ascension_quadrant = (e((NINETIETH * f(ascension)).f64().floor()) * e(90.0)).f32();
    ascension = (f(ascension) + f(longitude_quadrant) - f(ascension_quadrant)).f32();
    ascension = (FIFTEENTH * f(ascension)).f32();
    let sin_declination = (sin(radians(longitude)) * literal(0x3fd9_75e2_046c_764b)).f32();
    // The current asin RTL returns positive zero for either signed zero input.
    let declination = if sin_declination == 0.0 {
        e(0.0)
    } else {
        inverse(f64::from(sin_declination), false)
    };
    let cos_declination = cos(declination.f64()).f32();
    let zenith = f32::from_bits(if sunset { 0x42b5_a8f6 } else { 0x42b5_aa7f });
    let numerator = cos(radians(zenith)) - sin(radians(parameters.latitude)) * f(sin_declination);
    let denominator = cos(radians(parameters.latitude)) * f(cos_declination);
    let cos_hour = (numerator / denominator).f32();
    if !(-1.0..=1.0).contains(&cos_hour) {
        // The two native comparisons also hold this triplet for unordered NaN.
        return None;
    }
    let angle = inverse(f64::from(cos_hour), true) * e(180.0) * INVERSE_PI;
    let hour_angle = if sunset { angle } else { e(360.0) - angle }.f32();
    let hour_angle = (FIFTEENTH * f(hour_angle)).f32();
    let mean_time = (f(hour_angle) + f(ascension)
        - literal(0x3fb0_d25e_dd05_2935) * f(approximate)
        - literal(0x401a_7ced_9168_72b0))
    .f32();
    let utc = normalize((f(mean_time) - f(longitude_hours)).f32(), 0.0, 24.0);
    let mut local = (e(f64::from(parameters.timezone_hours)) + f(utc)).f32();
    if local < 0.0 {
        local = (f(local) + e(24.0)).f32();
    } else if local > 24.0 {
        local = (f(local) - e(24.0)).f32();
    }
    let whole_hour = e(f64::from(local).floor());
    let hour = ((whole_hour + e(if parameters.daylight_saving { 1.0 } else { 0.0 })).trunc_i64()
        as i16)
        % 24;
    let minute = ((f(local) - whole_hour) * e(60.0)).trunc_i64() as i16;
    let minutes = f(local) * e(60.0);
    let second = ((minutes - e(minutes.f64().floor())) * e(60.0)).trunc_i64() as i16;
    Some([hour, minute, second])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn taoyuan() -> DtmSolarParameters {
        DtmSolarParameters {
            longitude: 121.301,
            latitude: 24.993,
            timezone_hours: 8,
            daylight_saving: false,
        }
    }

    #[test]
    fn dtm_solar_ordinal_preserves_signed_year_zero_without_century_inference() {
        for (date, expected) in [
            ([18, 1, 1], 1),
            ([18, 12, 31], 365),
            ([0, 2, 29], 60),
            ([0, 3, 1], 60),
            ([4, 3, 1], 61),
            ([24, 3, 1], 61),
            ([100, 3, 1], 60),
            ([2000, 3, 1], 61),
        ] {
            assert_eq!(ordinal(date), expected, "{date:?}");
        }
    }

    #[test]
    fn dtm_solar_normalizer_preserves_both_endpoints_and_negative_multiples() {
        let _control = Control::enter();
        for (input, expected) in [
            (-720.0, 360.0),
            (-360.0, 360.0),
            (-1.0, 359.0),
            (0.0, 0.0),
            (360.0, 360.0),
            (361.0, 1.0),
            (720.0, 0.0),
        ] {
            assert_eq!(normalize(input, 0.0, 360.0), expected);
        }
        assert_eq!(normalize(-24.0, 0.0, 24.0), 24.0);
        assert_eq!(normalize(24.0, 0.0, 24.0), 24.0);
        assert_eq!(normalize(48.0, 0.0, 24.0), 0.0);
    }

    #[test]
    fn dtm_solar_matches_literal_manual_example_within_published_three_seconds() {
        for (sunset, expected) in [(false, [6, 39, 47]), (true, [17, 16, 42])] {
            let actual = event(taoyuan(), [18, 1, 1], sunset).unwrap();
            let seconds =
                |v: [i16; 3]| i32::from(v[0]) * 3600 + i32::from(v[1]) * 60 + i32::from(v[2]);
            assert!(
                (seconds(actual) - seconds(expected)).abs() <= 3,
                "{actual:?}"
            );
        }
    }

    #[test]
    fn dtm_solar_polar_events_hold_each_half_and_dst_changes_only_hour() {
        for latitude in [-90.0, 90.0] {
            let parameters = DtmSolarParameters {
                latitude,
                ..taoyuan()
            };
            for date in [[24, 6, 21], [24, 12, 21]] {
                assert_eq!(event(parameters, date, false), None);
                assert_eq!(event(parameters, date, true), None);
            }
        }
        for sunset in [false, true] {
            let ordinary = event(taoyuan(), [18, 1, 1], sunset).unwrap();
            let dst = event(
                DtmSolarParameters {
                    daylight_saving: true,
                    ..taoyuan()
                },
                [18, 1, 1],
                sunset,
            )
            .unwrap();
            assert_eq!(dst, [(ordinary[0] + 1) % 24, ordinary[1], ordinary[2]]);
        }
    }
}
