//! Installed ES2 DSPA reconstruction.
//! The numerical implementation follows recovered helper operations and literal tables.
use crate::plc::{Area, Device, Memory, word_range};
use std::collections::BTreeSet;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[path = "solar_position/dtm.rs"]
mod dtm;

/// DTM15 snapshots these fields before either event, but reads the date again
/// after publishing sunrise. This is separate from DSPA's position algorithm.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DtmSolarParameters {
    pub longitude: f32,
    pub latitude: f32,
    pub timezone_hours: i16,
    pub daylight_saving: bool,
}

impl DtmSolarParameters {
    pub(crate) fn event(self, date: [i16; 3], sunset: bool) -> Result<Option<[i16; 3]>, String> {
        // These are replay limits, not invented PLC errors or a calendar
        // conversion. The native date helper accepts signed year/day values.
        if !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
            || !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
        {
            return Err("DTM m=15: finite longitude -180..180 and latitude -90..90 are required for the verified numerical path".into());
        }
        if !(1..=12).contains(&date[1]) {
            return Err("DTM m=15: month must be 1..12 before the native date-table access".into());
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            Ok(dtm::event(self, date, sunset))
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let _ = sunset;
            Err(
                "DTM m=15: the verified x87 numerical path is unavailable on this architecture"
                    .into(),
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolarAction {
    pub source: Device,
    pub dest: Device,
}
impl SolarAction {
    pub fn parse(name: &str, args: &[String]) -> Result<Self, String> {
        if name != "DSPA" {
            return Err("Bilinmeyen güneş konumu komutu".into());
        }
        if args.len() != 2 {
            return Err("DSPA: kaynak ve hedef gerekli".into());
        }
        // The manual lists K/H sources but the installed wrapper treats the
        // payload as a byte address. Genuine WPL constant compilation is open.
        if args[0].starts_with('K') || args[0].starts_with('H') {
            return Err(
                "DSPA K/H kaynak adresleme sözleşmesi doğrulanmadı; D bloğu kullanın".into(),
            );
        }
        let source: Device = args[0].parse()?;
        let dest: Device = args[1].parse()?;
        if source.area != Area::D || dest.area != Area::D {
            return Err("DSPA kaynak ve hedefi D register bloğu olmalı".into());
        }
        word_range(source, 208)?;
        word_range(dest, 8)?;
        Ok(Self { source, dest })
    }
    pub fn name(&self) -> &'static str {
        "DSPA"
    }
    pub fn header(&self) -> u16 {
        0x8301
    }
    pub fn dest(&self) -> Device {
        self.dest
    }
    pub fn label(&self) -> String {
        format!("DSPA {} {}", self.source, self.dest)
    }
    pub fn devices(&self, out: &mut BTreeSet<Device>) {
        out.extend(word_range(self.source, 208).unwrap());
        out.extend(word_range(self.dest, 8).unwrap());
    }
    pub fn observed_devices(&self, m: &Memory, out: &mut BTreeSet<Device>) {
        let limit = m.data_register_limit.unwrap_or(9999);
        for (base, count) in [(self.source, 208u16), (self.dest, 8)] {
            for n in 0..count {
                if let Some(index) = base.index.checked_add(n).filter(|i| *i <= limit) {
                    out.insert(Device {
                        area: Area::D,
                        index,
                    });
                }
            }
        }
    }
    /// Conservative execution envelope for the installed math path. This is a
    /// replay safety guard, not an invented PLC validation/error contract.
    pub fn validate_installed_execution(&self, m: &Memory) -> Result<(), String> {
        let limit = m.data_register_limit.unwrap_or(9999);
        if ![(self.source, 208u16), (self.dest, 8)]
            .into_iter()
            .all(|(d, n)| {
                d.area == Area::D && d.index.checked_add(n - 1).is_some_and(|end| end <= limit)
            })
        {
            return Err("DSPA kaynak 208/hedef 8 kelime bloğu model D sınırını aşıyor".into());
        }
        let word = |n: u16| {
            m.word(Device {
                area: Area::D,
                index: self.source.index + n,
            })
        };
        let [year, month, day, hour, minute, second] = std::array::from_fn(|n| word(n as u16));
        if year < 2000
            || !(1..=12).contains(&month)
            || !(0..=23).contains(&hour)
            || !(0..=59).contains(&minute)
            || !(0..=59).contains(&second)
        {
            return Err("DSPA korumalı yürütme: geçerli 2000..32767 takvim tarihi ve 00:00:00..23:59:59 gerekli".into());
        }
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        if !(1..=days).contains(&day) {
            return Err("DSPA korumalı yürütme: ay için geçersiz gün".into());
        }
        for (n, low, high) in [
            (6, -8000.0, 8000.0),
            (10, -180.0, 180.0),
            (12, -90.0, 90.0),
            (14, 0.0, 6500000.0),
            (16, 0.0, 5000.0),
            (18, -273.0, 6000.0),
            (20, -360.0, 360.0),
            (22, -360.0, 360.0),
        ] {
            let value =
                f32::from_bits(u32::from(word(n) as u16) | (u32::from(word(n + 1) as u16) << 16));
            if !value.is_finite() || !(low..=high).contains(&value) || (n == 18 && value == -273.0)
            {
                return Err(format!(
                    "DSPA korumalı yürütme: S+{n} sonlu {low}..{high} aralığında olmalı; -273°C tekilliği henüz doğrulanmadı"
                ));
            }
        }
        Ok(())
    }
    pub fn execute(&self, m: &mut Memory) {
        // 42CFC3 sets this scratch before invoking the mathematical core.
        m.native_numeric_scratch = Some(self.source.index as i16);
        if let Err(reason) = self.validate_installed_execution(m) {
            m.execution_limitations.insert(reason);
            return;
        }
        // All actual source reads precede every destination store in 42C8DC.
        // Reserved S+26..207 are neither read nor written by the installed core.
        let input = std::array::from_fn(|n| {
            m.word(Device {
                area: Area::D,
                index: self.source.index + n as u16,
            })
        });
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let output = installed::position(input);
            for (n, value) in output.into_iter().enumerate() {
                m.set_word(
                    Device {
                        area: Area::D,
                        index: self.dest.index + n as u16,
                    },
                    value as i16,
                );
            }
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let _: [i16; 26] = input;
            m.execution_limitations.insert(
                "DSPA: kurulu simülatörün x87 sayısal çekirdeği bu mimaride desteklenmiyor".into(),
            );
        }
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod installed {
    use std::ops::{Add, Div, Mul, Neg, Sub};

    // Every recovered register-only intermediate has a 64-bit significand. Explicit
    // f64()/f32() calls below reproduce the original FSTP qword/dword boundaries.
    #[derive(Clone, Copy)]
    pub(super) struct Ext(pub(super) [u8; 10]);
    impl Ext {
        pub(super) fn f64(self) -> f64 {
            let mut out = 0.0;
            unsafe {
                std::arch::asm!("fld tbyte ptr [{a}]", "fstp qword ptr [{o}]", a=in(reg) self.0.as_ptr(),o=in(reg) &mut out,options(nostack,preserves_flags));
            }
            out
        }
        pub(super) fn f32(self) -> f32 {
            let mut out = 0.0;
            unsafe {
                std::arch::asm!("fld tbyte ptr [{a}]", "fstp dword ptr [{o}]", a=in(reg) self.0.as_ptr(),o=in(reg) &mut out,options(nostack,preserves_flags));
            }
            out
        }
        fn rounded(self) -> Self {
            Self::from(self.f64())
        }
        pub(super) fn trunc_i64(self) -> i64 {
            let mut out = 0i64;
            unsafe {
                std::arch::asm!("fld tbyte ptr [{a}]", "fisttp qword ptr [{o}]", a=in(reg) self.0.as_ptr(),o=in(reg) &mut out,options(nostack,preserves_flags));
            }
            out
        }
        fn pi() -> Self {
            let mut out = Self([0; 10]);
            unsafe {
                std::arch::asm!("fldpi", "fstp tbyte ptr [{o}]", o=in(reg) out.0.as_mut_ptr(),options(nostack,preserves_flags));
            }
            out
        }
        fn abs(self) -> Self {
            let mut out = self;
            out.0[9] &= 0x7f;
            out
        }
        fn magnitude_cmp(self, other: Self) -> std::cmp::Ordering {
            let key = |v: Self| {
                (
                    u16::from_le_bytes(v.0[8..10].try_into().unwrap()) & 0x7fff,
                    u64::from_le_bytes(v.0[..8].try_into().unwrap()),
                )
            };
            key(self).cmp(&key(other))
        }
        pub(super) fn atan(self) -> Self {
            // RTL518680 first reduces |x| to <=1; atan2 calls this with the
            // extended quotient, whereas standalone atan accepts a rounded double.
            let a = self.abs();
            let angle = if a.magnitude_cmp(e(1.0)).is_eq() {
                Self::pi() * e(0.25)
            } else if a.magnitude_cmp(e(1.0)).is_gt() {
                Self::pi() * e(0.5) - fpatan(e(1.0), a)
            } else {
                fpatan(a, e(1.0))
            };
            if self.0[9] & 0x80 != 0 { -angle } else { angle }
        }
        fn pow(self, n: usize) -> Self {
            // RTL518E16..518E39 uses left-to-right repeated squaring for these
            // exact small nonnegative integer exponents (0..5).
            if n == 0 {
                return e(1.0);
            }
            let mut value = self;
            for bit in (0..(usize::BITS - 1 - n.leading_zeros())).rev() {
                value = value * value;
                if n & (1usize << bit) != 0 {
                    value = value * self;
                }
            }
            value
        }
    }
    impl From<f64> for Ext {
        fn from(a: f64) -> Self {
            let mut out = Self([0; 10]);
            unsafe {
                std::arch::asm!("fld qword ptr [{a}]", "fstp tbyte ptr [{o}]",a=in(reg) &a,o=in(reg) out.0.as_mut_ptr(),options(nostack,preserves_flags));
            }
            out
        }
    }
    macro_rules! binary {
    ($trait:ident,$method:ident,$op:literal)=>{
        impl $trait for Ext { type Output=Self; fn $method(self,rhs:Self)->Self {
            let mut out=Self([0;10]);
            unsafe { std::arch::asm!("fld tbyte ptr [{a}]", "fld tbyte ptr [{b}]", $op,"fstp tbyte ptr [{o}]",a=in(reg) self.0.as_ptr(),b=in(reg) rhs.0.as_ptr(),o=in(reg) out.0.as_mut_ptr(),options(nostack,preserves_flags)); }
            out
        }}
    }
}
    binary!(Add, add, "faddp st(1), st(0)");
    binary!(Sub, sub, "fsubp st(1), st(0)");
    binary!(Mul, mul, "fmulp st(1), st(0)");
    binary!(Div, div, "fdivp st(1), st(0)");
    impl Neg for Ext {
        type Output = Self;
        fn neg(mut self) -> Self {
            self.0[9] ^= 0x80;
            self
        }
    }
    pub(super) fn e(a: f64) -> Ext {
        Ext::from(a)
    }
    macro_rules! unary {
    ($name:ident,$($op:literal),+)=>{ pub(super) fn $name(a:f64)->Ext {
        let mut out=Ext([0;10]);
        unsafe { std::arch::asm!("fld qword ptr [{a}]",$($op,)+"fstp tbyte ptr [{o}]",a=in(reg) &a,o=in(reg) out.0.as_mut_ptr(),options(nostack,preserves_flags)); }
        out
    }}
}
    unary!(sin, "fsin");
    unary!(cos, "fcos");
    unary!(tan, "fptan", "fstp st(0)");
    fn fpatan(y: Ext, x: Ext) -> Ext {
        let mut out = Ext([0; 10]);
        unsafe {
            std::arch::asm!("fld tbyte ptr [{y}]","fld tbyte ptr [{x}]","fpatan","fstp tbyte ptr [{o}]",y=in(reg) y.0.as_ptr(),x=in(reg) x.0.as_ptr(),o=in(reg) out.0.as_mut_ptr(),options(nostack,preserves_flags));
        }
        out
    }
    fn atan2(y: f64, x: f64) -> Ext {
        // The installed Delphi atan2 divides first, then invokes its reduced atan.
        if y == 0.0 {
            return if x.is_sign_negative() {
                if y.is_sign_negative() {
                    -Ext::pi()
                } else {
                    Ext::pi()
                }
            } else {
                e(y)
            };
        }
        if x == 0.0 {
            return if y.is_sign_negative() {
                -Ext::pi() * e(0.5)
            } else {
                Ext::pi() * e(0.5)
            };
        }
        let result = (e(y) / e(x)).atan();
        if x.is_sign_negative() {
            if y.is_sign_negative() {
                result - Ext::pi()
            } else {
                result + Ext::pi()
            }
        } else {
            result
        }
    }
    pub(super) fn inverse(a: f64, acos: bool) -> Ext {
        let magnitude = a.abs();
        let mut complement = acos;
        let mut out = Ext([0; 10]);
        unsafe {
            std::arch::asm!("fld qword ptr [{a}]","fld1","fld st(1)","fmul st(0), st(0)","fsubp st(1), st(0)","fsqrt","fstp tbyte ptr [{o}]","fstp st(0)",a=in(reg) &magnitude,o=in(reg) out.0.as_mut_ptr(),options(nostack,preserves_flags));
        }
        let root = out;
        let mut angle = if root.magnitude_cmp(e(magnitude)).is_eq() {
            Ext::pi() * e(0.25)
        } else {
            let ratio = if root.magnitude_cmp(e(magnitude)).is_lt() {
                complement = !complement;
                fpatan(root, e(magnitude))
            } else {
                fpatan(e(magnitude), root)
            };
            if complement {
                Ext::pi() * e(0.5) - ratio
            } else {
                ratio
            }
        };
        if a.is_sign_negative() {
            angle = -angle;
            if acos {
                angle = angle + Ext::pi();
            }
        }
        angle
    }
    pub(super) struct Control(u16);
    impl Control {
        pub(super) fn enter() -> Self {
            let mut saved = 0u16;
            unsafe {
                std::arch::asm!("fnstcw word ptr [{w}]",w=in(reg) &mut saved,options(nostack,preserves_flags));
            }
            let scoped = (saved & !0x0f00) | 0x0300;
            unsafe {
                std::arch::asm!("fldcw word ptr [{w}]",w=in(reg) &scoped,options(nostack,preserves_flags));
            }
            Self(saved)
        }
    }
    impl Drop for Control {
        fn drop(&mut self) {
            unsafe {
                std::arch::asm!("fldcw word ptr [{w}]",w=in(reg) &self.0,options(nostack,preserves_flags));
            }
        }
    }

    fn radians(a: f64) -> Ext {
        literal(0x42B85C) * e(a)
    }
    fn degrees(a: f64) -> Ext {
        literal(0x42B844) * e(a)
    }
    fn normalized(a: f64) -> Ext {
        let turns = (literal(0x42B8B4) * e(a)).f64();
        let mut angle = ((e(turns) - e(turns.floor())) * e(360.0)).f64();
        if angle < 0.0 {
            angle = (e(360.0) + e(angle)).f64();
        }
        e(angle)
    }

    fn julian(date: [i16; 6], longitude: f32) -> f64 {
        let [year, month, day, hour, minute, second] = date;
        let zone = (e(longitude as f64) * literal(0x42CF58)).f32();
        // 42B8F5..42B91F groups day + ((hour-zone + (minute+second/60)/60)/24).
        let fraction = (e(day as f64)
            + (e(hour as f64) - e(zone as f64)
                + (e(minute as f64) + e(second as f64) * literal(0x42B9DC)) * literal(0x42B9DC))
                * literal(0x42B9E8))
        .f64();
        let (year, month) = if month < 3 {
            (year as i32 - 1, month as i32 + 12)
        } else {
            (year as i32, month as i32)
        };
        let first = ((e(year as f64) + literal(0x42B9F4)) * literal(0x42B9F8))
            .f64()
            .floor();
        let second = (e((month + 1) as f64) * literal(0x42B9FC)).f64().floor();
        let mut jd = (e(first) + e(second) + e(fraction) - literal(0x42BA04)).f64();
        if jd > literal(0x42BA08).f64() {
            let century = (year / 100) as f64;
            let quarter = (e(century) * literal(0x42BA0C)).f64().floor();
            jd = (literal(0x42BA10) - e(century) + e(quarter) + e(jd)).f64();
        }
        jd
    }

    fn series(terms: &[[f64; 3]], jme: f64) -> f64 {
        let mut sum = 0.0;
        for &[a, b, c] in terms {
            sum = (cos((e(c) * e(jme) + e(b)).f64()) * e(a) + e(sum)).f64();
        }
        sum
    }
    fn polynomial_series(terms: &[&[[f64; 3]]], jme: f64) -> f64 {
        let mut sum = 0.0;
        for (n, table) in terms.iter().enumerate() {
            sum = (e(jme).pow(n) * e(series(table, jme)) + e(sum)).f64();
        }
        (literal(0x42BB50) * e(sum)).f64()
    }
    fn fundamental(c: [f64; 4], jce: f64) -> f64 {
        (((e(c[0]) * e(jce) + e(c[1])) * e(jce) + e(c[2])) * e(jce) + e(c[3])).f64()
    }
    fn nutation(jce: f64, x: [f64; 5]) -> (f64, f64) {
        let (mut psi, mut epsilon) = (0.0, 0.0);
        for n in 0..63 {
            let mut argument = 0.0;
            for (&coefficient, &value) in Y[n].iter().zip(x.iter()) {
                argument = (e(coefficient as f64) * e(value) + e(argument)).f64();
            }
            let argument = radians(argument).f64();
            let [a, b, c, d] = PE[n];
            psi = ((e(jce) * e(b) + e(a)) * sin(argument) + e(psi)).f64();
            epsilon = ((e(jce) * e(d) + e(c)) * cos(argument) + e(epsilon)).f64();
        }
        (
            (literal(0x42BFC8) * e(psi)).f64(),
            (literal(0x42BFC8) * e(epsilon)).f64(),
        )
    }
    fn obliquity(jme: f64) -> f64 {
        let u = (literal(0x42C04C) * e(jme)).f64();
        let mut value = e(u) * literal(0x42C058) + literal(0x42C060);
        for address in [
            0x42C068, 0x42C070, 0x42C078, 0x42C080, 0x42C088, 0x42C090, 0x42C094, 0x42C09C,
            0x42C0A4,
        ] {
            value = value * e(u) + literal(address);
        }
        value.f64()
    }
    fn parallax(latitude: f64, elevation: f64, xi: f64, h: f64, delta: f64) -> (f64, f64) {
        let lat = radians(latitude).f64();
        let xi = radians(xi).f64();
        let h = radians(h).f64();
        let delta = radians(delta).f64();
        let u = (tan(lat) * literal(0x42C57C)).rounded().atan().f64();
        let y = (sin(u) * literal(0x42C57C) + sin(lat) * e(elevation) * literal(0x42C584)).f64();
        let x = (cos(u) + cos(lat) * e(elevation) * literal(0x42C584)).f64();
        let alpha = atan2(
            ((-e(x) * sin(xi)) * sin(h)).f64(),
            (cos(delta) - (sin(xi) * e(x)) * cos(h)).f64(),
        )
        .f64();
        let prime = degrees(
            atan2(
                ((sin(delta) - sin(xi) * e(y)) * cos(alpha)).f64(),
                (cos(delta) - (sin(xi) * e(x)) * cos(h)).f64(),
            )
            .f64(),
        )
        .f64();
        (degrees(alpha).f64(), prime)
    }

    pub(super) fn position(input: [i16; 26]) -> [u16; 8] {
        let _cw = Control::enter();
        let f = |n: usize| {
            f32::from_bits(u32::from(input[n] as u16) | (u32::from(input[n + 1] as u16) << 16))
        };
        let jd = julian(input[..6].try_into().unwrap(), f(10));
        let jc = ((e(jd) - literal(0x42BA2C)) * literal(0x42BA30)).f64();
        let jde = (literal(0x42BA50) * e(f(6) as f64) + e(jd)).f64();
        let jce = ((e(jde) - literal(0x42BA74)) * literal(0x42BA78)).f64();
        let jme = (literal(0x42BA94) * e(jce)).f64();
        let l = normalized(degrees(polynomial_series(&[L0, L1, L2, L3, L4, L5], jme)).f64()).f64();
        let b0 = series(&B[..5], jme);
        let b1 = series(&B[5..], jme);
        let b = degrees((literal(0x42BD00) * e((e(b1) * e(jme) + e(b0)).f64())).f64()).f64();
        let r = polynomial_series(&[R0, R1, R2, R3, R4], jme);
        let mut theta = (e(l) + e(180.0)).f64();
        if theta >= 360.0 {
            theta = (e(theta) - e(360.0)).f64();
        }
        let beta = -b;
        let fundamentals = FUNDAMENTAL.map(|c| fundamental(c, jce));
        let (psi, delta_epsilon) = nutation(jce, fundamentals);
        let epsilon = (literal(0x42C0C0) * e(obliquity(jme)) + e(delta_epsilon)).f64();
        let aberration = (literal(0x42C0E4) / (literal(0x42C0E0) * e(r))).f64();
        let lambda = (e(theta) + e(psi) + e(aberration)).f64();
        let nu0 = normalized(
            ((e(jd) - literal(0x42C140)) * literal(0x42C144)
                + literal(0x42C14C)
                + (literal(0x42C160) - literal(0x42C154) * e(jc)) * (e(jc) * e(jc)))
            .f64(),
        )
        .f64();
        let nu = (cos(radians(epsilon).f64()) * e(psi) + e(nu0)).f64();
        let lr = radians(lambda).f64();
        let er = radians(epsilon).f64();
        let br = radians(beta).f64();
        let alpha = normalized(
            degrees(atan2((sin(lr) * cos(er) - tan(br) * sin(er)).f64(), cos(lr).f64()).f64())
                .f64(),
        )
        .f64();
        let delta = degrees(
            inverse(
                (sin(br) * cos(er) + (cos(br) * sin(er)) * sin(lr)).f64(),
                false,
            )
            .f64(),
        )
        .f64();
        let h = normalized((e(nu) + e(f(10) as f64) - e(alpha)).f64()).f64();
        let xi = (literal(0x42C348) / (literal(0x42C344) * e(r))).f64();
        let (delta_alpha, delta_prime) = parallax(f(12) as f64, f(14) as f64, xi, h, delta);
        let hp = (e(h) - e(delta_alpha)).f64();
        let lat = radians(f(12) as f64).f64();
        let dp = radians(delta_prime).f64();
        let hr = radians(hp).f64();
        let elevation = degrees(
            inverse(
                (sin(lat) * sin(dp) + (cos(lat) * cos(dp)) * cos(hr)).f64(),
                false,
            )
            .f64(),
        )
        .f64();
        let refracted = if -literal(0x42C6EC).f64() <= elevation {
            let angle = radians(
                (literal(0x42C700) / (e(elevation) + literal(0x42C6F8)) + e(elevation)).f64(),
            )
            .f64();
            let numerator = (literal(0x42C710) / (literal(0x42C70C) + e(f(18) as f64)))
                * (literal(0x42C714) * e(f(16) as f64))
                * literal(0x42C720);
            (numerator / (tan(angle) * literal(0x42C708))).f64()
        } else {
            0.0
        };
        let zenith = (literal(0x42C744) - e((e(elevation) + e(refracted)).f64())).f64();
        let astro = degrees(
            atan2(
                sin(hr).f64(),
                (cos(hr) * sin(lat) - tan(dp) * cos(lat)).f64(),
            )
            .f64(),
        )
        .f64();
        let azimuth = (e(astro) + literal(0x42C814)).f64();
        let zr = radians(zenith).f64();
        let sr = radians(f(20) as f64).f64();
        let diff = radians((e(astro) - e(f(22) as f64)).f64()).f64();
        let incidence = degrees(
            inverse(
                (cos(zr) * cos(sr) + (sin(sr) * sin(zr)) * cos(diff)).f64(),
                true,
            )
            .f64(),
        )
        .f64();
        let mut out = [0u16; 8];
        for (n, value) in [zenith, azimuth, incidence].into_iter().enumerate() {
            let bits = e(value).f32().to_bits();
            out[n * 2] = bits as u16;
            out[n * 2 + 1] = (bits >> 16) as u16;
        }
        out[6] = (literal(0x42CF64) * e(zenith) * literal(0x42CF70)).trunc_i64() as u16;
        out[7] = (literal(0x42CF7C) * e(azimuth) * literal(0x42CF70)).trunc_i64() as u16;
        out
    }

    // Exact coefficient bytes from ES2 SHA256 1f8974f13caad54334b5ad4c0402d8c1026a4dec29a7f61c19136e76df028515.
    const L0: &[[f64; 3]] = &[
        [
            f64::from_bits(0x41a4e72acc000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x41497eac00000000),
            f64::from_bits(0x4012ad51a79280a1),
            f64::from_bits(0x40b88b136ae7d567),
        ],
        [
            f64::from_bits(0x40e109c000000000),
            f64::from_bits(0x401281205bc01a37),
            f64::from_bits(0x40c88b136ae7d567),
        ],
        [
            f64::from_bits(0x40ab520000000000),
            f64::from_bits(0x4005f3eab367a0f9),
            f64::from_bits(0x40b6796288ce703b),
        ],
        [
            f64::from_bits(0x40aab40000000000),
            f64::from_bits(0x4006a19652bd3c36),
            f64::from_bits(0x400c2f4f0d844d01),
        ],
        [
            f64::from_bits(0x40a8800000000000),
            f64::from_bits(0x400d058793dd97f6),
            f64::from_bits(0x40f2f91c5810624e),
        ],
        [
            f64::from_bits(0x40a4e80000000000),
            f64::from_bits(0x4011ac226809d495),
            f64::from_bits(0x40beb46b5dcc63f1),
        ],
        [
            f64::from_bits(0x40a24e0000000000),
            f64::from_bits(0x40188a71de69ad43),
            f64::from_bits(0x40aeb46b5dcc63f1),
        ],
        [
            f64::from_bits(0x4094b00000000000),
            f64::from_bits(0x3fe7c28f5c28f5c3),
            f64::from_bits(0x40c6796288ce703b),
        ],
        [
            f64::from_bits(0x4093e40000000000),
            f64::from_bits(0x40004bfb15b573eb),
            f64::from_bits(0x40808d872b020c4a),
        ],
        [
            f64::from_bits(0x4092bc0000000000),
            f64::from_bits(0x3ff1c0ebedfa43fe),
            f64::from_bits(0x4098a55fbe76c8b4),
        ],
        [
            f64::from_bits(0x408ef00000000000),
            f64::from_bits(0x4014ee978d4fdf3b),
            f64::from_bits(0x40b6fced4fdf3b64),
        ],
        [
            f64::from_bits(0x408c300000000000),
            f64::from_bits(0x40005c28f5c28f5c),
            f64::from_bits(0x403a4c49ba5e353f),
        ],
        [
            f64::from_bits(0x408ac80000000000),
            f64::from_bits(0x400c10624dd2f1aa),
            f64::from_bits(0x4078e2624dd2f1aa),
        ],
        [
            f64::from_bits(0x4088600000000000),
            f64::from_bits(0x3ff2dd2f1a9fbe77),
            f64::from_bits(0x40b467b1a9fbe76d),
        ],
        [
            f64::from_bits(0x4087880000000000),
            f64::from_bits(0x40044395810624dd),
            f64::from_bits(0x40b5838d916872b0),
        ],
        [
            f64::from_bits(0x407f900000000000),
            f64::from_bits(0x401254fdf3b645a2),
            f64::from_bits(0x40d2684e978d4fdf),
        ],
        [
            f64::from_bits(0x407ec00000000000),
            f64::from_bits(0x4010d1eb851eb852),
            f64::from_bits(0x40883c2f1a9fbe77),
        ],
        [
            f64::from_bits(0x4076500000000000),
            f64::from_bits(0x40075c28f5c28f5c),
            f64::from_bits(0x3fb126e978d4fdf4),
        ],
        [
            f64::from_bits(0x4073d00000000000),
            f64::from_bits(0x401765604189374c),
            f64::from_bits(0x40c7075083126e98),
        ],
        [
            f64::from_bits(0x4071c00000000000),
            f64::from_bits(0x3ffe624dd2f1a9fc),
            f64::from_bits(0x4088e2624dd2f1aa),
        ],
        [
            f64::from_bits(0x4070f00000000000),
            f64::from_bits(0x3fd428f5c28f5c29),
            f64::from_bits(0x40c5708a1cac0831),
        ],
        [
            f64::from_bits(0x406e600000000000),
            f64::from_bits(0x3fd6147ae147ae14),
            f64::from_bits(0x40b56ec72b020c4a),
        ],
        [
            f64::from_bits(0x4069c00000000000),
            f64::from_bits(0x4013395810624dd3),
            f64::from_bits(0x40a3e0a0c49ba5e3),
        ],
        [
            f64::from_bits(0x4069a00000000000),
            f64::from_bits(0x3ffde76c8b439581),
            f64::from_bits(0x40b5c5249ba5e354),
        ],
        [
            f64::from_bits(0x4069400000000000),
            f64::from_bits(0x4003a9fbe76c8b44),
            f64::from_bits(0x40b7b5c6e978d4fe),
        ],
        [
            f64::from_bits(0x4063800000000000),
            f64::from_bits(0x3feaa7ef9db22d0e),
            f64::from_bits(0x406aa9916872b021),
        ],
        [
            f64::from_bits(0x4060800000000000),
            f64::from_bits(0x400b49ba5e353f7d),
            f64::from_bits(0x40a6fced0e560419),
        ],
        [
            f64::from_bits(0x405f800000000000),
            f64::from_bits(0x3ff153f7ced91687),
            f64::from_bits(0x4034c66666666666),
        ],
        [
            f64::from_bits(0x405cc00000000000),
            f64::from_bits(0x3fe4a3d70a3d70a4),
            f64::from_bits(0x3fef5c28f5c28f5c),
        ],
        [
            f64::from_bits(0x4059c00000000000),
            f64::from_bits(0x3fe45a1cac083127),
            f64::from_bits(0x40b25600c49ba5e3),
        ],
        [
            f64::from_bits(0x4059800000000000),
            f64::from_bits(0x3fef3b645a1cac08),
            f64::from_bits(0x40ceb46b645a1cac),
        ],
        [
            f64::from_bits(0x4059800000000000),
            f64::from_bits(0x4011116872b020c5),
            f64::from_bits(0x401c74bc6a7ef9db),
        ],
        [
            f64::from_bits(0x4058c00000000000),
            f64::from_bits(0x4018d70a3d70a3d7),
            f64::from_bits(0x40a0c4570a3d70a4),
        ],
        [
            f64::from_bits(0x4058800000000000),
            f64::from_bits(0x3fe5c28f5c28f5c3),
            f64::from_bits(0x40636d70a3d70a3d),
        ],
        [
            f64::from_bits(0x4055800000000000),
            f64::from_bits(0x4017eb851eb851ec),
            f64::from_bits(0x4103a745851eb852),
        ],
        [
            f64::from_bits(0x4055400000000000),
            f64::from_bits(0x3ff4cccccccccccd),
            f64::from_bits(0x40b883f5c28f5c29),
        ],
        [
            f64::from_bits(0x4055400000000000),
            f64::from_bits(0x400d5c28f5c28f5c),
            f64::from_bits(0x40f1706b33333333),
        ],
        [
            f64::from_bits(0x4054000000000000),
            f64::from_bits(0x3ffcf5c28f5c28f6),
            f64::from_bits(0x40d0db099999999a),
        ],
        [
            f64::from_bits(0x4053c00000000000),
            f64::from_bits(0x400851eb851eb852),
            f64::from_bits(0x40c7823ae147ae14),
        ],
        [
            f64::from_bits(0x4052c00000000000),
            f64::from_bits(0x3ffc28f5c28f5c29),
            f64::from_bits(0x40b3e0a147ae147b),
        ],
        [
            f64::from_bits(0x4052800000000000),
            f64::from_bits(0x400c000000000000),
            f64::from_bits(0x40a8a56147ae147b),
        ],
        [
            f64::from_bits(0x4052800000000000),
            f64::from_bits(0x4012b851eb851eb8),
            f64::from_bits(0x40890e8f5c28f5c3),
        ],
        [
            f64::from_bits(0x4051800000000000),
            f64::from_bits(0x3fea8f5c28f5c28f),
            f64::from_bits(0x40c26ee147ae147b),
        ],
        [
            f64::from_bits(0x404f000000000000),
            f64::from_bits(0x400fd70a3d70a3d7),
            f64::from_bits(0x40c13db1eb851eb8),
        ],
        [
            f64::from_bits(0x404e800000000000),
            f64::from_bits(0x3ffd1eb851eb851f),
            f64::from_bits(0x40bbace666666666),
        ],
        [
            f64::from_bits(0x404c800000000000),
            f64::from_bits(0x40063d70a3d70a3d),
            f64::from_bits(0x40b88e999999999a),
        ],
        [
            f64::from_bits(0x404c000000000000),
            f64::from_bits(0x40118f5c28f5c28f),
            f64::from_bits(0x40cb9fc000000000),
        ],
        [
            f64::from_bits(0x404c000000000000),
            f64::from_bits(0x400bc28f5c28f5c3),
            f64::from_bits(0x40b8878ccccccccd),
        ],
        [
            f64::from_bits(0x404a000000000000),
            f64::from_bits(0x3fc851eb851eb852),
            f64::from_bits(0x40c7b5c666666666),
        ],
        [
            f64::from_bits(0x404a000000000000),
            f64::from_bits(0x3ff547ae147ae148),
            f64::from_bits(0x409b50147ae147ae),
        ],
        [
            f64::from_bits(0x4049800000000000),
            f64::from_bits(0x3fd1eb851eb851ec),
            f64::from_bits(0x40b6e07ae147ae14),
        ],
        [
            f64::from_bits(0x4048800000000000),
            f64::from_bits(0x3fdf5c28f5c28f5c),
            f64::from_bits(0x4092a9cccccccccd),
        ],
        [
            f64::from_bits(0x4044800000000000),
            f64::from_bits(0x40157ae147ae147b),
            f64::from_bits(0x40c0769eb851eb85),
        ],
        [
            f64::from_bits(0x4044800000000000),
            f64::from_bits(0x4003333333333333),
            f64::from_bits(0x40d330c333333333),
        ],
        [
            f64::from_bits(0x4043800000000000),
            f64::from_bits(0x4018ae147ae147ae),
            f64::from_bits(0x40c467b1eb851eb8),
        ],
        [
            f64::from_bits(0x4042800000000000),
            f64::from_bits(0x401828f5c28f5c29),
            f64::from_bits(0x40c3f2a51eb851ec),
        ],
        [
            f64::from_bits(0x4042800000000000),
            f64::from_bits(0x40048f5c28f5c28f),
            f64::from_bits(0x40908d851eb851ec),
        ],
        [
            f64::from_bits(0x4042000000000000),
            f64::from_bits(0x3ffb5c28f5c28f5c),
            f64::from_bits(0x40a261bd70a3d70a),
        ],
        [
            f64::from_bits(0x4042000000000000),
            f64::from_bits(0x3ffc7ae147ae147b),
            f64::from_bits(0x40ba9cc51eb851ec),
        ],
        [
            f64::from_bits(0x4040800000000000),
            f64::from_bits(0x3fe2e147ae147ae1),
            f64::from_bits(0x40d15f7666666666),
        ],
        [
            f64::from_bits(0x403e000000000000),
            f64::from_bits(0x3fdc28f5c28f5c29),
            f64::from_bits(0x40f481cd9999999a),
        ],
        [
            f64::from_bits(0x403e000000000000),
            f64::from_bits(0x4005eb851eb851ec),
            f64::from_bits(0x4095177ae147ae14),
        ],
        [
            f64::from_bits(0x4039000000000000),
            f64::from_bits(0x400947ae147ae148),
            f64::from_bits(0x40b2527ae147ae14),
        ],
    ];
    const L1: &[[f64; 3]] = &[
        [
            f64::from_bits(0x426249701e236000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4109275800000000),
            f64::from_bits(0x40056d0678c0053e),
            f64::from_bits(0x40b88b136ae7d567),
        ],
        [
            f64::from_bits(0x40b0cf0000000000),
            f64::from_bits(0x400514af4f0d844d),
            f64::from_bits(0x40c88b136ae7d567),
        ],
        [
            f64::from_bits(0x407a900000000000),
            f64::from_bits(0x3ff970a3d70a3d71),
            f64::from_bits(0x400c2f1a9fbe76c9),
        ],
        [
            f64::from_bits(0x405dc00000000000),
            f64::from_bits(0x40172f1a9fbe76c9),
            f64::from_bits(0x403a4c49ba5e353f),
        ],
        [
            f64::from_bits(0x405b400000000000),
            f64::from_bits(0x4007ba5e353f7cee),
            f64::from_bits(0x4098a5604189374c),
        ],
        [
            f64::from_bits(0x4057400000000000),
            f64::from_bits(0x4004b851eb851eb8),
            f64::from_bits(0x40d2684eb851eb85),
        ],
        [
            f64::from_bits(0x4052000000000000),
            f64::from_bits(0x3ff23d70a3d70a3d),
            f64::from_bits(0x40808d851eb851ec),
        ],
        [
            f64::from_bits(0x4051000000000000),
            f64::from_bits(0x3ffdeb851eb851ec),
            f64::from_bits(0x4078e26666666666),
        ],
        [
            f64::from_bits(0x4050c00000000000),
            f64::from_bits(0x4011a3d70a3d70a4),
            f64::from_bits(0x40b5838ccccccccd),
        ],
        [
            f64::from_bits(0x404d800000000000),
            f64::from_bits(0x40071eb851eb851f),
            f64::from_bits(0x40b467b0a3d70a3d),
        ],
        [
            f64::from_bits(0x404c000000000000),
            f64::from_bits(0x40015c28f5c28f5c),
            f64::from_bits(0x40636d70a3d70a3d),
        ],
        [
            f64::from_bits(0x4046800000000000),
            f64::from_bits(0x3fd999999999999a),
            f64::from_bits(0x4088e26666666666),
        ],
        [
            f64::from_bits(0x4042000000000000),
            f64::from_bits(0x3fde147ae147ae14),
            f64::from_bits(0x40883c28f5c28f5c),
        ],
        [
            f64::from_bits(0x403d000000000000),
            f64::from_bits(0x4005333333333333),
            f64::from_bits(0x401c70a3d70a3d71),
        ],
        [
            f64::from_bits(0x4035000000000000),
            f64::from_bits(0x40155c28f5c28f5c),
            f64::from_bits(0x3fef5c28f5c28f5c),
        ],
        [
            f64::from_bits(0x4033000000000000),
            f64::from_bits(0x3ffd99999999999a),
            f64::from_bits(0x40b56ec7ae147ae1),
        ],
        [
            f64::from_bits(0x4033000000000000),
            f64::from_bits(0x4013e147ae147ae1),
            f64::from_bits(0x406aa9999999999a),
        ],
        [
            f64::from_bits(0x4031000000000000),
            f64::from_bits(0x4007eb851eb851ec),
            f64::from_bits(0x40b883f5c28f5c29),
        ],
        [
            f64::from_bits(0x4030000000000000),
            f64::from_bits(0x3f9eb851eb851eb8),
            f64::from_bits(0x40a3e09eb851eb85),
        ],
        [
            f64::from_bits(0x4030000000000000),
            f64::from_bits(0x3ff6e147ae147ae1),
            f64::from_bits(0x40a0c4570a3d70a4),
        ],
        [
            f64::from_bits(0x402e000000000000),
            f64::from_bits(0x3ff35c28f5c28f5c),
            f64::from_bits(0x40c5708a3d70a3d7),
        ],
        [
            f64::from_bits(0x4028000000000000),
            f64::from_bits(0x4006a3d70a3d70a4),
            f64::from_bits(0x409b50147ae147ae),
        ],
        [
            f64::from_bits(0x4028000000000000),
            f64::from_bits(0x400a147ae147ae14),
            f64::from_bits(0x40b3e0a147ae147b),
        ],
        [
            f64::from_bits(0x4028000000000000),
            f64::from_bits(0x4015147ae147ae14),
            f64::from_bits(0x4092a9cccccccccd),
        ],
        [
            f64::from_bits(0x4028000000000000),
            f64::from_bits(0x4000a3d70a3d70a4),
            f64::from_bits(0x40b2560000000000),
        ],
        [
            f64::from_bits(0x4026000000000000),
            f64::from_bits(0x3fe8a3d70a3d70a4),
            f64::from_bits(0x40814c8f5c28f5c3),
        ],
        [
            f64::from_bits(0x4024000000000000),
            f64::from_bits(0x3ff4cccccccccccd),
            f64::from_bits(0x40b88e999999999a),
        ],
        [
            f64::from_bits(0x4024000000000000),
            f64::from_bits(0x4010f5c28f5c28f6),
            f64::from_bits(0x4095177ae147ae14),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x400599999999999a),
            f64::from_bits(0x406e575c28f5c28f),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x40168f5c28f5c28f),
            f64::from_bits(0x408dbdc28f5c28f6),
        ],
        [
            f64::from_bits(0x4020000000000000),
            f64::from_bits(0x4015333333333333),
            f64::from_bits(0x40a261bd70a3d70a),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x4005333333333333),
            f64::from_bits(0x40c26ee147ae147b),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x4012ae147ae147ae),
            f64::from_bits(0x40b2527ae147ae14),
        ],
    ];
    const L2: &[[f64; 3]] = &[
        [
            f64::from_bits(0x40e9d6e000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x40c1080000000000),
            f64::from_bits(0x3ff127525460aa65),
            f64::from_bits(0x40b88b1367a0f909),
        ],
        [
            f64::from_bits(0x4073500000000000),
            f64::from_bits(0x3febbe76c8b43958),
            f64::from_bits(0x40c88b1374bc6a7f),
        ],
        [
            f64::from_bits(0x403b000000000000),
            f64::from_bits(0x3fa999999999999a),
            f64::from_bits(0x400c28f5c28f5c29),
        ],
        [
            f64::from_bits(0x4030000000000000),
            f64::from_bits(0x4014c28f5c28f5c3),
            f64::from_bits(0x403a4ccccccccccd),
        ],
        [
            f64::from_bits(0x4030000000000000),
            f64::from_bits(0x400d70a3d70a3d71),
            f64::from_bits(0x40636d70a3d70a3d),
        ],
        [
            f64::from_bits(0x4024000000000000),
            f64::from_bits(0x3fe851eb851eb852),
            f64::from_bits(0x40d2684eb851eb85),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x40007ae147ae147b),
            f64::from_bits(0x40f2f91c51eb851f),
        ],
        [
            f64::from_bits(0x401c000000000000),
            f64::from_bits(0x3fea8f5c28f5c28f),
            f64::from_bits(0x40883c28f5c28f5c),
        ],
        [
            f64::from_bits(0x4014000000000000),
            f64::from_bits(0x4012a3d70a3d70a4),
            f64::from_bits(0x4098a55c28f5c28f),
        ],
        [
            f64::from_bits(0x4010000000000000),
            f64::from_bits(0x3ff07ae147ae147b),
            f64::from_bits(0x401c70a3d70a3d71),
        ],
        [
            f64::from_bits(0x4010000000000000),
            f64::from_bits(0x400b851eb851eb85),
            f64::from_bits(0x40b5c523d70a3d71),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x40148f5c28f5c28f),
            f64::from_bits(0x4088e26666666666),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x4018333333333333),
            f64::from_bits(0x40b5838ccccccccd),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x3ff30a3d70a3d70a),
            f64::from_bits(0x406e575c28f5c28f),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x40187ae147ae147b),
            f64::from_bits(0x40808d851eb851ec),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x3fd3d70a3d70a3d7),
            f64::from_bits(0x4078e26666666666),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x40023d70a3d70a3d),
            f64::from_bits(0x40814c8f5c28f5c3),
        ],
        [
            f64::from_bits(0x4000000000000000),
            f64::from_bits(0x4011851eb851eb85),
            f64::from_bits(0x40b467b0a3d70a3d),
        ],
        [
            f64::from_bits(0x4000000000000000),
            f64::from_bits(0x400e000000000000),
            f64::from_bits(0x3fef5c28f5c28f5c),
        ],
    ];
    const L3: &[[f64; 3]] = &[
        [
            f64::from_bits(0x4072100000000000),
            f64::from_bits(0x4017604189374bc7),
            f64::from_bits(0x40b88b1374bc6a7f),
        ],
        [
            f64::from_bits(0x4041800000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4031000000000000),
            f64::from_bits(0x4015f5c28f5c28f6),
            f64::from_bits(0x40c88b1333333333),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x4014cccccccccccd),
            f64::from_bits(0x40636d70a3d70a3d),
        ],
        [
            f64::from_bits(0x3ff0000000000000),
            f64::from_bits(0x4012e147ae147ae1),
            f64::from_bits(0x400c28f5c28f5c29),
        ],
        [
            f64::from_bits(0x3ff0000000000000),
            f64::from_bits(0x4015333333333333),
            f64::from_bits(0x40d2684eb851eb85),
        ],
        [
            f64::from_bits(0x3ff0000000000000),
            f64::from_bits(0x4017e147ae147ae1),
            f64::from_bits(0x406e575c28f5c28f),
        ],
    ];
    const L4: &[[f64; 3]] = &[
        [
            f64::from_bits(0x405c800000000000),
            f64::from_bits(0x400922d0e5604189),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4020000000000000),
            f64::from_bits(0x4010851eb851eb85),
            f64::from_bits(0x40b88b147ae147ae),
        ],
        [
            f64::from_bits(0x3ff0000000000000),
            f64::from_bits(0x400eb851eb851eb8),
            f64::from_bits(0x40c88b1333333333),
        ],
    ];
    const L5: &[[f64; 3]] = &[[
        f64::from_bits(0x3ff0000000000000),
        f64::from_bits(0x40091eb851eb851f),
        f64::from_bits(0x0000000000000000),
    ]];
    const R0: &[[f64; 3]] = &[
        [
            f64::from_bits(0x4197d85e94000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x41397e2c00000000),
            f64::from_bits(0x4008c9a73b42cc2d),
            f64::from_bits(0x40b88b136ae7d567),
        ],
        [
            f64::from_bits(0x40cb420000000000),
            f64::from_bits(0x40087126e978d4fe),
            f64::from_bits(0x40c88b136ae7d567),
        ],
        [
            f64::from_bits(0x40a8180000000000),
            f64::from_bits(0x4014cb4395810625),
            f64::from_bits(0x40f2f91c5810624e),
        ],
        [
            f64::from_bits(0x4099700000000000),
            f64::from_bits(0x3ff2c84b5dcc63f1),
            f64::from_bits(0x40b6796288ce703b),
        ],
        [
            f64::from_bits(0x4098a00000000000),
            f64::from_bits(0x4006c67381d7dbf5),
            f64::from_bits(0x40beb46b5dcc63f1),
        ],
        [
            f64::from_bits(0x408ce80000000000),
            f64::from_bits(0x4015cfdf3b645a1d),
            f64::from_bits(0x40c679628f5c28f6),
        ],
        [
            f64::from_bits(0x4080f00000000000),
            f64::from_bits(0x40124189374bc6a8),
            f64::from_bits(0x40aeb46b851eb852),
        ],
        [
            f64::from_bits(0x407d800000000000),
            f64::from_bits(0x400d49ba5e353f7d),
            f64::from_bits(0x40b6fced4fdf3b64),
        ],
        [
            f64::from_bits(0x4075a00000000000),
            f64::from_bits(0x3feed916872b020c),
            f64::from_bits(0x40b5838d916872b0),
        ],
        [
            f64::from_bits(0x4074900000000000),
            f64::from_bits(0x401799999999999a),
            f64::from_bits(0x40b467b1a9fbe76d),
        ],
        [
            f64::from_bits(0x4073300000000000),
            f64::from_bits(0x3fd322d0e5604189),
            f64::from_bits(0x40b5c5249ba5e354),
        ],
        [
            f64::from_bits(0x406e600000000000),
            f64::from_bits(0x4011178d4fdf3b64),
            f64::from_bits(0x40c7075083126e98),
        ],
        [
            f64::from_bits(0x406a800000000000),
            f64::from_bits(0x40176353f7ced917),
            f64::from_bits(0x4098a5604189374c),
        ],
        [
            f64::from_bits(0x4067400000000000),
            f64::from_bits(0x401416872b020c4a),
            f64::from_bits(0x40c5708a1cac0831),
        ],
        [
            f64::from_bits(0x4065e00000000000),
            f64::from_bits(0x4008189374bc6a7f),
            f64::from_bits(0x40d2684e978d4fdf),
        ],
        [
            f64::from_bits(0x405b800000000000),
            f64::from_bits(0x40143851eb851eb8),
            f64::from_bits(0x40b56ec72b020c4a),
        ],
        [
            f64::from_bits(0x4058800000000000),
            f64::from_bits(0x3fec7ae147ae147b),
            f64::from_bits(0x40b7b5c7ae147ae1),
        ],
        [
            f64::from_bits(0x4055800000000000),
            f64::from_bits(0x4016c28f5c28f5c3),
            f64::from_bits(0x40ceb46b851eb852),
        ],
        [
            f64::from_bits(0x4055800000000000),
            f64::from_bits(0x3ff451eb851eb852),
            f64::from_bits(0x4103a745851eb852),
        ],
        [
            f64::from_bits(0x4050400000000000),
            f64::from_bits(0x3fd147ae147ae148),
            f64::from_bits(0x40d0db099999999a),
        ],
        [
            f64::from_bits(0x404f800000000000),
            f64::from_bits(0x3fed70a3d70a3d71),
            f64::from_bits(0x40808d851eb851ec),
        ],
        [
            f64::from_bits(0x404c800000000000),
            f64::from_bits(0x4000147ae147ae14),
            f64::from_bits(0x40f481cd9999999a),
        ],
        [
            f64::from_bits(0x404c000000000000),
            f64::from_bits(0x4014f5c28f5c28f6),
            f64::from_bits(0x40f1706b33333333),
        ],
        [
            f64::from_bits(0x4048800000000000),
            f64::from_bits(0x400a000000000000),
            f64::from_bits(0x40a3e09eb851eb85),
        ],
        [
            f64::from_bits(0x4047800000000000),
            f64::from_bits(0x4004a3d70a3d70a4),
            f64::from_bits(0x40883c28f5c28f5c),
        ],
        [
            f64::from_bits(0x4046800000000000),
            f64::from_bits(0x401628f5c28f5c29),
            f64::from_bits(0x40c26ee147ae147b),
        ],
        [
            f64::from_bits(0x4045800000000000),
            f64::from_bits(0x40180a3d70a3d70a),
            f64::from_bits(0x40b883f5c28f5c29),
        ],
        [
            f64::from_bits(0x4043800000000000),
            f64::from_bits(0x401570a3d70a3d71),
            f64::from_bits(0x40b2560000000000),
        ],
        [
            f64::from_bits(0x4043000000000000),
            f64::from_bits(0x40031eb851eb851f),
            f64::from_bits(0x40c13db1eb851eb8),
        ],
        [
            f64::from_bits(0x4042800000000000),
            f64::from_bits(0x3fea8f5c28f5c28f),
            f64::from_bits(0x40d330c333333333),
        ],
        [
            f64::from_bits(0x4042800000000000),
            f64::from_bits(0x401399999999999a),
            f64::from_bits(0x40c7b5c666666666),
        ],
        [
            f64::from_bits(0x4042000000000000),
            f64::from_bits(0x3ffab851eb851eb8),
            f64::from_bits(0x40c7823ae147ae14),
        ],
        [
            f64::from_bits(0x4041800000000000),
            f64::from_bits(0x3ffd70a3d70a3d71),
            f64::from_bits(0x40a6fceb851eb852),
        ],
        [
            f64::from_bits(0x4040800000000000),
            f64::from_bits(0x3fceb851eb851eb8),
            f64::from_bits(0x40bbace666666666),
        ],
        [
            f64::from_bits(0x4040000000000000),
            f64::from_bits(0x3fc70a3d70a3d70a),
            f64::from_bits(0x40b3e0a147ae147b),
        ],
        [
            f64::from_bits(0x4040000000000000),
            f64::from_bits(0x3ffc7ae147ae147b),
            f64::from_bits(0x4078e26666666666),
        ],
        [
            f64::from_bits(0x403c000000000000),
            f64::from_bits(0x3ff35c28f5c28f5c),
            f64::from_bits(0x40b88e999999999a),
        ],
        [
            f64::from_bits(0x403c000000000000),
            f64::from_bits(0x3ffe666666666666),
            f64::from_bits(0x40b8878ccccccccd),
        ],
        [
            f64::from_bits(0x403a000000000000),
            f64::from_bits(0x40125c28f5c28f5c),
            f64::from_bits(0x40c467b1eb851eb8),
        ],
    ];
    const R1: &[[f64; 3]] = &[
        [
            f64::from_bits(0x40f926b000000000),
            f64::from_bits(0x3ff1b8476f2a5a47),
            f64::from_bits(0x40b88b136ae7d567),
        ],
        [
            f64::from_bits(0x409ae40000000000),
            f64::from_bits(0x3ff107c84b5dcc64),
            f64::from_bits(0x40c88b136ae7d567),
        ],
        [
            f64::from_bits(0x4085f00000000000),
            f64::from_bits(0x400922d0e5604189),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4040000000000000),
            f64::from_bits(0x3ff051eb851eb852),
            f64::from_bits(0x40d2684eb851eb85),
        ],
        [
            f64::from_bits(0x403f000000000000),
            f64::from_bits(0x4006b851eb851eb8),
            f64::from_bits(0x40b5838ccccccccd),
        ],
        [
            f64::from_bits(0x4039000000000000),
            f64::from_bits(0x3ff51eb851eb851f),
            f64::from_bits(0x40b467b0a3d70a3d),
        ],
        [
            f64::from_bits(0x4032000000000000),
            f64::from_bits(0x3ff6b851eb851eb8),
            f64::from_bits(0x4098a55c28f5c28f),
        ],
        [
            f64::from_bits(0x4024000000000000),
            f64::from_bits(0x4017a3d70a3d70a4),
            f64::from_bits(0x40c5708a3d70a3d7),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x3ff6b851eb851eb8),
            f64::from_bits(0x40b883f5c28f5c29),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x3fd147ae147ae148),
            f64::from_bits(0x40b56ec7ae147ae1),
        ],
    ];
    const R2: &[[f64; 3]] = &[
        [
            f64::from_bits(0x40b1070000000000),
            f64::from_bits(0x4017236e2eb1c433),
            f64::from_bits(0x40b88b1367a0f909),
        ],
        [
            f64::from_bits(0x405f000000000000),
            f64::from_bits(0x401650e560418937),
            f64::from_bits(0x40c88b1374bc6a7f),
        ],
        [
            f64::from_bits(0x4028000000000000),
            f64::from_bits(0x40091eb851eb851f),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x400d0a3d70a3d70a),
            f64::from_bits(0x40f2f91c51eb851f),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x3ffdeb851eb851ec),
            f64::from_bits(0x40b5c523d70a3d71),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x4015e147ae147ae1),
            f64::from_bits(0x40d2684eb851eb85),
        ],
    ];
    const R3: &[[f64; 3]] = &[
        [
            f64::from_bits(0x4062200000000000),
            f64::from_bits(0x4011178d4fdf3b64),
            f64::from_bits(0x40b88b1374bc6a7f),
        ],
        [
            f64::from_bits(0x401c000000000000),
            f64::from_bits(0x400f5c28f5c28f5c),
            f64::from_bits(0x40c88b1333333333),
        ],
    ];
    const R4: &[[f64; 3]] = &[[
        f64::from_bits(0x4010000000000000),
        f64::from_bits(0x40047ae147ae147b),
        f64::from_bits(0x40b88b147ae147ae),
    ]];
    const B: &[[f64; 3]] = &[
        [
            f64::from_bits(0x4071800000000000),
            f64::from_bits(0x4009978d4fdf3b64),
            f64::from_bits(0x40f496ea978d4fdf),
        ],
        [
            f64::from_bits(0x4059800000000000),
            f64::from_bits(0x4015b020c49ba5e3),
            f64::from_bits(0x40b5838d916872b0),
        ],
        [
            f64::from_bits(0x4054000000000000),
            f64::from_bits(0x400f0a3d70a3d70a),
            f64::from_bits(0x40b467b0a3d70a3d),
        ],
        [
            f64::from_bits(0x4046000000000000),
            f64::from_bits(0x400d99999999999a),
            f64::from_bits(0x40a261bd70a3d70a),
        ],
        [
            f64::from_bits(0x4040000000000000),
            f64::from_bits(0x4010000000000000),
            f64::from_bits(0x4098a55c28f5c28f),
        ],
        [
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x400f333333333333),
            f64::from_bits(0x40b5838ccccccccd),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x3ffbae147ae147ae),
            f64::from_bits(0x40b467b0a3d70a3d),
        ],
    ];
    const Y: [[i32; 5]; 63] = [
        [0, 0, 0, 0, 1],
        [-2, 0, 0, 2, 2],
        [0, 0, 0, 2, 2],
        [0, 0, 0, 0, 2],
        [0, 1, 0, 0, 0],
        [0, 0, 1, 0, 0],
        [-2, 1, 0, 2, 2],
        [0, 0, 0, 2, 1],
        [0, 0, 1, 2, 2],
        [-2, -1, 0, 2, 2],
        [-2, 0, 1, 0, 0],
        [-2, 0, 0, 2, 1],
        [0, 0, -1, 2, 2],
        [2, 0, 0, 0, 0],
        [0, 0, 1, 0, 1],
        [2, 0, -1, 2, 2],
        [0, 0, -1, 0, 1],
        [0, 0, 1, 2, 1],
        [-2, 0, 2, 0, 0],
        [0, 0, -2, 2, 1],
        [2, 0, 0, 2, 2],
        [0, 0, 2, 2, 2],
        [0, 0, 2, 0, 0],
        [-2, 0, 1, 2, 2],
        [0, 0, 0, 2, 0],
        [-2, 0, 0, 2, 0],
        [0, 0, -1, 2, 1],
        [0, 2, 0, 0, 0],
        [2, 0, -1, 0, 1],
        [-2, 2, 0, 2, 2],
        [0, 1, 0, 0, 1],
        [-2, 0, 1, 0, 1],
        [0, -1, 0, 0, 1],
        [0, 0, 2, -2, 0],
        [2, 0, -1, 2, 1],
        [2, 0, 1, 2, 2],
        [0, 1, 0, 2, 2],
        [-2, 1, 1, 0, 0],
        [0, -1, 0, 2, 2],
        [2, 0, 0, 2, 1],
        [2, 0, 1, 0, 0],
        [-2, 0, 2, 2, 2],
        [-2, 0, 1, 2, 1],
        [2, 0, -2, 0, 1],
        [2, 0, 0, 0, 1],
        [0, -1, 1, 0, 0],
        [-2, -1, 0, 2, 1],
        [-2, 0, 0, 0, 1],
        [0, 0, 2, 2, 1],
        [-2, 0, 2, 0, 1],
        [-2, 1, 0, 2, 1],
        [0, 0, 1, -2, 0],
        [-1, 0, 1, 0, 0],
        [-2, 1, 0, 0, 0],
        [1, 0, 0, 0, 0],
        [0, 0, 1, 2, 0],
        [0, 0, -2, 2, 2],
        [-1, -1, 1, 0, 0],
        [0, 1, 1, 0, 0],
        [0, -1, 1, 2, 2],
        [2, -1, -1, 2, 2],
        [0, 0, 3, 2, 2],
        [2, -1, 0, 2, 2],
    ];
    const PE: &[[f64; 4]] = &[
        [
            f64::from_bits(0xc104fee000000000),
            f64::from_bits(0xc065c66666666666),
            f64::from_bits(0x40f6779000000000),
            f64::from_bits(0x4021cccccccccccd),
        ],
        [
            f64::from_bits(0xc0c9c18000000000),
            f64::from_bits(0xbff999999999999a),
            f64::from_bits(0x40b6680000000000),
            f64::from_bits(0xc008cccccccccccd),
        ],
        [
            f64::from_bits(0xc0a1c40000000000),
            f64::from_bits(0xbfc999999999999a),
            f64::from_bits(0x408e880000000000),
            f64::from_bits(0xbfe0000000000000),
        ],
        [
            f64::from_bits(0x40a01c0000000000),
            f64::from_bits(0x3fc999999999999a),
            f64::from_bits(0xc08bf80000000000),
            f64::from_bits(0x3fe0000000000000),
        ],
        [
            f64::from_bits(0x4096480000000000),
            f64::from_bits(0xc00b333333333333),
            f64::from_bits(0x404b000000000000),
            f64::from_bits(0xbfb999999999999a),
        ],
        [
            f64::from_bits(0x4086400000000000),
            f64::from_bits(0x3fb999999999999a),
            f64::from_bits(0xc01c000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc080280000000000),
            f64::from_bits(0x3ff3333333333333),
            f64::from_bits(0x406c000000000000),
            f64::from_bits(0xbfe3333333333333),
        ],
        [
            f64::from_bits(0xc078200000000000),
            f64::from_bits(0xbfd999999999999a),
            f64::from_bits(0x4069000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc072d00000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4060200000000000),
            f64::from_bits(0xbfb999999999999a),
        ],
        [
            f64::from_bits(0x406b200000000000),
            f64::from_bits(0xbfe0000000000000),
            f64::from_bits(0xc057c00000000000),
            f64::from_bits(0x3fd3333333333333),
        ],
        [
            f64::from_bits(0xc063c00000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4060200000000000),
            f64::from_bits(0x3fb999999999999a),
            f64::from_bits(0xc051800000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x405ec00000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc04a800000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x404f800000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x404f800000000000),
            f64::from_bits(0x3fb999999999999a),
            f64::from_bits(0xc040800000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc04d800000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x403a000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc04d000000000000),
            f64::from_bits(0xbfb999999999999a),
            f64::from_bits(0x4040000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc049800000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x403b000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4048000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4047000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc038000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc043000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4030000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc03f000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x402a000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x403d000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x403d000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc028000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x403a000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc036000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4035000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc024000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4031000000000000),
            f64::from_bits(0xbfb999999999999a),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4030000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc020000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc030000000000000),
            f64::from_bits(0x3fb999999999999a),
            f64::from_bits(0x401c000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc02e000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4022000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc02a000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x401c000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc028000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4026000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc024000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4014000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc020000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x401c000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc01c000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc01c000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc01c000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4018000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc018000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc018000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4014000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc014000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc014000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc014000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4010000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4010000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4010000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc010000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc010000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc010000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0x4008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
        [
            f64::from_bits(0xc008000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
            f64::from_bits(0x0000000000000000),
        ],
    ];
    const FUNDAMENTAL: [[f64; 4]; 5] = [
        [
            f64::from_bits(0x3ed622f61749a783),
            f64::from_bits(0xbf5f5cbc9978e26a),
            f64::from_bits(0x411b2d4c7227d029),
            f64::from_bits(0x40729d9b13165d3a),
        ],
        [
            f64::from_bits(0xbecbf647612f3696),
            f64::from_bits(0xbf2502c683dd6fb2),
            f64::from_bits(0x40e193e19c62a1b6),
            f64::from_bits(0x407658718a86d71f),
        ],
        [
            f64::from_bits(0x3ef2a42f961f79b9),
            f64::from_bits(0x3f81cfd66c88bf57),
            f64::from_bits(0x411d203b78372e6a),
            f64::from_bits(0x4060ded0bb6ed677),
        ],
        [
            f64::from_bits(0x3ec9a1cf6e0ad913),
            f64::from_bits(0xbf6e2ac322291fb4),
            f64::from_bits(0x411d7e0811f57b42),
            f64::from_bits(0x40575166f9335d25),
        ],
        [
            f64::from_bits(0x3ec2a42f961f79b9),
            f64::from_bits(0x3f60f6c848d98da5),
            f64::from_bits(0xc09e388b8800eae2),
            f64::from_bits(0x405f42d96a6a0126),
        ],
    ];
    fn literal(address: u32) -> Ext {
        match address {
            0x42B844 => Ext::from(f64::from_bits(0x404ca5dc1a63c1e6)),
            0x42B85C => Ext::from(f64::from_bits(0x3f91df46a2529d44)),
            0x42B8B4 => Ext([11, 182, 96, 11, 182, 96, 11, 182, 246, 63]),
            0x42B8C0 => Ext::from(f32::from_bits(0x43b40000) as f64),
            0x42B8C4 => Ext::from(f32::from_bits(0x00000000) as f64),
            0x42B9DC => Ext([137, 136, 136, 136, 136, 136, 136, 136, 249, 63]),
            0x42B9E8 => Ext([171, 170, 170, 170, 170, 170, 170, 170, 250, 63]),
            0x42B9F4 => Ext::from(f32::from_bits(0x45936000) as f64),
            0x42B9F8 => Ext::from(f32::from_bits(0x43b6a000) as f64),
            0x42B9FC => Ext::from(f64::from_bits(0x403e99a027525461)),
            0x42BA04 => Ext::from(f32::from_bits(0x44be9000) as f64),
            0x42BA08 => Ext::from(f32::from_bits(0x4a0c5460) as f64),
            0x42BA0C => Ext::from(f32::from_bits(0x3e800000) as f64),
            0x42BA10 => Ext::from(f32::from_bits(0x40000000) as f64),
            0x42BA2C => Ext::from(f32::from_bits(0x4a15a164) as f64),
            0x42BA30 => Ext([154, 154, 238, 185, 224, 229, 170, 229, 239, 63]),
            0x42BA50 => Ext([183, 74, 137, 114, 6, 69, 46, 194, 238, 63]),
            0x42BA74 => Ext::from(f32::from_bits(0x4a15a164) as f64),
            0x42BA78 => Ext([154, 154, 238, 185, 224, 229, 170, 229, 239, 63]),
            0x42BA94 => Ext([205, 204, 204, 204, 204, 204, 204, 204, 251, 63]),
            0x42BB50 => Ext([253, 206, 97, 132, 17, 119, 204, 171, 228, 63]),
            0x42BD00 => Ext([253, 206, 97, 132, 17, 119, 204, 171, 228, 63]),
            0x42BDA0 => Ext::from(f32::from_bits(0x43340000) as f64),
            0x42BDA4 => Ext::from(f32::from_bits(0x43b40000) as f64),
            0x42BFC8 => Ext([67, 17, 22, 198, 180, 250, 155, 238, 229, 63]),
            0x42C04C => Ext([205, 204, 204, 204, 204, 204, 204, 204, 251, 63]),
            0x42C058 => Ext::from(f64::from_bits(0x400399999999999a)),
            0x42C060 => Ext::from(f64::from_bits(0x401728f5c28f5c29)),
            0x42C068 => Ext::from(f64::from_bits(0x403bdeb851eb851f)),
            0x42C070 => Ext::from(f64::from_bits(0x401c7ae147ae147b)),
            0x42C078 => Ext::from(f64::from_bits(0xc043866666666666)),
            0x42C080 => Ext::from(f64::from_bits(0xc06f3570a3d70a3d)),
            0x42C088 => Ext::from(f64::from_bits(0xc049b0a3d70a3d71)),
            0x42C090 => Ext::from(f32::from_bits(0x44f9e800) as f64),
            0x42C094 => Ext::from(f64::from_bits(0xbff8cccccccccccd)),
            0x42C09C => Ext::from(f64::from_bits(0xc0b248f5c28f5c29)),
            0x42C0A4 => Ext::from(f64::from_bits(0x40f499d72b020c4a)),
            0x42C0C0 => Ext([9, 248, 230, 213, 196, 179, 162, 145, 243, 63]),
            0x42C0E0 => Ext::from(f32::from_bits(0x45610000) as f64),
            0x42C0E4 => Ext::from(f64::from_bits(0xc0347d63886594af)),
            0x42C140 => Ext::from(f32::from_bits(0x4a15a164) as f64),
            0x42C144 => Ext::from(f64::from_bits(0x40768fc5362c39aa)),
            0x42C14C => Ext::from(f64::from_bits(0x4071875eb15e3164)),
            0x42C154 => Ext([251, 247, 179, 107, 155, 160, 231, 221, 229, 63]),
            0x42C160 => Ext::from(f64::from_bits(0x3f396c6f8c4c4b7f)),
            0x42C344 => Ext::from(f32::from_bits(0x45610000) as f64),
            0x42C348 => Ext::from(f64::from_bits(0x402196872b020c4a)),
            0x42C57C => Ext::from(f64::from_bits(0x3fefe488a5d668d8)),
            0x42C584 => Ext([180, 175, 207, 150, 47, 224, 88, 168, 232, 63]),
            0x42C6EC => Ext::from(f64::from_bits(0x3fd1111f0c34c1a9)),
            0x42C6F4 => Ext::from(f32::from_bits(0xbf800000) as f64),
            0x42C6F8 => Ext::from(f64::from_bits(0x401470a3d70a3d71)),
            0x42C700 => Ext::from(f64::from_bits(0x402499999999999a)),
            0x42C708 => Ext::from(f32::from_bits(0x42700000) as f64),
            0x42C70C => Ext::from(f32::from_bits(0x43888000) as f64),
            0x42C710 => Ext::from(f32::from_bits(0x438d8000) as f64),
            0x42C714 => Ext([142, 223, 63, 18, 188, 53, 198, 129, 245, 63]),
            0x42C720 => Ext::from(f64::from_bits(0x3ff051eb851eb852)),
            0x42C744 => Ext::from(f32::from_bits(0x42b40000) as f64),
            0x42C814 => Ext::from(f32::from_bits(0x43340000) as f64),
            0x42CF58 => Ext([137, 136, 136, 136, 136, 136, 136, 136, 251, 63]),
            0x42CF64 => Ext([142, 227, 56, 142, 227, 56, 142, 227, 251, 63]),
            0x42CF70 => Ext([212, 254, 255, 255, 255, 255, 255, 199, 6, 64]),
            0x42CF7C => Ext([142, 227, 56, 142, 227, 56, 142, 227, 249, 63]),
            _ => unreachable!("unknown recovered DSPA literal {address:X}"),
        }
    }
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;
    fn d(index: u16) -> Device {
        Device {
            area: Area::D,
            index,
        }
    }
    fn action(source: u16, dest: u16) -> SolarAction {
        SolarAction {
            source: d(source),
            dest: d(dest),
        }
    }
    fn float(m: &mut Memory, index: u16, value: f32) {
        m.set_double_word(d(index), value.to_bits() as i32);
    }
    fn read(m: &Memory, index: u16) -> f32 {
        f32::from_bits(m.double_word(d(index)) as u32)
    }
    fn example() -> Memory {
        let mut m = Memory::default();
        for (n, value) in [2009, 3, 23, 10, 10, 30].into_iter().enumerate() {
            m.set_word(d(n as u16), value);
        }
        for (n, value) in [
            (6, 0.0),
            (8, 8.0),
            (10, 119.192345),
            (12, 24.593456),
            (14, 132.2),
            (16, 820.0),
            (18, 15.0),
            (20, 30.0),
            (22, -10.0),
            (24, 0.5667),
        ] {
            float(&mut m, n, value);
        }
        m
    }
    #[test]
    fn parsing_preserves_real_form_and_full_documented_blocks() {
        let parse = |name: &str, s: &str, t: &str| SolarAction::parse(name, &[s.into(), t.into()]);
        assert!(parse("DSPA", "D9792", "D9992").is_ok());
        assert!(parse("DSPA", "D9793", "D200").is_err());
        assert!(parse("DSPA", "D0", "D9993").is_err());
        for name in ["SPA", "DDSPA", "DSPAP"] {
            assert!(parse(name, "D0", "D200").is_err());
        }
        assert!(
            parse("DSPA", "K0", "D200")
                .unwrap_err()
                .contains("doğrulanmadı")
        );
        assert!(parse("DSPA", "T0", "D200").is_err());
        let a = parse("DSPA", "D0", "D300").unwrap();
        assert_eq!(a.header(), 0x8301);
        let mut devices = BTreeSet::new();
        a.devices(&mut devices);
        assert!(devices.contains(&d(207)));
        assert!(devices.contains(&d(307)));
        assert!(!devices.contains(&d(208)));
    }
    #[test]
    fn manual_civil_inputs_match_the_old_installed_runtime_baseline() {
        // Independent ES2 and SS2 probes agree on these exact old-runtime
        // words. The newer PDF's 37.2394/124.7042 display remains a conflict.
        let mut m = example();
        action(0, 300).execute(&mut m);
        assert_eq!(
            (0..8)
                .map(|n| m.word(d(300 + n)) as u16)
                .collect::<Vec<_>>(),
            [36460, 16914, 13483, 17147, 2584, 16838, 814, 697]
        );
        assert_eq!(m.native_numeric_scratch, Some(0));
        assert!(m.execution_limitations.is_empty());
    }
    #[test]
    fn timezone_refraction_and_reserved_state_are_ignored_and_untouched() {
        let mut a = example();
        let mut b = a.clone();
        float(&mut b, 8, f32::NAN);
        float(&mut b, 24, f32::NEG_INFINITY);
        for n in 26..208 {
            a.set_word(d(n), n as i16);
            b.set_word(d(n), !(n as i16));
        }
        action(0, 300).execute(&mut a);
        action(0, 300).execute(&mut b);
        for n in 300..308 {
            assert_eq!(a.word(d(n)), b.word(d(n)));
        }
        for n in 26..208 {
            assert_eq!(a.word(d(n)), n as i16);
            assert_eq!(b.word(d(n)), !(n as i16));
        }
    }
    #[test]
    fn destination_alias_reads_a_snapshot_and_preserves_flags() {
        let mut expected = example();
        let mut alias = expected.clone();
        for n in [1020, 1021, 1022, 1029, 1067, 1162] {
            alias.set_system(n, true);
        }
        action(0, 300).execute(&mut expected);
        action(0, 6).execute(&mut alias);
        for n in 0..8 {
            assert_eq!(expected.word(d(300 + n)), alias.word(d(6 + n)));
        }
        for n in [1020, 1021, 1022, 1029, 1067, 1162] {
            assert!(alias.system_bit(n));
        }
    }
    #[test]
    fn model_bounds_and_guard_hold_without_inventing_plc_errors() {
        let mut m = example();
        m.data_register_limit = Some(4999);
        let a = action(0, 4993);
        m.set_word(d(4993), 1234);
        assert!(a.validate_installed_execution(&m).is_err());
        a.execute(&mut m);
        assert_eq!(m.word(d(4993)), 1234);
        assert!(!m.system_bit(1067));
        assert!(!m.execution_limitations.is_empty());
        let mut m = example();
        m.set_word(d(0), 2100);
        m.set_word(d(1), 2);
        m.set_word(d(2), 29);
        assert!(action(0, 300).validate_installed_execution(&m).is_err());
        m.set_word(d(0), 2000);
        assert!(action(0, 300).validate_installed_execution(&m).is_ok());
        float(&mut m, 18, -273.0);
        assert!(action(0, 300).validate_installed_execution(&m).is_err());
    }
    #[test]
    fn nighttime_outputs_are_not_clamped_to_manual_display_ranges() {
        let mut m = example();
        m.set_word(d(3), 0);
        action(0, 300).execute(&mut m);
        assert!(read(&m, 300) > 90.0);
        assert!(m.word(d(306)) > 2000);
    }
}
